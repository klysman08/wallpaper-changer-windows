"""Headless JSON-RPC engine driven over stdio.

This is the adapter that lets a non-Python front end (the Tauri + shadcn desktop
shell in ``desktop/``) drive the existing engine. It adds no wallpaper logic of its
own — every method here is a thin wrapper over the modules that already exist, so
the Win32 behaviour the GUI relied on is byte-for-byte the same.

Protocol — newline-delimited JSON, one object per line:

    request   {"id": 1, "method": "get_config", "params": {}}
    response  {"id": 1, "ok": true,  "result": {...}}
    error     {"id": 1, "ok": false, "error": {"type": "...", "message": "..."}}
    event     {"event": "video_status", "data": {...}}          (unsolicited)

Events carry no ``id``: they originate from engine threads (video status, the
rotation timer), not from a request.

**stdout is the protocol channel and nothing else.** Any stray ``print`` from a
dependency would corrupt the stream, so ``main()`` swaps ``sys.stdout`` for stderr
and keeps the real handle private. Log with ``logging`` (stderr) when debugging.
"""

from __future__ import annotations

import base64
import io
import json
import logging
import sys
import tempfile
import threading
import traceback
from pathlib import Path
from typing import Any, Callable

from . import i18n, scroll_transparency, startup, transparency
from .config import (
    get_default_config_path,
    load_config,
    resolve_output_dir,
    save_config,
)
from .image_utils import list_images
from .monitor import get_monitors, get_virtual_desktop_size
from .notifications import send_windows_notification
from .video_wallpaper import VideoWallpaperPlayer, has_mpv, scan_video_folder
from .wallpaper import EFFECTS, apply_single_wallpaper, apply_wallpaper, compose_collage

log = logging.getLogger(__name__)

PROTOCOL_VERSION = 1

# Preview must never disturb the real rotation history, so image selection during a
# preview is journalled to a throwaway state file instead of ``state.json``.
_PREVIEW_STATE = Path(tempfile.gettempdir()) / "wallpaper_changer_preview_state.json"

# How many applied image sets the "previous wallpaper" hotkey can step back through.
_HISTORY_LIMIT = 50

# Opacity the transparency hotkey toggles to (half of fully opaque).
_HALF_OPACITY = 128
_FULLY_OPAQUE = 255


class RpcError(Exception):
    """An error that is expected and should reach the client as a clean envelope."""

    def __init__(self, message: str, kind: str = "error") -> None:
        super().__init__(message)
        self.kind = kind


