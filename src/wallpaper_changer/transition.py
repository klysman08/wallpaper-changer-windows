"""Desktop wallpaper transition engine — WorkerW injection + Pillow ImageWin rendering.

Architecture:
  1. Locate (or create) the WorkerW desktop-layer window via the Progman 0x052C trick.
     WorkerW sits behind desktop icons but above the raw wallpaper bitmap, making it
     the correct surface for frame-accurate animation.
  2. Pre-render all transition frames in memory as PIL Images (no disk I/O during playback).
  3. Blit each frame to WorkerW's GDI device context using Pillow's ImageWin.Dib —
     internally a StretchDIBits call, hardware-accelerated on all modern Windows.
  4. After playback, write the final BMP once and call set_wallpaper_win() to persist.

Supported transitions: "none", "fade", "slide"
"""
from __future__ import annotations

import ctypes
import ctypes.wintypes as wt
import time
import winreg
from pathlib import Path
from typing import Callable

from PIL import Image, ImageWin

# ── Win32 bindings ────────────────────────────────────────────────────────────

user32 = ctypes.windll.user32
WNDENUMPROC = ctypes.WINFUNCTYPE(wt.BOOL, wt.HWND, wt.LPARAM)

TRANSITIONS = ("none", "fade", "slide")

_DEFAULT_DURATION = 0.6   # seconds
_DEFAULT_FPS      = 30


# ── Easing ────────────────────────────────────────────────────────────────────

def _smoothstep(t: float) -> float:
    t = max(0.0, min(1.0, t))
    return t * t * (3.0 - 2.0 * t)


# ── Timing ────────────────────────────────────────────────────────────────────

def _sleep_until(target: float) -> None:
    """Sleep with sub-millisecond accuracy using coarse sleep + busy-wait."""
    remaining = target - time.perf_counter()
    if remaining <= 0:
        return
    if remaining > 0.003:
        time.sleep(remaining - 0.002)
    while time.perf_counter() < target:
        pass


# ── WorkerW discovery ─────────────────────────────────────────────────────────

def _find_worker_w() -> int:
    """Return the HWND of the WorkerW desktop-layer window.

    Sends the undocumented 0x052C message to Progman, which causes it to spawn
    a WorkerW child positioned behind the SHELLDLL_DefView (desktop icons).
    The same WorkerW is reused on subsequent calls — the message is idempotent.
    """
    progman = user32.FindWindowW("Progman", None)
    if not progman:
        raise RuntimeError("Progman window not found")

    result = wt.DWORD(0)
    user32.SendMessageTimeoutW(
        progman, 0x052C, 0, 0,
        0,      # SMTO_NORMAL
        1000,   # timeout ms
        ctypes.byref(result),
    )

    worker_w = wt.HWND(0)

    def _enum(hwnd: int, _lp: int) -> bool:
        if user32.FindWindowExW(hwnd, None, "SHELLDLL_DefView", None):
            found = user32.FindWindowExW(None, hwnd, "WorkerW", None)
            if found:
                worker_w.value = found
        return True

    user32.EnumWindows(WNDENUMPROC(_enum), 0)

    if not worker_w.value:
        raise RuntimeError("WorkerW desktop layer not found")

    return worker_w.value


# ── Current wallpaper ─────────────────────────────────────────────────────────

def _get_current_wallpaper() -> Image.Image | None:
    """Read the current wallpaper from the registry and return it as a PIL Image."""
    try:
        key = winreg.OpenKey(winreg.HKEY_CURRENT_USER, r"Control Panel\Desktop")
        val, _ = winreg.QueryValueEx(key, "Wallpaper")
        winreg.CloseKey(key)
        path = Path(val)
        if not path.exists():
            return None
        return Image.open(path).convert("RGB")
    except Exception:
        return None


# ── Frame builders ────────────────────────────────────────────────────────────

def _build_fade_frames(old: Image.Image, new: Image.Image, n: int) -> list[Image.Image]:
    """Cross-dissolve with smoothstep easing."""
    return [Image.blend(old, new, _smoothstep(i / n)) for i in range(1, n + 1)]


def _build_slide_frames(old: Image.Image, new: Image.Image, n: int) -> list[Image.Image]:
    """New image slides in from the right; old image exits left.

    Equivalent to panning a viewport rightward across [old | new].
    """
    w, h = old.size
    frames: list[Image.Image] = []
    for i in range(1, n + 1):
        offset = int(w * _smoothstep(i / n))
        frame = Image.new("RGB", (w, h))
        if w > offset:
            frame.paste(old.crop((offset, 0, w, h)), (0, 0))
        if offset > 0:
            frame.paste(new.crop((0, 0, offset, h)), (w - offset, 0))
        frames.append(frame)
    return frames


# ── GDI playback ──────────────────────────────────────────────────────────────

def _play_frames(
    worker_w: int,
    frames: list[Image.Image],
    fps: int,
    canvas_size: tuple[int, int],
) -> None:
    """Blit pre-rendered frames to WorkerW at the target frame rate.

    Converts each PIL Image to a Pillow ImageWin.Dib (DIB section) upfront,
    then iterates with wall-clock scheduling to avoid drift accumulation.
    """
    w, h = canvas_size
    dibs = [ImageWin.Dib(f) for f in frames]
    frame_delay = 1.0 / fps

    hdc = user32.GetDC(worker_w)
    try:
        t_start = time.perf_counter()
        for i, dib in enumerate(dibs):
            dib.draw(hdc, (0, 0, w, h))
            _sleep_until(t_start + (i + 1) * frame_delay)
    finally:
        user32.ReleaseDC(worker_w, hdc)


# ── Public API ────────────────────────────────────────────────────────────────

def apply_transition(
    canvas: Image.Image,
    out: Path,
    transition: str,
    duration: float,
    fps: int,
    set_wallpaper_fn: Callable[[Path], None],
) -> None:
    """Animate from the current wallpaper to *canvas*, then persist the result.

    Falls back to a direct apply (no animation) if WorkerW cannot be found or
    the current wallpaper is unavailable/incompatible.

    Args:
        canvas:           Final composed image (full virtual desktop size).
        out:              Destination BMP path for the persisted wallpaper.
        transition:       One of TRANSITIONS — "none", "fade", "slide".
        duration:         Total animation time in seconds.
        fps:              Target frames per second (30–60 recommended).
        set_wallpaper_fn: Callable that accepts a Path and applies it via the
                          Windows wallpaper API. Called exactly once after
                          animation completes.
    """
    if transition == "none" or transition not in TRANSITIONS:
        canvas.save(str(out), "BMP")
        set_wallpaper_fn(out)
        return

    old_img = _get_current_wallpaper()
    if old_img is None:
        canvas.save(str(out), "BMP")
        set_wallpaper_fn(out)
        return

    if old_img.size != canvas.size:
        old_img = old_img.resize(canvas.size, Image.LANCZOS)

    n_frames = max(1, round(duration * fps))

    if transition == "fade":
        frames = _build_fade_frames(old_img, canvas, n_frames)
    else:  # slide
        frames = _build_slide_frames(old_img, canvas, n_frames)

    try:
        worker_w = _find_worker_w()
        _play_frames(worker_w, frames, fps, canvas.size)
    except Exception:
        pass  # animation failed — fall through to persist final frame normally

    canvas.save(str(out), "BMP")
    set_wallpaper_fn(out)
