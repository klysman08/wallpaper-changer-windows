"""Differential harness: Rust composition vs Pillow, one stage at a time.

Run from the repository root:

    uv run python tests/differential/compare.py

Deliberately three separate comparisons rather than one. A single number over the
finished collage mixes resampler noise with effect arithmetic, and you cannot tell
which is wrong:

  * plan_collage geometry  -- pure integer maths, so ZERO tolerance
  * apply_effect           -- no resampling involved, so <= 1
  * fit_image              -- resampler noise; downscale <= 1, upscale looser

The bounds come from measuring `image`'s Lanczos3 against Pillow's LANCZOS in
phase 2: downscaling agrees to within 1, upscaling reaches 16.

This harness is transitional. `--freeze` writes the Pillow outputs into
tests/differential/golden/, and those PNGs stay as the permanent Rust regression
suite once the Python is gone.
"""
from __future__ import annotations

import argparse
import io
import json
import math
import os
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
PROBE = REPO / "desktop" / "src-tauri" / "target" / "debug" / "examples" / "probe.exe"
WORK = Path(__file__).resolve().parent / "work"
SOURCES = Path(__file__).resolve().parent / "sources"
GOLDEN = Path(__file__).resolve().parent / "golden"

sys.path.insert(0, str(REPO / "src"))
from PIL import Image, ImageEnhance, ImageFilter, ImageOps  # noqa: E402

from wallpaper_changer.image_utils import fit_image  # noqa: E402
from wallpaper_changer.monitor import Monitor  # noqa: E402
from wallpaper_changer.wallpaper import apply_effect, plan_collage  # noqa: E402

EFFECTS = ("normal", "bw", "vintage", "hdr")
FIT_MODES = ("fill", "fit", "stretch", "center", "span")

# Monitor arrangements worth covering: one screen, a side-by-side pair, a screen at a
# negative offset (the common multi-head layout), and a three-head mix.
LAYOUTS = {
    "single": [{"x": 0, "y": 0, "width": 1920, "height": 1080}],
    "pair": [
        {"x": 0, "y": 0, "width": 1920, "height": 1080},
        {"x": 1920, "y": 0, "width": 1920, "height": 1080},
    ],
    "negative-offset": [
        {"x": 0, "y": 0, "width": 1920, "height": 1080},
        {"x": 1920, "y": -1072, "width": 3840, "height": 2160},
    ],
    "mixed-three": [
        {"x": -1080, "y": -200, "width": 1080, "height": 1920},
        {"x": 0, "y": 0, "width": 2560, "height": 1440},
        {"x": 2560, "y": 300, "width": 1366, "height": 768},
    ],
}


def run_probe(*args: str) -> str:
    result = subprocess.run(
        [str(PROBE), *args], capture_output=True, text=True, encoding="utf-8"
    )
    if result.returncode != 0:
        raise SystemExit(f"probe failed: {' '.join(args)}\n{result.stderr}")
    return result.stdout