class Engine:
    """Holds the process-wide state the front end drives.

    One instance per process. Methods map 1:1 onto RPC method names via
    :meth:`dispatch`; each returns a JSON-serialisable value.
    """

    def __init__(self, emit: Callable[[str, dict], None]) -> None:
        self._emit = emit
        self._cfg: dict = {}
        self._video = VideoWallpaperPlayer()
        self._watch_timer: threading.Timer | None = None
        self._watch_lock = threading.Lock()
        self._apply_lock = threading.Lock()
        # Image sets applied so far, oldest first, so the "previous wallpaper"
        # hotkey can step back through them. Bounded to keep memory flat.
        self._history: list[list[str]] = []
        self._history_idx = -1
        # Modifier+wheel transparency. The listener edits real windows, so it is
        # started explicitly rather than on import.
        self._scroll = scroll_transparency.ScrollTransparency(
            on_change=lambda data: self._emit("transparency_changed", data)
        )

    # ── Config ────────────────────────────────────────────────────────────────

    def _config(self) -> dict:
        if not self._cfg:
            self._cfg = load_config()
        return self._cfg

    def _output_dir(self) -> Path:
        out = resolve_output_dir(self._config())
        out.mkdir(parents=True, exist_ok=True)
        return out

    def get_config(self) -> dict:
        cfg = dict(self._config())
        return {
            "config": {k: v for k, v in cfg.items() if not k.startswith("_")},
            "config_path": cfg.get("_config_path", str(get_default_config_path())),
        }

    def save_config(self, config: dict) -> dict:
        """Persist *config* and adopt it as the live configuration."""
        merged = dict(config)
        merged["_config_path"] = self._config().get("_config_path", str(get_default_config_path()))
        # The session flags describe what is running *now*, and a hotkey or the tray
        # can have flipped them since the client read the config. Taking the client's
        # copy would let a stale draft switch rotation back off on the next launch.
        merged["general"] = {
            **merged.get("general", {}),
            "rotation_active": self.watch_status()["watching"],
        }
        merged["video"] = {
            **merged.get("video", {}),
            "enabled": self._video.is_running(),
        }
        save_config(merged)
        self._cfg = merged
        lang = merged.get("general", {}).get("language")
        if lang:
            i18n.set_language(lang)
        # The scroll listener holds a system-wide hook, so it has to follow the
        # saved settings immediately rather than waiting for a restart.
        self.sync_scroll_transparency()
        return {"saved": True, "config_path": merged["_config_path"]}

    def _remember(self, section: str, key: str, value: object) -> None:
        """Write one session flag straight to ``settings.toml``.

        Rotation and video playback are toggled from four places (the window, the
        tray, a global hotkey, the CLI), and the next launch is supposed to come up
        the way the user left it. Persisting on the toggle — rather than waiting for
        an explicit Save — is what makes that true even if the window is never opened.
        """
        cfg = self._config()
        current = cfg.setdefault(section, {})
        if current.get(key) == value:
            return
        current[key] = value
        try:
            save_config(cfg)
        except OSError as exc:
            # Losing the flag only costs us the restore on the next launch; the
            # running session is unaffected, so this must not fail the call.
            log.warning("Could not persist %s.%s: %s", section, key, exc)

    def restore_session(self) -> dict:
        """Bring back what was running when the app was last closed.

        Called once at startup. Each half is independent: no videos (or no libmpv)
        must not stop the rotation timer from coming back.
        """
        cfg = self._config()
        restored = {"rotation": False, "video": False}
        if bool(cfg.get("general", {}).get("rotation_active", False)):
            try:
                self.watch_start()
                restored["rotation"] = True
            except Exception as exc:
                log.warning("Could not restore the rotation timer: %s", exc)
        if bool(cfg.get("video", {}).get("enabled", False)) and has_mpv():
            try:
                self.video_start()
                restored["video"] = True
            except Exception as exc:
                log.warning("Could not restore the video wallpaper: %s", exc)
        return restored

    # ── Environment ───────────────────────────────────────────────────────────

    def get_monitors(self) -> dict:
        mons = get_monitors()
        width, height = get_virtual_desktop_size(mons) if mons else (0, 0)
        return {
            "monitors": [
                {
                    "index": m.index,
                    "x": m.x,
                    "y": m.y,
                    "width": m.width,
                    "height": m.height,
                }
                for m in mons
            ],
            "virtual_width": width,
            "virtual_height": height,
        }

    def get_translations(self) -> dict:
        return {
            "translations": i18n.get_translations(),
            "supported": i18n.SUPPORTED_LANGUAGES,
            "default": i18n.DEFAULT_LANGUAGE,
            "current": i18n.get_language(),
        }

    def get_capabilities(self) -> dict:
        return {
            "protocol": PROTOCOL_VERSION,
            "effects": list(EFFECTS),
            "has_mpv": has_mpv(),
            "startup_enabled": startup.is_startup_enabled(),
            "has_scroll_transparency": scroll_transparency.is_available(),
        }

    def list_folder_images(self, folder: str) -> dict:
        images = list_images(folder)
        return {
            "count": len(images),
            "images": [str(p) for p in images],
        }

    # ── Wallpaper ─────────────────────────────────────────────────────────────

    def apply_wallpaper(
        self, config: dict | None = None, images: list[str] | None = None
    ) -> dict:
        """Compose and apply the collage. *config* overrides the saved settings.

        Pass *images* — the set a preview reported — to apply exactly what is on
        screen instead of picking a fresh selection.
        """
        cfg = self._merged(config)
        if not self._apply_lock.acquire(blocking=False):
            raise RpcError("An apply is already running.", "busy")
        try:
            monitors = get_monitors()
            out, images = apply_wallpaper(
                cfg, monitors, self._output_dir(), preset_images=images or None
            )
        finally:
            self._apply_lock.release()
        self._push_history(images)
        result = {"output": str(out), "images": images}
        self._emit("wallpaper_applied", result)
        return result

    def apply_previous_wallpaper(self, config: dict | None = None) -> dict:
        """Re-apply the image set shown before the current one.

        Mirrors the old GUI's back navigation: the selection is replayed exactly
        rather than re-picked, so stepping back is deterministic.
        """
        if self._history_idx <= 0:
            raise RpcError("No previous wallpaper in history.", "no_history")
        cfg = self._merged(config)
        target = self._history_idx - 1
        images = self._history[target]
        if not self._apply_lock.acquire(blocking=False):
            raise RpcError("An apply is already running.", "busy")
        try:
            out, _ = apply_wallpaper(
                cfg, get_monitors(), self._output_dir(), preset_images=images
            )
        finally:
            self._apply_lock.release()
        self._history_idx = target
        result = {"output": str(out), "images": images}
        self._emit("wallpaper_applied", result)
        return result

    def set_effect(self, effect: str) -> dict:
        """Switch the visual effect and re-apply immediately.

        Bound to the effect hotkeys. The change is live but not persisted — saving
        stays an explicit action, as it was in the GUI.
        """
        if effect not in EFFECTS:
            raise RpcError(f"Unknown effect: {effect}", "invalid")
        cfg = self._config()
        cfg.setdefault("display", {})["effect"] = effect
        result = self.apply_wallpaper()
        result["effect"] = effect
        return result

    def _push_history(self, images: list[str]) -> None:
        """Record an applied selection, dropping any entries ahead of the cursor."""
        del self._history[self._history_idx + 1 :]
        self._history.append(list(images))
        if len(self._history) > _HISTORY_LIMIT:
            self._history.pop(0)
        self._history_idx = len(self._history) - 1

    def apply_default_wallpaper(self, config: dict | None = None) -> dict:
        cfg = self._merged(config)
        raw = cfg.get("paths", {}).get("default_wallpaper", "")
        if not raw:
            raise RpcError("No default wallpaper is configured.", "not_configured")
        path = Path(raw)
        if not path.exists():
            raise RpcError(f"Default wallpaper not found: {path}", "not_found")
        out = apply_single_wallpaper(
            path,
            get_monitors(),
            self._output_dir(),
            fit_mode=cfg.get("display", {}).get("fit_mode", "fill"),
            effect=cfg.get("display", {}).get("effect", "normal"),
        )
        result = {"output": str(out), "images": [str(path)]}
        self._emit("wallpaper_applied", result)
        return result

    def preview(
        self,
        config: dict | None = None,
        max_width: int = 960,
        images: list[str] | None = None,
    ) -> dict:
        """Render the collage to a base64 PNG **without** touching the desktop.

        Pass ``images`` (from a previous preview) to re-render the same selection
        under different settings — that keeps effect/fit tweaks from reshuffling the
        picture on every slider move.
        """
        cfg = self._merged(config)
        monitors = get_monitors()
        if not monitors:
            raise RpcError("No monitors detected.", "no_monitors")
        canvas, used = compose_collage(
            cfg, monitors, preset_images=images, state_file=_PREVIEW_STATE
        )
        if max_width and canvas.width > max_width:
            ratio = max_width / canvas.width
            canvas = canvas.resize(
                (max_width, max(1, int(canvas.height * ratio)))
            )
        buf = io.BytesIO()
        canvas.save(buf, "PNG", optimize=False, compress_level=1)
        return {
            "png_base64": base64.b64encode(buf.getvalue()).decode("ascii"),
            "width": canvas.width,
            "height": canvas.height,
            "images": used,
        }

    def _merged(self, config: dict | None) -> dict:
        """Overlay *config* on the saved configuration without persisting it."""
        base = dict(self._config())
        if not config:
            return base
        merged = dict(base)
        for section, values in config.items():
            if section.startswith("_"):
                continue
            if isinstance(values, dict) and isinstance(merged.get(section), dict):
                merged[section] = {**merged[section], **values}
            else:
                merged[section] = values
        return merged

    # ── Rotation timer ────────────────────────────────────────────────────────

    def watch_start(self, interval: int | None = None) -> dict:
        cfg = self._config()
        secs = max(1, int(interval or cfg.get("general", {}).get("interval", 300)))
        with self._watch_lock:
            self._cancel_watch_locked()
            self._schedule_watch_locked(secs)
        self._remember("general", "rotation_active", True)
        return {"watching": True, "interval": secs}

    def watch_stop(self) -> dict:
        with self._watch_lock:
            self._cancel_watch_locked()
        self._remember("general", "rotation_active", False)
        return {"watching": False}

    def watch_status(self) -> dict:
        with self._watch_lock:
            return {"watching": self._watch_timer is not None}

    def _schedule_watch_locked(self, secs: int) -> None:
        timer = threading.Timer(secs, self._watch_tick, args=(secs,))
        timer.daemon = True
        self._watch_timer = timer
        timer.start()

    def _cancel_watch_locked(self) -> None:
        if self._watch_timer is not None:
            self._watch_timer.cancel()
            self._watch_timer = None

    def _watch_tick(self, secs: int) -> None:
        try:
            self.apply_wallpaper()
        except Exception as exc:  # a failed tick must not kill the rotation
            log.warning("Scheduled wallpaper apply failed: %s", exc)
            self._emit("error", {"source": "watch", "message": str(exc)})
        with self._watch_lock:
            if self._watch_timer is not None:  # still watching → queue the next tick
                self._schedule_watch_locked(secs)

    # ── Transparency ──────────────────────────────────────────────────────────

    def list_windows(self) -> dict:
        return {
            "windows": [
                {"hwnd": hwnd, "title": title, "process": process}
                for hwnd, title, process in transparency.list_visible_windows()
            ]
        }

    def set_window_opacity(self, hwnd: int, alpha: int) -> dict:
        transparency.set_window_opacity(int(hwnd), max(0, min(255, int(alpha))))
        return {"hwnd": int(hwnd), "alpha": int(alpha)}

    def get_foreground_window(self) -> dict:
        return {"hwnd": transparency.get_foreground_window()}

    def toggle_foreground_opacity(self) -> dict:
        """Toggle the focused window between half opacity and fully opaque.

        Bound to the transparency hotkey. Keyed by process name (not window handle)
        so the choice survives the window being closed and reopened, and persisted
        straight away — a shortcut that forgot its own effect would be useless.
        """
        hwnd = transparency.get_foreground_window()
        if not hwnd:
            raise RpcError("No focused window.", "not_found")
        process = transparency._get_process_name_for_hwnd(hwnd)
        if not process:
            raise RpcError("Could not identify the focused window.", "not_found")

        settings = transparency.load_opacity_settings()
        current = settings.get(process, _FULLY_OPAQUE)
        alpha = _FULLY_OPAQUE if current < _FULLY_OPAQUE else _HALF_OPACITY
        transparency.set_window_opacity(hwnd, alpha)
        settings[process] = alpha
        transparency.save_opacity_settings(settings)
        return {"hwnd": hwnd, "process": process, "alpha": alpha}

    def get_opacity_settings(self) -> dict:
        return {"settings": transparency.load_opacity_settings()}

    def save_opacity_settings(self, settings: dict) -> dict:
        transparency.save_opacity_settings({k: int(v) for k, v in settings.items()})
        return {"saved": True}

    def reapply_opacity_settings(self) -> dict:
        return {"applied": transparency.reapply_saved_settings()}

    # ── Modifier + wheel transparency ─────────────────────────────────────────

    def sync_scroll_transparency(self) -> dict:
        """Match the listener to the saved settings.

        Called at start-up and after every save, so turning the switch off really
        removes the hook rather than leaving it installed until the next restart.
        """
        hotkeys = self._config().get("hotkeys", {})
        modifier = scroll_transparency.normalize_modifier(
            hotkeys.get("scroll_modifier")
        )
        if hotkeys.get("scroll_enabled"):
            self._scroll.start(modifier)
        else:
            self._scroll.stop()
        return self.scroll_transparency_status()

    def scroll_transparency_status(self) -> dict:
        """Report what the listener is actually doing.

        ``enabled`` is what the config asks for; ``running`` is whether the hook is
        installed. They differ when the hook could not be installed at all, which
        is the case the interface needs to surface.
        """
        status = self._scroll.status()
        hotkeys = self._config().get("hotkeys", {})
        status["enabled"] = bool(hotkeys.get("scroll_enabled"))
        status["modifiers"] = list(scroll_transparency.SUPPORTED_MODIFIERS)
        status["step"] = scroll_transparency.STEP
        return status

    # ── Video wallpaper ───────────────────────────────────────────────────────

    def scan_videos(self, folder: str) -> dict:
        videos = scan_video_folder(folder)
        return {"count": len(videos), "videos": [str(p) for p in videos]}

    def video_start(self, config: dict | None = None) -> dict:
        if not has_mpv():
            raise RpcError("libmpv is not available.", "no_mpv")
        cfg = self._merged(config)
        video_cfg = cfg.get("video", {})
        videos = scan_video_folder(video_cfg.get("folder", ""))
        if not videos:
            raise RpcError("No videos found in the configured folder.", "not_found")
        monitors = get_monitors()
        if not monitors:
            raise RpcError("No monitors detected.", "no_monitors")
        self._video.configure(
            videos,
            loop=bool(video_cfg.get("loop", True)),
            sound=bool(video_cfg.get("sound", False)),
            monitors=monitors,
            on_status=lambda name: self._emit("video_status", {"current": name}),
        )
        self._video.start()
        self._remember("video", "enabled", True)
        return self.video_status()

    def video_stop(self) -> dict:
        self._video.stop()
        self._remember("video", "enabled", False)
        return self.video_status()

    def video_next(self) -> dict:
        self._video.next_video()
        return self.video_status()

    def video_prev(self) -> dict:
        self._video.prev_video()
        return self.video_status()

    def video_set_sound(self, enabled: bool) -> dict:
        self._video.set_sound(bool(enabled))
        return self.video_status()

    def video_toggle(self, config: dict | None = None) -> dict:
        """Start the video wallpaper if stopped, stop it if running."""
        if self._video.is_running():
            return self.video_stop()
        return self.video_start(config)

    def video_toggle_sound(self) -> dict:
        """Flip the audio state, live if something is playing."""
        enabled = not bool(self._config().get("video", {}).get("sound", False))
        self._config().setdefault("video", {})["sound"] = enabled
        if self._video.is_running():
            self._video.set_sound(enabled)
        return {**self.video_status(), "sound": enabled}

    def watch_toggle(self) -> dict:
        """Start the rotation timer if idle, stop it if running."""
        with self._watch_lock:
            running = self._watch_timer is not None
        return self.watch_stop() if running else self.watch_start()

    def video_status(self) -> dict:
        return {
            "running": self._video.is_running(),
            "current": self._video.current_name(),
            "has_mpv": has_mpv(),
        }

    # ── Windows integration ───────────────────────────────────────────────────

    def get_startup_enabled(self) -> dict:
        return {"enabled": startup.is_startup_enabled()}

    def set_startup_enabled(self, enabled: bool) -> dict:
        startup.set_startup_enabled(bool(enabled))
        return {"enabled": startup.is_startup_enabled()}

    def notify(self, title: str, message: str) -> dict:
        send_windows_notification(title, message)
        return {"sent": True}

    def ping(self) -> dict:
        return {"pong": True, "protocol": PROTOCOL_VERSION}

    def shutdown(self) -> dict:
        """Release desktop-layer resources before the process exits.

        Video host windows are children of WORKERW; leaving them behind would strand
        them on the desktop until Explorer restarts, so this must run on every exit
        path the front end controls.
        """
        with self._watch_lock:
            self._cancel_watch_locked()
        try:
            self._video.stop()
        except Exception as exc:
            log.warning("Video teardown failed during shutdown: %s", exc)
        # Removes the system-wide mouse hook and flushes any debounced save.
        try:
            self._scroll.stop()
        except Exception as exc:
            log.warning("Scroll listener teardown failed during shutdown: %s", exc)
        return {"bye": True}

    # ── Dispatch ──────────────────────────────────────────────────────────────

    _METHODS = (
        "ping",
        "get_capabilities",
        "get_config",
        "save_config",
        "get_monitors",
        "get_translations",
        "list_folder_images",
        "apply_wallpaper",
        "apply_default_wallpaper",
        "apply_previous_wallpaper",
        "set_effect",
        "preview",
        "watch_start",
        "watch_stop",
        "watch_status",
        "watch_toggle",
        "list_windows",
        "set_window_opacity",
        "get_foreground_window",
        "toggle_foreground_opacity",
        "get_opacity_settings",
        "save_opacity_settings",
        "reapply_opacity_settings",
        "scroll_transparency_status",
        "sync_scroll_transparency",
        "scan_videos",
        "video_start",
        "video_stop",
        "video_next",
        "video_prev",
        "video_set_sound",
        "video_toggle",
        "video_toggle_sound",
        "video_status",
        "get_startup_enabled",
        "set_startup_enabled",
        "notify",
        "shutdown",
    )

    def dispatch(self, method: str, params: dict[str, Any]) -> Any:
        if method not in self._METHODS:
            raise RpcError(f"Unknown method: {method}", "unknown_method")
        try:
            return getattr(self, method)(**params)
        except TypeError as exc:
            # A signature mismatch is a caller bug, not an engine failure.
            raise RpcError(f"Bad params for {method}: {exc}", "bad_params") from exc


