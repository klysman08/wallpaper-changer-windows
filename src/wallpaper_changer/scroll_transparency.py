"""Hold a modifier and scroll to fade the window under the cursor.

Ported from the old ttkbootstrap GUI, which ran the same listener in-process. The
engine owns it now, because the engine is what touches Win32; the Tauri shell only
turns it on and off.

The modifier is checked with ``GetAsyncKeyState`` rather than tracked by the
listener: the wheel event tells us nothing about which keys are held, and polling
the key state at the moment of the scroll is both simpler and immune to missing a
key-up while the app was not focused.

Two things this does that the old implementation did not:

- the process name is cached per window handle, since resolving it opens a process
  handle and a fast scroll fires dozens of events a second
- persistence is debounced, so a burst of ticks writes the JSON once instead of
  once per tick
"""

from __future__ import annotations

import ctypes
import logging
import threading
from collections.abc import Callable

from . import transparency

log = logging.getLogger(__name__)

#: Alpha applied per wheel notch.
STEP = 5

#: Never scroll a window past invisible-in-practice. Matches the slider's floor in
#: the UI, so anything reached by scrolling can also be dragged back.
MIN_ALPHA = 20
MAX_ALPHA = 255

#: How long to wait after the last tick before writing settings to disk.
_SAVE_DEBOUNCE = 0.6

#: Window handles to remember process names for. Bounded so a long session that
#: cycles through many windows cannot grow the cache without limit.
_CACHE_LIMIT = 64

#: Modifier name -> the virtual-key codes that count as "held".
#: Windows has separate left/right keys; either satisfies the modifier.
_MODIFIER_VK: dict[str, tuple[int, ...]] = {
    "alt": (0x12,),               # VK_MENU
    "ctrl": (0x11,),              # VK_CONTROL
    "shift": (0x10,),             # VK_SHIFT
    "win": (0x5B, 0x5C),          # VK_LWIN, VK_RWIN
}

DEFAULT_MODIFIER = "alt"

#: The choices the interface offers, in the order it should list them.
SUPPORTED_MODIFIERS = ("alt", "ctrl", "shift", "win")


def normalize_modifier(name: str | None) -> str:
    """Return a supported modifier name, falling back to the default."""
    key = (name or "").strip().lower()
    aliases = {"control": "ctrl", "windows": "win", "super": "win", "meta": "win"}
    key = aliases.get(key, key)
    return key if key in _MODIFIER_VK else DEFAULT_MODIFIER


def is_available() -> bool:
    """Whether the mouse hook can be installed at all."""
    # Deliberately local: pynput is optional at runtime, and a frozen build that
    # missed the hidden import must degrade to "unavailable" rather than refusing
    # to start the whole engine.
    try:
        import pynput.mouse  # noqa: F401, PLC0415
    except Exception:
        return False
    return True


def modifier_is_down(modifier: str) -> bool:
    """Whether *modifier* is currently held."""
    codes = _MODIFIER_VK.get(normalize_modifier(modifier), ())
    get_state = ctypes.windll.user32.GetAsyncKeyState
    return any(get_state(vk) & 0x8000 for vk in codes)


def next_alpha(current: int, notches: float) -> int:
    """Alpha after scrolling *notches* from *current*, clamped to the usable range.

    Scrolling up makes the window more opaque, which is the direction people
    expect from "more".
    """
    return max(MIN_ALPHA, min(MAX_ALPHA, int(current) + int(notches) * STEP))