def source_image(name: str, w: int, h: int) -> Path:
    """A picture that is hostile to a resampler and to an effect at once.

    Committed rather than regenerated on demand: the Rust golden test reads the very
    same bytes, so nothing depends on `math.sin` agreeing across implementations.
    """
    SOURCES.mkdir(parents=True, exist_ok=True)
    path = SOURCES / name
    if path.exists():
        return path
    px = []
    for y in range(h):
        for x in range(w):
            r = (x * 255) // max(w - 1, 1)
            g = 255 if ((x // 3 + y // 3) % 2) else 0  # hard 3px edges
            b = int(math.sin(x / 7.0) * 127 + 128)  # fine detail
            px.append((r, g, b))
    img = Image.new("RGB", (w, h))
    img.putdata(px)
    img.save(path)
    return path


def deltas(a: Image.Image, b: Image.Image) -> tuple[int, float, float]:
    """(max delta, mean delta, percentage of channels differing by more than 1)."""
    if a.size != b.size:
        raise SystemExit(f"size mismatch: {a.size} vs {b.size}")
    ab, bb = a.tobytes(), b.tobytes()
    worst = 0
    total = 0
    over_one = 0
    for x, y in zip(ab, bb):
        d = abs(x - y)
        worst = max(worst, d)
        total += d
        over_one += d > 1
    n = len(ab)
    return worst, total / n, 100.0 * over_one / n


class Report:
    def __init__(self) -> None:
        self.failures: list[str] = []
        self.checks = 0

    def check(self, ok: bool, label: str, detail: str) -> None:
        self.checks += 1
        status = "OK  " if ok else "FAIL"
        print(f"  {status} {label}  {detail}")
        if not ok:
            self.failures.append(f"{label}: {detail}")


# ── 1. geometry: zero tolerance ──────────────────────────────────────────────

def compare_geometry(report: Report) -> None:
    print("\n[1] plan_collage geometry - zero tolerance (pure integer arithmetic)")
    for layout_name, rects in LAYOUTS.items():
        monitors = [
            Monitor(index=i, x=r["x"], y=r["y"], width=r["width"], height=r["height"])
            for i, r in enumerate(rects)
        ]
        for count in range(1, 10):
            for same in (False, True):
                py = plan_collage(monitors, count, same)
                rust = json.loads(
                    run_probe("plan", json.dumps(rects), str(count), "true" if same else "false")
                )
                label = f"{layout_name} n={count} same={str(same):5s}"
                if py == rust:
                    report.check(True, label, f"{len(py)} cells identical")
                else:
                    first = next(
                        (i for i, (a, b) in enumerate(zip(py, rust)) if a != b), None
                    )
                    detail = f"cells differ (len {len(py)} vs {len(rust)})"
                    if first is not None:
                        detail += f"; first at {first}: {py[first]} vs {rust[first]}"
                    report.check(False, label, detail)


# ── 2. effects: <= 1 ─────────────────────────────────────────────────────────

def compare_effects(report: Report, freeze: bool) -> None:
    print("\n[2] apply_effect - tolerance 1 (no resampling involved)")
    # More than one picture: a flat image would hide a border bug, and a tiny one
    # exercises the "every pixel is a border pixel" path.
    sources = [
        ("detailed", source_image("effect-source.png", 300, 200)),
        ("small", source_image("effect-small.png", 2, 2)),
        ("thin", source_image("effect-thin.png", 40, 1)),
    ]
    for label, src_path in sources:
        src = Image.open(src_path).convert("RGB")
        for effect in EFFECTS:
            out = WORK / f"rust-effect-{label}-{effect}.png"
            run_probe("effect", str(src_path), effect, str(out))
            rust = Image.open(out).convert("RGB")
            py = apply_effect(src.copy(), effect)
            if freeze:
                py.save(GOLDEN / f"effect-{label}-{effect}.png")
            worst, mean, over = deltas(py, rust)
            report.check(
                worst <= 1,
                f"{label:8s} effect={effect:8s}",
                f"max={worst} mean={mean:.4f} >1={over:.3f}%",
            )


# ── 3. fitting: bound depends on whether the resample upscales ───────────────

# Upscaling is where the two Lanczos implementations genuinely diverge; measured
# maxima across this corpus reach 22. The bound is a smoke test, not a contract --
# what actually pins fitting from here on is the frozen golden set.
UPSCALE_BOUND = 24


def scale_direction(mode: str, sw: int, sh: int, tw: int, th: int) -> str:
    """Whether this fit resamples, and if so in which direction.

    The label on a test case is not enough: fitting a 400x300 source into 500x100
    *upscales* the width even though both targets look smaller. `fill` and `fit`
    pick a single ratio, so one number decides it.
    """
    if mode == "center":
        return "none"
    if mode == "stretch":
        return "up" if (tw > sw or th > sh) else "down"
    ratio_w, ratio_h = tw / sw, th / sh
    ratio = max(ratio_w, ratio_h) if mode in ("fill", "span") else min(ratio_w, ratio_h)
    return "up" if ratio > 1.0 else "down"


def compare_fitting(report: Report, freeze: bool) -> None:
    print("\n[3] fit_image - exact where nothing is resampled, 1 for a downscale,")
    print(f"    {UPSCALE_BOUND} where any axis is upscaled")
    src_path = source_image("fit-source.png", 400, 300)
    src = Image.open(src_path).convert("RGB")
    sw, sh = src.size

    cases = [
        ("shrink-both", 200, 150),
        ("narrow-tall", 120, 400),
        ("wide-short", 500, 100),
        ("grow-both", 800, 600),
        ("same-size", 400, 300),
    ]
    for kind, tw, th in cases:
        for mode in FIT_MODES:
            out = WORK / f"rust-fit-{kind}-{mode}.png"
            run_probe("fit", str(src_path), str(tw), str(th), mode, str(out))
            rust = Image.open(out).convert("RGB")
            py = fit_image(src.copy(), tw, th, mode)
            if freeze:
                py.save(GOLDEN / f"fit-{kind}-{mode}.png")
            worst, mean, over = deltas(py, rust)

            direction = scale_direction(mode, sw, sh, tw, th)
            bound = {"none": 0, "down": 1, "up": UPSCALE_BOUND}[direction]
            report.check(
                worst <= bound,
                f"{kind:12s} {mode:8s} -> {tw}x{th} [{direction:4s}]",
                f"max={worst} (bound {bound}) mean={mean:.4f} >1={over:.3f}%",
            )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--freeze",
        action="store_true",
        help="write the Pillow outputs to golden/ as the permanent regression set",
    )
    args = parser.parse_args()

    if not PROBE.exists():
        raise SystemExit(
            f"probe binary missing: {PROBE}\n"
            "build it with: cargo build -p wallpaper-core --example probe"
        )
    WORK.mkdir(parents=True, exist_ok=True)
    if args.freeze:
        GOLDEN.mkdir(parents=True, exist_ok=True)

    report = Report()
    compare_geometry(report)
    compare_effects(report, args.freeze)
    compare_fitting(report, args.freeze)

    print(f"\n{report.checks - len(report.failures)}/{report.checks} checks passed")
    if report.failures:
        print("\nfailures:")
        for failure in report.failures:
            print(f"  - {failure}")
        return 1
    if args.freeze:
        print(f"golden images written to {GOLDEN}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
