"""Desktop wallpaper application — native Windows fade.

Earlier versions hand-rendered a cross-dissolve, first in Pillow on the CPU and then
via a GPU/DWM layered overlay. Both turned out to be redundant: on Windows 11, applying
a wallpaper with ``SystemParametersInfo`` already performs its *own* GPU-composited
crossfade. Running a second custom animation on top produced a visible "double fade"
(new -> revert to old -> fade again) and, for the Pillow version, heavy CPU load.

So the transition is simply the operating system's native wallpaper fade now: write the
composed image and let ``set_wallpaper_fn`` apply it. Zero extra CPU, one clean fade,
and nothing for us to synchronize.
"""
from __future__ import annotations

from pathlib import Path
from typing import Callable

from PIL import Image


def apply_transition(
    canvas: Image.Image,
    out: Path,
    set_wallpaper_fn: Callable[[Path], None],
) -> None:
    """Persist *canvas* and apply it as the desktop wallpaper.

    The visual transition is Windows' own native wallpaper fade, performed inside
    ``set_wallpaper_fn`` (``SystemParametersInfo``). There is no custom animation.
    """
    canvas.save(str(out), "BMP")
    set_wallpaper_fn(out)
