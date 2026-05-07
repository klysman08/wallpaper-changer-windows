"""Desktop wallpaper transition engine — WorkerW injection + Pillow ImageWin rendering.

Architecture:
  1. Locate (or create) the WorkerW desktop-layer window via the Progman 0x052C trick.
     WorkerW sits behind desktop icons but above the raw wallpaper bitmap, making it
     the correct surface for frame-accurate animation.
  2. Generate each frame on-the-fly during playback (lazy) — only one frame lives in
     RAM at a time, keeping peak usage to ~2 frames regardless of fps or duration.
  3. Blit each frame to WorkerW's GDI device context using Pillow's ImageWin.Dib —
     internally a StretchDIBits call, hardware-accelerated on all modern Windows.
  4. WorkerW is ALWAYS synced to the final canvas BEFORE set_wallpaper_fn is called.
     set_wallpaper_fn uses SPIF_SENDWININICHANGE which sends a synchronous broadcast
     to Explorer; Explorer may crossfade and/or invalidate WorkerW during that call.
     By painting WorkerW first, the Windows system crossfade runs behind WorkerW
     (invisible) and the correct image is already on screen from our blit.

Supported transitions: "none", "fade", "slide", "slide_up", "zoom"
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

TRANSITIONS = ("none", "fade", "slide", "slide_up", "zoom")

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
        with Image.open(path) as img:
            return img.convert("RGB")
    except Exception:
        return None


# ── Per-frame generators ──────────────────────────────────────────────────────

def _make_fade_frame(old: Image.Image, new: Image.Image, progress: float) -> Image.Image:
    """Cross-dissolve with smoothstep easing."""
    return Image.blend(old, new, progress)


def _make_slide_frame(old: Image.Image, new: Image.Image, progress: float) -> Image.Image:
    """New image slides in from the right; old exits left (horizontal pan over [old|new])."""
    w, h = old.size
    offset = int(w * progress)
    frame = Image.new("RGB", (w, h))
    if w > offset:
        frame.paste(old.crop((offset, 0, w, h)), (0, 0))
    if offset > 0:
        frame.paste(new.crop((0, 0, offset, h)), (w - offset, 0))
    return frame


def _make_slide_up_frame(old: Image.Image, new: Image.Image, progress: float) -> Image.Image:
    """New image slides in from the bottom; old exits top (vertical pan over [new|old])."""
    w, h = old.size
    offset = int(h * progress)
    frame = Image.new("RGB", (w, h))
    if h > offset:
        frame.paste(old.crop((0, offset, w, h)), (0, 0))
    if offset > 0:
        frame.paste(new.crop((0, 0, w, offset)), (0, h - offset))
    return frame


def _make_zoom_frame(old: Image.Image, new: Image.Image, progress: float) -> Image.Image:
    """New image zooms in from center (scales 0 → 100%) while fading in over old."""
    w, h = old.size
    scale = max(0.02, progress)
    new_w = max(1, int(w * scale))
    new_h = max(1, int(h * scale))
    scaled = new.resize((new_w, new_h), Image.LANCZOS)
    x = (w - new_w) // 2
    y = (h - new_h) // 2
    frame = old.copy()
    mask = Image.new("L", (new_w, new_h), int(255 * progress))
    frame.paste(scaled, (x, y), mask)
    return frame


_FRAME_MAKERS = {
    "fade":     _make_fade_frame,
    "slide":    _make_slide_frame,
    "slide_up": _make_slide_up_frame,
    "zoom":     _make_zoom_frame,
}


# ── WorkerW drawing ───────────────────────────────────────────────────────────

def _blit_to_worker_w(worker_w: int, img: Image.Image) -> None:
    """Blit a single PIL Image to WorkerW's GDI device context."""
    w, h = img.size
    hdc = user32.GetDC(worker_w)
    try:
        dib = ImageWin.Dib(img)
        dib.draw(hdc, (0, 0, w, h))
    finally:
        user32.ReleaseDC(worker_w, hdc)


def _play_animation(
    worker_w: int,
    old: Image.Image,
    new: Image.Image,
    make_frame: Callable[[Image.Image, Image.Image, float], Image.Image],
    n_frames: int,
    fps: int,
) -> None:
    """Generate and blit frames lazily at the target rate.

    Only one frame is alive in memory at a time: built, converted to a Dib,
    blitted, then immediately discarded. Peak RAM ≈ 2 frames.
    """
    w, h = new.size
    frame_delay = 1.0 / fps

    hdc = user32.GetDC(worker_w)
    try:
        t_start = time.perf_counter()
        for i in range(1, n_frames + 1):
            progress = _smoothstep(i / n_frames)
            frame = make_frame(old, new, progress)
            dib = ImageWin.Dib(frame)
            dib.draw(hdc, (0, 0, w, h))
            del dib, frame
            _sleep_until(t_start + i * frame_delay)
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

    Critical ordering guarantee: WorkerW is ALWAYS painted with *canvas* BEFORE
    set_wallpaper_fn() is called. set_wallpaper_fn broadcasts SPIF_SENDWININICHANGE
    synchronously to Explorer, which may apply a system-level crossfade and/or
    invalidate WorkerW during the call. Painting WorkerW first ensures the new
    canvas is already on screen, masking any system animation.

    Falls back gracefully if WorkerW is unavailable or the animation fails.
    """
    worker_w: int | None = None
    try:
        worker_w = _find_worker_w()
    except Exception:
        pass

    # ── Animated transition ───────────────────────────────────────────────────
    make_frame = _FRAME_MAKERS.get(transition)
    if make_frame is not None and transition != "none" and worker_w is not None:
        old_img = _get_current_wallpaper()
        if old_img is not None:
            if old_img.size != canvas.size:
                old_img = old_img.resize(canvas.size, Image.LANCZOS)
            n_frames = max(1, round(duration * fps))
            try:
                _play_animation(worker_w, old_img, canvas, make_frame, n_frames, fps)
            except Exception:
                pass

    # ── Sync WorkerW to final canvas BEFORE persisting ────────────────────────
    # Must happen before set_wallpaper_fn so the Windows system crossfade
    # (triggered by SPIF_SENDWININICHANGE inside set_wallpaper_fn) runs behind
    # WorkerW and is invisible to the user.
    if worker_w is not None:
        try:
            _blit_to_worker_w(worker_w, canvas)
        except Exception:
            pass

    # ── Persist ───────────────────────────────────────────────────────────────
    canvas.save(str(out), "BMP")
    set_wallpaper_fn(out)
