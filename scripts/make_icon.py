"""Generate the application icon source image.

Produces ``desktop/app-icon.png`` (1024x1024). Regenerate the platform icon set
afterwards with::

    cd desktop; bunx tauri icon app-icon.png

The mark is what the app actually does: a collage of images tiled across a screen.
Four tiles in a 2x2 grid sit on a rounded-square field, with the top-right tile
lifted and rotated to suggest the wallpaper being swapped out.

Everything is drawn at 4x and downsampled, because Pillow's draw primitives are not
antialiased on their own.
"""

from __future__ import annotations

from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

SIZE = 1024
SS = 4  # supersampling factor
S = SIZE * SS

# Deep indigo field, so the tile colours carry the eye.
BG_TOP = (46, 42, 88)
BG_BOTTOM = (24, 22, 48)

# Warm-to-cool spread; reads as "photographs" without being literal.
TILES = [
    ((250, 176, 92), (236, 118, 92)),    # amber
    ((122, 208, 176), (66, 160, 158)),   # teal
    ((140, 152, 240), (104, 110, 214)),  # periwinkle
    ((236, 128, 176), (186, 96, 176)),   # rose
]


def vertical_gradient(size: int, top: tuple, bottom: tuple) -> Image.Image:
    """A one-pixel-wide gradient stretched to a square."""
    strip = Image.new("RGB", (1, size))
    pixels = strip.load()
    for y in range(size):
        t = y / max(1, size - 1)
        pixels[0, y] = tuple(round(a + (b - a) * t) for a, b in zip(top, bottom))
    return strip.resize((size, size), Image.BILINEAR)


def rounded_mask(size: tuple[int, int], radius: int) -> Image.Image:
    mask = Image.new("L", size, 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, size[0] - 1, size[1] - 1], radius, fill=255)
    return mask


def tile(size: tuple[int, int], colors: tuple, radius: int) -> Image.Image:
    art = vertical_gradient(max(size), *colors).resize(size, Image.BILINEAR)
    art.putalpha(rounded_mask(size, radius))
    return art


def build() -> Image.Image:
    canvas = Image.new("RGBA", (S, S), (0, 0, 0, 0))

    field = vertical_gradient(S, BG_TOP, BG_BOTTOM).convert("RGBA")
    field.putalpha(rounded_mask((S, S), int(S * 0.22)))
    canvas.alpha_composite(field)

    # Grid geometry: generous margin so the mark survives being scaled to 16px.
    margin = int(S * 0.17)
    gap = int(S * 0.045)
    span = S - margin * 2
    cell = (span - gap) // 2

    positions = [
        (margin, margin),
        (margin + cell + gap, margin),
        (margin, margin + cell + gap),
        (margin + cell + gap, margin + cell + gap),
    ]
    radius = int(cell * 0.16)

    for index, (pos, colors) in enumerate(zip(positions, TILES)):
        # The top-right tile is the one being swapped: lifted, tilted, shadowed.
        if index == 1:
            art = tile((cell, cell), colors, radius)
            art = art.rotate(-9, resample=Image.BICUBIC, expand=True)

            shadow = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
            shadow.paste(
                Image.new("RGBA", art.size, (0, 0, 0, 150)),
                (pos[0] - int(cell * 0.06), pos[1] - int(cell * 0.02) + int(cell * 0.05)),
                art,
            )
            canvas.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(int(S * 0.012))))
            canvas.alpha_composite(art, (pos[0] - int(cell * 0.06), pos[1] - int(cell * 0.02)))
        else:
            canvas.alpha_composite(tile((cell, cell), colors, radius), pos)

    return canvas.resize((SIZE, SIZE), Image.LANCZOS)


def main() -> None:
    out = Path(__file__).resolve().parent.parent / "desktop" / "app-icon.png"
    icon = build()
    icon.save(out)
    print(f"wrote {out} ({icon.width}x{icon.height})")

    # A quick legibility check at the sizes Windows actually renders.
    preview_sizes = [16, 32, 48, 128, 256]
    strip = Image.new("RGBA", (sum(preview_sizes) + 20 * len(preview_sizes), 256), (30, 30, 34, 255))
    x = 10
    for size in preview_sizes:
        strip.alpha_composite(icon.resize((size, size), Image.LANCZOS), (x, (256 - size) // 2))
        x += size + 20
    preview = out.parent / "app-icon-preview.png"
    strip.save(preview)
    print(f"wrote {preview}")


if __name__ == "__main__":
    main()
