"""Prepare the application icon from the project logo.

Stands alone. The project carries no Python environment since the engine was ported
to Rust, so run this with an interpreter that has Pillow — ``uv run --no-project
--with pillow python scripts/make_icon.py`` needs nothing installed.

Reads ``assets/icon/wpaper-logo.png`` and writes ``desktop/app-icon.png``
(1024x1024 RGBA), which is what Tauri's icon generator wants as its source.
Regenerate the platform icon set afterwards with::

    cd desktop; bunx tauri icon app-icon.png

The logo is a transparent cutout rather than a full-bleed square, so the work here
is squaring it, upscaling to the size the generator expects, and leaving a small
margin — Windows and macOS both round or mask icon corners, and artwork that runs
to the very edge loses its outline to that mask.
"""

from __future__ import annotations

from pathlib import Path

from PIL import Image

SIZE = 1024

# Fraction of the canvas left empty on every side.
PADDING = 0.04

REPO_ROOT = Path(__file__).resolve().parent.parent
SOURCE = REPO_ROOT / "assets" / "icon" / "wpaper-logo.png"


def build(source: Path) -> Image.Image:
    logo = Image.open(source).convert("RGBA")

    # Crop to what is actually drawn: any transparent border in the source would
    # otherwise eat into the margin budget and shrink the mark.
    bbox = logo.getbbox()
    if bbox:
        logo = logo.crop(bbox)

    inner = round(SIZE * (1 - PADDING * 2))
    scale = inner / max(logo.width, logo.height)
    resized = logo.resize(
        (max(1, round(logo.width * scale)), max(1, round(logo.height * scale))),
        Image.LANCZOS,
    )

    canvas = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    canvas.alpha_composite(
        resized, ((SIZE - resized.width) // 2, (SIZE - resized.height) // 2)
    )
    return canvas


def main() -> None:
    if not SOURCE.exists():
        raise SystemExit(f"logo not found: {SOURCE}")

    icon = build(SOURCE)
    out = REPO_ROOT / "desktop" / "app-icon.png"
    icon.save(out)
    print(f"wrote {out} ({icon.width}x{icon.height})")

    # The webview uses its own copy for the sidebar mark and the favicon.
    web = REPO_ROOT / "desktop" / "public" / "icon.png"
    icon.resize((256, 256), Image.LANCZOS).save(web)
    print(f"wrote {web}")

    # A quick legibility check at the sizes Windows actually renders.
    preview_sizes = [16, 32, 48, 128, 256]
    strip = Image.new(
        "RGBA", (sum(preview_sizes) + 20 * len(preview_sizes), 256), (30, 30, 34, 255)
    )
    x = 10
    for size in preview_sizes:
        thumb = icon.resize((size, size), Image.LANCZOS)
        strip.alpha_composite(thumb, (x, (256 - size) // 2))
        x += size + 20
    preview = out.parent / "app-icon-preview.png"
    strip.save(preview)
    print(f"wrote {preview}")


if __name__ == "__main__":
    main()
