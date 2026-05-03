"""Desktop wallpaper transition engine — WorkerW injection + Pillow ImageWin rendering.

Architecture:
  1. Locate (or create) the WorkerW desktop-layer window via the Progman 0x052C trick.
     WorkerW sits behind desktop icons but above the raw wallpaper bitmap, making it
     the correct surface for frame-accurate animation.
  2. Generate each frame on-the-fly during playback (lazy) — only one frame lives in
     RAM at a time, keeping peak usage to ~2 frames regardless of fps or duration.
  3. Blit each frame to WorkerW's GDI device context using Pillow's ImageWin.Dib —
     internally a StretchDIBits call, hardware-accelerated on all modern Windows.
  4. After playback, always sync WorkerW with the final canvas so subsequent transitions
     always start from the correct visual state (fixes stuck wallpaper after "none").
  5. Write the final BMP once and call set_wallpaper_win() to persist.

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
        with Image.open(path) as img:
            return img.convert("RGB")
    except Exception:
        return None


# ── Single-frame generators ───────────────────────────────────────────────────

def _make_fade_frame(old: Image.Image, new: Image.Image, progress: float) -> Image.Image:
    return Image.blend(old, new, progress)


def _make_slide_frame(old: Image.Image, new: Image.Image, progress: float) -> Image.Image:
    """New image slides in from the right; old exits left (viewport pan over [old|new])."""
    w, h = old.size
    offset = int(w * progress)
    frame = Image.new("RGB", (w, h))
    if w > offset:
        frame.paste(old.crop((offset, 0, w, h)), (0, 0))
    if offset > 0:
        frame.paste(new.crop((0, 0, offset, h)), (w - offset, 0))
    return frame


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
    transition: str,
    n_frames: int,
    fps: int,
) -> None:
    """Generate and blit frames lazily at the target rate.

    Only one frame is alive in memory at a time: the PIL Image is created,
    converted to a Dib, blitted, and then immediately discarded before the
    next frame is generated. Peak RAM = ~2 frames (one active + one Dib).
    """
    w, h = new.size
    frame_delay = 1.0 / fps

    hdc = user32.GetDC(worker_w)
    try:
        t_start = time.perf_counter()
        for i in range(1, n_frames + 1):
            progress = _smoothstep(i / n_frames)
            frame = (
                _make_fade_frame(old, new, progress)
                if transition == "fade"
                else _make_slide_frame(old, new, progress)
            )
            dib = ImageWin.Dib(frame)
            dib.draw(hdc, (0, 0, w, h))
            del dib, frame          # free immediately before sleeping
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

    WorkerW is always synced to *canvas* at the end of every call — including
    "none" — so that subsequent transitions always start from the correct visual
    state and the stuck-wallpaper / effect-not-changing bug cannot occur.

    Falls back gracefully if WorkerW is unavailable or the animation fails.

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
    worker_w: int | None = None
    try:
        worker_w = _find_worker_w()
    except Exception:
        pass

    # ── Run animation (skip for "none") ───────────────────────────────────────
    if transition in TRANSITIONS and transition != "none" and worker_w is not None:
        old_img = _get_current_wallpaper()
        if old_img is not None:
            if old_img.size != canvas.size:
                old_img = old_img.resize(canvas.size, Image.LANCZOS)
            n_frames = max(1, round(duration * fps))
            try:
                _play_animation(worker_w, old_img, canvas, transition, n_frames, fps)
            except Exception:
                pass  # animation failed; WorkerW sync below still runs

    # ── Persist final wallpaper ───────────────────────────────────────────────
    canvas.save(str(out), "BMP")
    set_wallpaper_fn(out)

    # ── Always sync WorkerW with the final canvas ─────────────────────────────
    # This guarantees WorkerW always reflects the current wallpaper so that:
    #   • Switching from an animated transition back to "none" shows the correct image.
    #   • Changing the image effect (normal → bw → vintage) is immediately visible.
    #   • The next transition starts from the right "old" frame visually.
    if worker_w is not None:
        try:
            _blit_to_worker_w(worker_w, canvas)
        except Exception:
            pass
