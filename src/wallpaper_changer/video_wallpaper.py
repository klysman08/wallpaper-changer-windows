"""Video wallpaper via libmpv rendered into WORKERW-embedded host windows.

Architecture:
  1. ``get_desktop_parent()`` resolves the WORKERW desktop layer (the surface behind
     the desktop icons) — see ``workerw.py``.
  2. For each monitor a borderless **host window** (a predefined ``static`` control,
     so no custom WndProc is needed) is created *as a child of WORKERW* and positioned
     at the monitor's offset within the WORKERW client area.
  3. A libmpv instance is attached to each host window via its ``wid`` option. mpv
     renders the video directly into the window with GPU decoding and handles looping
     and audio natively — no frames are ever decoded in Python.

This replaces an earlier OpenCV (``cv2.imshow``) approach whose HighGUI windows were
not designed for embedding (flicker, focus theft, CPU-only decode).

libmpv is an optional native dependency (``libmpv-2.dll``). When it is unavailable the
feature degrades gracefully: ``has_mpv()`` returns ``False`` and callers disable the UI.
"""
from __future__ import annotations

import ctypes
import ctypes.wintypes as wt
import logging
import os
import sys
import threading
from pathlib import Path
from typing import Callable

from .workerw import get_desktop_parent

log = logging.getLogger(__name__)

VIDEO_EXTENSIONS = {".mp4", ".mkv", ".avi", ".mov", ".wmv", ".webm", ".m4v"}

user32 = ctypes.windll.user32

# ── Win32 constants for borderless child host windows ─────────────────────────
_WS_CHILD = 0x40000000
_WS_VISIBLE = 0x10000000
_WS_EX_NOACTIVATE = 0x08000000

user32.CreateWindowExW.restype = wt.HWND
user32.CreateWindowExW.argtypes = [
    wt.DWORD, wt.LPCWSTR, wt.LPCWSTR, wt.DWORD,
    ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int,
    wt.HWND, wt.HMENU, wt.HINSTANCE, wt.LPVOID,
]
user32.DestroyWindow.argtypes = [wt.HWND]
user32.IsWindow.argtypes = [wt.HWND]
user32.IsWindow.restype = wt.BOOL
user32.GetWindowRect.argtypes = [wt.HWND, ctypes.POINTER(wt.RECT)]
user32.InvalidateRect.argtypes = [wt.HWND, ctypes.c_void_p, wt.BOOL]
user32.UpdateWindow.argtypes = [wt.HWND]


# ── libmpv discovery ──────────────────────────────────────────────────────────

# One-shot guard (a list avoids a module-level ``global`` statement).
_libmpv_prepared: list[bool] = []

# Hold the handles returned by os.add_dll_directory for the process lifetime.
# Per the os.add_dll_directory contract, the returned object owns the search-path
# entry; keeping a reference guarantees the directory stays resolvable.
_dll_dir_handles: list = []


def _prepare_libmpv() -> None:
    """Make likely ``libmpv-2.dll`` locations discoverable before ``import mpv``.

    python-mpv loads libmpv at *import* time via ``ctypes.util.find_library``, which
    on Windows scans ``%PATH%`` — *not* the ``os.add_dll_directory`` list. So each
    candidate directory is both registered with ``add_dll_directory`` (for libmpv's
    own dependent DLLs) and prepended to ``%PATH%`` (so find_library locates it).

    Looks at ``MPV_DLL_DIR`` and a vendored ``libmpv/`` folder beside the project root
    (and, when frozen, beside the executable). Missing directories are ignored.
    """
    if _libmpv_prepared:
        return
    _libmpv_prepared.append(True)

    candidates: list[Path] = []
    # When frozen by PyInstaller, bundled binaries live in sys._MEIPASS
    # (the _internal folder); python-mpv finds the DLL via %PATH%, not _MEIPASS.
    meipass = getattr(sys, "_MEIPASS", None)
    if meipass:
        candidates.append(Path(meipass))
    env_dir = os.environ.get("MPV_DLL_DIR")
    if env_dir:
        candidates.append(Path(env_dir))
    try:
        from .config import get_project_root

        root = get_project_root()
        candidates.append(root / "libmpv")
        candidates.append(root)
    except Exception:
        pass

    for directory in candidates:
        try:
            if not directory.is_dir():
                continue
            _dll_dir_handles.append(os.add_dll_directory(str(directory)))
            # python-mpv finds the DLL via %PATH%, so prepend the directory there too.
            os.environ["PATH"] = f"{directory}{os.pathsep}{os.environ.get('PATH', '')}"
        except Exception:
            pass


