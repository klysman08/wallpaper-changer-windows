"""Win32 window transparency helpers using ctypes."""
from __future__ import annotations

import ctypes
import ctypes.wintypes as wt
from typing import List, Tuple

# ── Win32 Constants ──────────────────────────────────────────────────────────
GWL_EXSTYLE = -20
WS_EX_LAYERED = 0x00080000
LWA_ALPHA = 0x00000002

# ── Win32 Function Bindings ──────────────────────────────────────────────────
user32 = ctypes.windll.user32

GetForegroundWindow = user32.GetForegroundWindow
GetForegroundWindow.restype = wt.HWND

SetWindowLongW = user32.SetWindowLongW
SetWindowLongW.argtypes = [wt.HWND, ctypes.c_int, ctypes.c_long]
SetWindowLongW.restype = ctypes.c_long

GetWindowLongW = user32.GetWindowLongW
GetWindowLongW.argtypes = [wt.HWND, ctypes.c_int]
GetWindowLongW.restype = ctypes.c_long

SetLayeredWindowAttributes = user32.SetLayeredWindowAttributes
SetLayeredWindowAttributes.argtypes = [
    wt.HWND, wt.COLORREF, wt.BYTE, wt.DWORD,
]
SetLayeredWindowAttributes.restype = wt.BOOL

IsWindowVisible = user32.IsWindowVisible
IsWindowVisible.argtypes = [wt.HWND]
IsWindowVisible.restype = wt.BOOL

GetWindowTextW = user32.GetWindowTextW
GetWindowTextW.argtypes = [wt.HWND, ctypes.c_wchar_p, ctypes.c_int]
GetWindowTextW.restype = ctypes.c_int

GetWindowTextLengthW = user32.GetWindowTextLengthW
GetWindowTextLengthW.argtypes = [wt.HWND]
GetWindowTextLengthW.restype = ctypes.c_int

EnumWindows = user32.EnumWindows
WNDENUMPROC = ctypes.WINFUNCTYPE(wt.BOOL, wt.HWND, wt.LPARAM)
EnumWindows.argtypes = [WNDENUMPROC, wt.LPARAM]
EnumWindows.restype = wt.BOOL

# DwmGetWindowAttribute – used to detect cloaked (hidden) UWP windows
try:
    _dwmapi = ctypes.windll.dwmapi
    DwmGetWindowAttribute = _dwmapi.DwmGetWindowAttribute
    DwmGetWindowAttribute.argtypes = [
        wt.HWND, wt.DWORD, ctypes.c_void_p, wt.DWORD,
    ]
    DwmGetWindowAttribute.restype = ctypes.c_long
    DWMWA_CLOAKED = 14
    _HAS_DWM = True
except Exception:
    _HAS_DWM = False


# ── Public API ───────────────────────────────────────────────────────────────

def get_foreground_window() -> int:
    """Return the HWND of the currently focused foreground window."""
    return GetForegroundWindow()


def set_window_opacity(hwnd: int, alpha: int = 255) -> None:
    """Apply *alpha* (0–255) opacity to the window identified by *hwnd*.

    The function first ensures the ``WS_EX_LAYERED`` extended style is set,
    then calls ``SetLayeredWindowAttributes`` to apply the opacity.

    If *alpha* is 255 (fully opaque), the layered flag is removed so the
    window returns to its normal rendering path.
    """
    alpha = max(0, min(255, alpha))

    if alpha < 255:
        ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE)
        if not (ex_style & WS_EX_LAYERED):
            SetWindowLongW(hwnd, GWL_EXSTYLE, ex_style | WS_EX_LAYERED)
        SetLayeredWindowAttributes(hwnd, 0, alpha, LWA_ALPHA)
    else:
        # Remove layered flag to restore full opacity
        ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE)
        if ex_style & WS_EX_LAYERED:
            SetWindowLongW(hwnd, GWL_EXSTYLE, ex_style & ~WS_EX_LAYERED)


def _is_cloaked(hwnd: int) -> bool:
    """Return True if the window is *cloaked* (invisible UWP/system window)."""
    if not _HAS_DWM:
        return False
    cloaked = ctypes.c_int(0)
    hr = DwmGetWindowAttribute(
        hwnd, DWMWA_CLOAKED, ctypes.byref(cloaked), ctypes.sizeof(cloaked),
    )
    return hr == 0 and cloaked.value != 0


def _get_window_title(hwnd: int) -> str:
    """Return the title-bar text of the given window."""
    length = GetWindowTextLengthW(hwnd)
    if length == 0:
        return ""
    buf = ctypes.create_unicode_buffer(length + 1)
    GetWindowTextW(hwnd, buf, length + 1)
    return buf.value


def list_visible_windows() -> List[Tuple[int, str]]:
    """Enumerate all visible, titled windows, filtering out system noise.

    Returns a sorted list of ``(hwnd, title)`` tuples.
    """
    results: list[tuple[int, str]] = []

    def _callback(hwnd: int, _lp: int) -> bool:
        if not IsWindowVisible(hwnd):
            return True
        if _is_cloaked(hwnd):
            return True
        title = _get_window_title(hwnd)
        if not title or title in {
            "Program Manager",
            "Windows Input Experience",
            "Settings",
            "MSCTFIME UI",
        }:
            return True
        results.append((hwnd, title))
        return True

    EnumWindows(WNDENUMPROC(_callback), 0)
    results.sort(key=lambda t: t[1].lower())
    return results