def _error_payload(exc: BaseException) -> dict:
    if isinstance(exc, RpcError):
        return {"type": exc.kind, "message": str(exc)}
    if isinstance(exc, FileNotFoundError):
        return {"type": "not_found", "message": str(exc)}
    if isinstance(exc, (ValueError, KeyError)):
        return {"type": "invalid", "message": str(exc)}
    return {
        "type": "internal",
        "message": str(exc) or exc.__class__.__name__,
        "traceback": traceback.format_exc(),
    }


def serve(stdin, stdout) -> int:
    """Run the request loop until *stdin* closes. Returns a process exit code."""
    write_lock = threading.Lock()

    def write(obj: dict) -> None:
        line = json.dumps(obj, ensure_ascii=False)
        with write_lock:
            stdout.write(line + "\n")
            stdout.flush()

    def emit(event: str, data: dict) -> None:
        write({"event": event, "data": data})

    engine = Engine(emit)
    # Restore the mouse hook if the user left it on. A failure here must not stop
    # the engine from serving: everything else still works without it.
    try:
        engine.sync_scroll_transparency()
    except Exception as exc:
        log.warning("Could not start scroll transparency: %s", exc)
    emit("ready", {"protocol": PROTOCOL_VERSION})

    # Come back up the way the user left it. On a thread because starting the video
    # wallpaper spins up mpv and reparents windows, which must not delay the first
    # request the shell sends.
    def _restore() -> None:
        try:
            emit("session_restored", engine.restore_session())
        except Exception as exc:
            log.warning("Could not restore the previous session: %s", exc)

    threading.Thread(target=_restore, name="restore-session", daemon=True).start()

    for raw_line in stdin:
        # Strip a stray BOM as well as whitespace: callers that reopen the stream
        # mid-flight can re-emit one, and it is never valid inside the protocol.
        line = raw_line.strip().lstrip("﻿")
        if not line:
            continue
        try:
            request = json.loads(line)
        except json.JSONDecodeError as exc:
            write({
                "id": None,
                "ok": False,
                "error": {"type": "parse", "message": str(exc)},
            })
            continue

        req_id = request.get("id")
        method = request.get("method", "")
        params = request.get("params") or {}
        if not isinstance(params, dict):
            write({
                "id": req_id,
                "ok": False,
                "error": {"type": "bad_params", "message": "params must be an object"},
            })
            continue

        try:
            result = engine.dispatch(method, params)
            write({"id": req_id, "ok": True, "result": result})
        except Exception as exc:
            log.debug("RPC method %s failed", method, exc_info=True)
            write({"id": req_id, "ok": False, "error": _error_payload(exc)})

        if method == "shutdown":
            break

    engine.shutdown()
    return 0


def main() -> int:
    logging.basicConfig(
        level=logging.INFO,
        stream=sys.stderr,
        format="%(levelname)s %(name)s: %(message)s",
    )
    real_stdout = sys.stdout
    try:
        real_stdout.reconfigure(encoding="utf-8", newline="\n")
        # utf-8-sig, not utf-8: Windows tooling (PowerShell pipes in particular)
        # prefixes a UTF-8 BOM, which would otherwise break the first request.
        sys.stdin.reconfigure(encoding="utf-8-sig")
    except AttributeError:  # pragma: no cover - non-standard stream objects
        pass
    # Protect the protocol channel: anything that prints now lands on stderr.
    sys.stdout = sys.stderr
    try:
        return serve(sys.stdin, real_stdout)
    finally:
        sys.stdout = real_stdout


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
