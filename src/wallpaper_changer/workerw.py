"""Shared discovery of the WORKERW desktop-layer window.

WORKERW sits *behind* the desktop icons (SHELLDLL_DefView) but *above* the raw
wallpaper bitmap. It is the correct surface for rendering live content (video) and
frame-accurate transitions so that they appear behind the desktop icons.

Windows 10 and Windows 11 expose WORKERW differently, and some recent Windows 11
builds paint the wallpaper directly from Progman with no separate WorkerW child.
``get_workerw`` therefore tries several strategies, and ``get_desktop_parent``
falls back to Progman itself so callers always have a window to embed into.

All ``ctypes`` calls set explicit ``argtypes``/``restype`` so 64-bit window handles
are not truncated (the classic cause of intermittent "WorkerW not found" failures).
"""
from __future__ import annotations

import ctypes
import ctypes.wintypes as wt
import time

user32 = ctypes.windll.user32

WNDENUMPROC = ctypes.WINFUNCTYPE(wt.BOOL, wt.HWND, wt.LPARAM)

# Undocumented message asking Progman to spawn the WorkerW behind the icons.
_WM_SPAWN_WORKERW = 0x052C
_SMTO_NORMAL = 0x0000

# ── Correct 64-bit handle handling ────────────────────────────────────────────
user32.FindWindowW.restype = wt.HWND
user32.FindWindowW.argtypes = [wt.LPCWSTR, wt.LPCWSTR]
user32.FindWindowExW.restype = wt.HWND
user32.FindWindowExW.argtypes = [wt.HWND, wt.HWND, wt.LPCWSTR, wt.LPCWSTR]
user32.EnumWindows.argtypes = [WNDENUMPROC, wt.LPARAM]
user32.EnumWindows.restype = wt.BOOL


def _spawn_workerw(progman: int) -> None:
    """Ask Progman to create the WorkerW layer. Idempotent — safe to repeat."""
    result = ctypes.c_size_t(0)
    user32.SendMessageTimeoutW(
        progman, _WM_SPAWN_WORKERW, 0, 0, _SMTO_NORMAL, 1000, ctypes.byref(result)
    )


def get_workerw() -> int | None:
    """Return the HWND of the WORKERW desktop layer, or ``None`` if unavailable.

    Strategy, in order:
      1. WorkerW as a *direct child* of Progman (Windows 11).
      2. A top-level WorkerW that is the sibling following the window hosting
         ``SHELLDLL_DefView`` (Windows 10 style).
    """
    progman = user32.FindWindowW("Progman", None)
    if not progman:
        return None

    _spawn_workerw(progman)
    # Brief pause so the shell can materialise the WorkerW child window.
    time.sleep(0.1)

    # ── Windows 11: WorkerW is a direct child of Progman ──────────────────────
    child = user32.FindWindowExW(progman, None, "WorkerW", None)
    if child:
        return int(child)

    # ── Windows 10: WorkerW is the top-level sibling of the DefView host ──────
    found = wt.HWND(0)

    def _enum(hwnd: int, _lparam: int) -> bool:
        if user32.FindWindowExW(hwnd, None, "SHELLDLL_DefView", None):
            sibling = user32.FindWindowExW(None, hwnd, "WorkerW", None)
            if sibling:
                found.value = sibling
        return True

    user32.EnumWindows(WNDENUMPROC(_enum), 0)
    return int(found.value) if found.value else None


def get_desktop_parent() -> int | None:
    """Return a window suitable as the parent for desktop-layer embedding.

    Prefers WORKERW; falls back to Progman itself, which on some Windows 11 builds
    paints the wallpaper directly with no separate WorkerW child. Returns ``None``
    only when the desktop shell (Explorer) is not running.
    """
    worker = get_workerw()
    if worker:
        return worker
    progman = user32.FindWindowW("Progman", None)
    return int(progman) if progman else None