class ScrollTransparency:
    """Owns the mouse listener thread and the opacity map it edits."""

    def __init__(self, on_change: Callable[[dict], None] | None = None) -> None:
        self._on_change = on_change
        self._listener = None
        self._lock = threading.Lock()
        self._modifier = DEFAULT_MODIFIER
        self._settings: dict[str, int] = {}
        self._save_timer: threading.Timer | None = None
        # hwnd -> process name. Cleared whenever the listener restarts, so a
        # recycled handle from a closed window cannot leak across sessions.
        self._process_cache: dict[int, str] = {}

    # ── Lifecycle ─────────────────────────────────────────────────────────────

    @property
    def running(self) -> bool:
        return self._listener is not None

    @property
    def modifier(self) -> str:
        return self._modifier

    def start(self, modifier: str = DEFAULT_MODIFIER) -> bool:
        """Install the hook. Returns whether it is running afterwards.

        Restarts cleanly if already running with a different modifier.
        """
        modifier = normalize_modifier(modifier)
        with self._lock:
            if self._listener is not None and modifier == self._modifier:
                return True
        self.stop()

        try:
            from pynput import mouse  # noqa: PLC0415 - optional, see is_available
        except Exception:
            log.info("pynput is unavailable; scroll transparency stays off")
            return False

        with self._lock:
            self._modifier = modifier
            self._settings = transparency.load_opacity_settings()
            self._process_cache.clear()

        try:
            listener = mouse.Listener(on_scroll=self._on_scroll)
            listener.daemon = True
            listener.start()
        except Exception:
            log.exception("could not install the scroll listener")
            return False

        with self._lock:
            self._listener = listener
        log.info("scroll transparency active on %s+wheel", modifier)
        return True

    def stop(self) -> None:
        """Remove the hook and flush any pending save."""
        with self._lock:
            listener, self._listener = self._listener, None
            timer, self._save_timer = self._save_timer, None
        if timer is not None:
            timer.cancel()
            self._flush()
        if listener is not None:
            try:
                listener.stop()
            except Exception:
                log.debug("scroll listener did not stop cleanly", exc_info=True)

    def status(self) -> dict:
        return {
            "running": self.running,
            "modifier": self._modifier,
            "available": is_available(),
        }

    def reload_settings(self) -> None:
        """Re-read the opacities from disk.

        Transitional, for the Rust port. ``self._settings`` is loaded once at
        ``start()``, and the native core now owns every other write to that file — so
        without this the next debounced ``_flush`` would put a stale snapshot back
        over a setting the user had just saved from the window.

        Goes away with the sidecar, when the hook itself moves across.
        """
        with self._lock:
            self._settings = transparency.load_opacity_settings()

    # ── Hook ──────────────────────────────────────────────────────────────────

    def _on_scroll(self, _x: int, _y: int, _dx: int, dy: int) -> None:
        # Runs on the hook thread: keep it short and never let it raise, or the
        # listener dies and the feature silently stops working. The whole body is
        # inside the try for exactly that reason.
        try:  # noqa: PLW0717
            if not dy or not modifier_is_down(self._modifier):
                return

            hwnd = transparency.get_foreground_window()
            if not hwnd:
                return
            process = self._process_for(hwnd)
            if not process:
                return

            with self._lock:
                current = self._settings.get(process, MAX_ALPHA)
                alpha = next_alpha(current, dy)
                if alpha == current:
                    return
                self._settings[process] = alpha

            transparency.set_window_opacity(hwnd, alpha)
            self._schedule_save()

            if self._on_change is not None:
                self._on_change({"hwnd": hwnd, "process": process, "alpha": alpha})
        except Exception:
            log.exception("scroll transparency handler failed")

    def _process_for(self, hwnd: int) -> str:
        cached = self._process_cache.get(hwnd)
        if cached is not None:
            return cached
        name = transparency._get_process_name_for_hwnd(hwnd)
        if name:
            if len(self._process_cache) > _CACHE_LIMIT:
                self._process_cache.clear()
            self._process_cache[hwnd] = name
        return name

    # ── Persistence ───────────────────────────────────────────────────────────

    def _schedule_save(self) -> None:
        with self._lock:
            if self._save_timer is not None:
                self._save_timer.cancel()
            timer = threading.Timer(_SAVE_DEBOUNCE, self._flush)
            timer.daemon = True
            self._save_timer = timer
        timer.start()

    def _flush(self) -> None:
        with self._lock:
            self._save_timer = None
            snapshot = dict(self._settings)
        if snapshot:
            transparency.save_opacity_settings(snapshot)