def has_mpv() -> bool:
    """Return True when python-mpv and libmpv are importable/loadable."""
    _prepare_libmpv()
    try:
        import mpv  # noqa: F401
    except (ImportError, OSError):
        return False
    return True


# ── Folder scanning ───────────────────────────────────────────────────────────

def scan_video_folder(folder: str | Path) -> list[Path]:
    """Return a sorted list of video files in *folder* (empty if it is not a dir)."""
    p = Path(folder)
    if not p.is_dir():
        return []
    return sorted(
        f for f in p.iterdir()
        if f.is_file() and f.suffix.lower() in VIDEO_EXTENSIONS
    )


# ── Host window helpers ───────────────────────────────────────────────────────

def _window_origin(hwnd: int) -> tuple[int, int]:
    """Return the screen-space top-left of *hwnd* (used to map monitor coords)."""
    rect = wt.RECT()
    user32.GetWindowRect(hwnd, ctypes.byref(rect))
    return rect.left, rect.top


def _create_host_window(
    parent: int, mon, parent_left: int, parent_top: int
) -> int | None:
    """Create a borderless child ``static`` window covering *mon* inside *parent*.

    Coordinates are relative to *parent*'s client area, so the monitor's screen
    position is offset by the parent window's top-left corner.
    """
    child_x = mon.x - parent_left
    child_y = mon.y - parent_top
    hwnd = user32.CreateWindowExW(
        _WS_EX_NOACTIVATE,
        "static", None,
        _WS_CHILD | _WS_VISIBLE,
        child_x, child_y, mon.width, mon.height,
        parent, None, None, None,
    )
    if not hwnd:
        log.warning("CreateWindowExW failed for monitor %s (GLE=%d)",
                    getattr(mon, "index", "?"), ctypes.get_last_error())
        return None
    return int(hwnd)


def _destroy_window(hwnd: int | None) -> None:
    """Destroy a host window if it still exists.

    Safe to call with ``None`` or a stale handle: the WORKERW layer can be recreated
    by Explorer (display changes, shell restart), which destroys our child windows
    out from under us, so the handle may already be invalid.
    """
    if not hwnd:
        return
    try:
        if user32.IsWindow(hwnd):
            user32.DestroyWindow(hwnd)
    except Exception:
        pass


def _refresh_desktop(parent: int | None) -> None:
    """Force the desktop layer to repaint so no last video frame stays frozen."""
    if not parent:
        return
    try:
        user32.InvalidateRect(parent, None, True)
        user32.UpdateWindow(parent)
    except Exception:
        pass


# ── Player ────────────────────────────────────────────────────────────────────

class VideoWallpaperPlayer:
    """Play a video playlist as the desktop wallpaper via libmpv + WORKERW windows.

    libmpv runs its own decode/render threads once a file is loaded, so this class
    holds no playback loop of its own: ``start()`` creates the windows and mpv
    instances, ``stop()`` tears them down.
    """

    def __init__(self) -> None:
        self._videos: list[Path] = []
        self._loop = True
        self._sound = False
        self._monitors: list = []
        self._on_status: Callable[[str], None] | None = None

        self._players: list = []          # list[mpv.MPV]
        self._hwnds: list[int] = []
        self._parent: int | None = None
        self._running = False
        self._lock = threading.Lock()

    # ── Configuration ─────────────────────────────────────────────────────────

    def configure(
        self,
        videos: list[Path],
        *,
        loop: bool = True,
        sound: bool = False,
        monitors: list,
        on_status: Callable[[str], None] | None = None,
    ) -> None:
        self._videos = list(videos)
        self._loop = loop
        self._sound = sound
        self._monitors = monitors
        self._on_status = on_status

    # ── Lifecycle ─────────────────────────────────────────────────────────────

    def start(self) -> None:
        with self._lock:
            if self._running:
                self._stop_locked()

            if not self._videos:
                raise RuntimeError("No videos configured.")
            if not self._monitors:
                raise RuntimeError("No monitors detected.")

            _prepare_libmpv()
            try:
                import mpv
            except (ImportError, OSError) as exc:
                raise RuntimeError(
                    "libmpv / python-mpv is not available. Install python-mpv and "
                    "place libmpv-2.dll on the path."
                ) from exc

            try:
                self._start_locked(mpv)
            except Exception:
                # Never leave half-created windows or un-terminated mpv instances
                # behind — a window destroyed while mpv still renders into it crashes.
                self._stop_locked()
                raise

        if self._videos:
            self._notify(self._videos[0].name)

    def _start_locked(self, mpv) -> None:
        """Create the host windows and mpv instances. Caller must hold ``_lock``."""
        parent = get_desktop_parent()
        if not parent:
            raise RuntimeError(
                "Could not find the desktop layer (WORKERW/Progman). "
                "Is Explorer running?"
            )
        self._parent = parent
        parent_left, parent_top = _window_origin(parent)

        first = True
        for mon in self._monitors:
            hwnd = _create_host_window(parent, mon, parent_left, parent_top)
            if not hwnd:
                continue
            try:
                player = self._make_player(mpv, hwnd, audio=self._sound and first)
            except Exception as exc:
                log.warning("Failed to start mpv on monitor %s: %s",
                            getattr(mon, "index", "?"), exc)
                _destroy_window(hwnd)
                continue
            self._players.append(player)
            self._hwnds.append(hwnd)
            first = False

        if not self._players:
            raise RuntimeError(
                "No video host windows could be created on the desktop layer."
            )
        self._running = True

    def _make_player(self, mpv, hwnd: int, *, audio: bool):
        """Create one mpv instance bound to *hwnd* and load the playlist."""
        player = mpv.MPV(
            wid=str(int(hwnd)),
            vo="gpu",
            hwdec="auto",
            keepaspect=True,                        # preserve aspect ratio (letterbox)
            loop_playlist="inf" if self._loop else "no",
            mute=not audio,
            osc=False,
            input_default_bindings=False,
            input_vo_keyboard=False,
        )
        try:
            for i, video in enumerate(self._videos):
                player.loadfile(str(video), mode="replace" if i == 0 else "append")
        except Exception:
            # The instance is already bound to hwnd. Terminate it before the caller
            # destroys the window, otherwise mpv keeps rendering into a dead HWND.
            try:
                player.terminate()
            except Exception:
                pass
            raise
        return player

    def stop(self) -> None:
        with self._lock:
            self._stop_locked()

    def _stop_locked(self) -> None:
        # Terminate every mpv instance BEFORE destroying its window, so mpv never
        # renders into a freed HWND (which would crash the process natively).
        for player in self._players:
            try:
                player.terminate()
            except Exception:
                pass
        self._players.clear()
        for hwnd in self._hwnds:
            _destroy_window(hwnd)
        self._hwnds.clear()
        _refresh_desktop(self._parent)
        self._parent = None
        self._running = False

    def is_running(self) -> bool:
        return self._running

    # ── Live controls ─────────────────────────────────────────────────────────

    def set_sound(self, enabled: bool) -> None:
        """Toggle audio live. Only the first instance ever carries audio (no echo)."""
        self._sound = enabled
        with self._lock:
            for i, player in enumerate(self._players):
                try:
                    player.mute = not (enabled and i == 0)
                except Exception:
                    pass

    def next_video(self) -> str:
        """Skip to the next video in the folder (wraps). Returns its name."""
        return self._step(1)

    def prev_video(self) -> str:
        """Skip to the previous video in the folder (wraps). Returns its name."""
        return self._step(-1)

    def _step(self, direction: int) -> str:
        """Move every monitor's playlist by *direction* and re-sync them.

        Navigation is driven through mpv's ``playlist_pos`` so all monitors jump to
        the same index (re-syncing them) and the exact target name is known. Wrapping
        works regardless of the loop setting, which is what users expect from an
        explicit next/previous action.
        """
        with self._lock:
            players = list(self._players)
            videos = list(self._videos)
        n = len(videos)
        if not players or n == 0:
            return videos[0].name if videos else ""
        try:
            current = players[0].playlist_pos or 0
        except Exception:
            current = 0
        target = (current + direction) % n
        for player in players:
            try:
                player.playlist_pos = target
            except Exception:
                pass
        name = videos[target].name
        self._notify(name)
        return name

    def current_name(self) -> str:
        """Name of the video currently playing (reflects mpv's real position)."""
        with self._lock:
            players = list(self._players)
            videos = list(self._videos)
        if players and videos:
            try:
                pos = players[0].playlist_pos
                if pos is not None and 0 <= pos < len(videos):
                    return videos[pos].name
            except Exception:
                pass
        return videos[0].name if videos else ""

    # ── Internals ─────────────────────────────────────────────────────────────

    def _notify(self, msg: str) -> None:
        if self._on_status:
            try:
                self._on_status(msg)
            except Exception:
                pass
