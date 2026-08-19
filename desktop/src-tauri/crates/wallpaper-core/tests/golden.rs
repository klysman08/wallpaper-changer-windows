//! Composition pinned against images Pillow produced.
//!
//! `tests/differential/compare.py --freeze` wrote `golden/` from the Python engine
//! while both implementations still existed. This test replays the same inputs
//! through the Rust core and holds it to them, which is what keeps the port honest
//! after `src/wallpaper_changer/` is deleted — at that point the goldens are the only
//! remaining record of what the composition used to produce.
//!
//! The tolerances differ per stage on purpose, and the reasoning is the same as the
//! harness's:
//!
//! - **Effects are exact.** No resampling is involved, so any difference at all is a
//!   bug in the arithmetic — a wrong luma coefficient, a mishandled border, the wrong
//!   rounding in a kernel.
//! - **A downscale may differ by 1.** `image`'s Lanczos3 and Pillow's LANCZOS are
//!   different implementations of the same filter; measured, they agree to within one
//!   level when shrinking.
//! - **An upscale may differ more.** That is where the two genuinely diverge; the
//!   golden image, not the bound, is what pins it.
//!
//! Regenerating a golden is a deliberate act. If one of these fails, the question is
//! what changed in the composition — not how to make the number go away.

use std::path::{Path, PathBuf};

use image::RgbImage;
use wallpaper_core::{collage, effects};

fn fixtures() -> PathBuf {
    // crates/wallpaper-core -> crates -> src-tauri -> desktop -> repo root
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .expect("walked above the repo root")
        .join("tests")
        .join("differential")
}

fn load(path: &Path) -> RgbImage {
    image::open(path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
        .to_rgb8()
}

fn source(name: &str) -> RgbImage {
    load(&fixtures().join("sources").join(name))
}

fn golden(name: &str) -> RgbImage {
    load(&fixtures().join("golden").join(name))
}

/// `(max delta, count of channels differing at all)`.
fn compare(a: &RgbImage, b: &RgbImage) -> (u8, usize) {
    assert_eq!(a.dimensions(), b.dimensions(), "dimensions differ");
    let mut worst = 0u8;
    let mut differing = 0usize;
    for (left, right) in a.pixels().zip(b.pixels()) {
        for c in 0..3 {
            let delta = left[c].abs_diff(right[c]);
            worst = worst.max(delta);
            differing += usize::from(delta != 0);
        }
    }
    (worst, differing)
}

fn assert_within(actual: &RgbImage, expected: &RgbImage, bound: u8, label: &str) {
    let (worst, differing) = compare(actual, expected);
    assert!(
        worst <= bound,
        "{label}: max delta {worst} exceeds {bound} ({differing} channels differ)"
    );
}

/// Every effect, on three shapes. Exact — these involve no resampling.
#[test]
fn effects_match_the_golden_images_exactly() {
    for (label, file) in [
        ("detailed", "effect-source.png"),
        ("small", "effect-small.png"),
        ("thin", "effect-thin.png"),
    ] {
        let src = source(file);
        for effect in ["normal", "bw", "vintage", "hdr"] {
            let actual = effects::apply_effect(&src, effect).expect("apply effect");
            let expected = golden(&format!("effect-{label}-{effect}.png"));
            assert_within(&actual, &expected, 0, &format!("{label}/{effect}"));
        }
    }
}

/// The five fit modes across five target shapes.
///
/// The bound follows the actual resampling direction rather than the case's name: a
/// 400x300 source into 500x100 *upscales* the width even though both numbers look
/// smaller, and `fill` and `fit` can disagree about direction for the same target.
#[test]
fn fitting_matches_the_golden_images() {
    let src = source("fit-source.png");
    let (sw, sh) = (src.width() as f64, src.height() as f64);

    for (kind, tw, th) in [
        ("shrink-both", 200, 150),
        ("narrow-tall", 120, 400),
        ("wide-short", 500, 100),
        ("grow-both", 800, 600),
        ("same-size", 400, 300),
    ] {
        for mode in ["fill", "fit", "stretch", "center", "span"] {
            let actual = collage::fit_image(&src, tw, th, mode);
            let expected = golden(&format!("fit-{kind}-{mode}.png"));

            let bound = match mode {
                // Never resamples: a straight paste, so it must be exact.
                "center" => 0,
                "stretch" => {
                    if tw as f64 > sw || th as f64 > sh {
                        24
                    } else {
                        1
                    }
                }
                _ => {
                    let (rw, rh) = (tw as f64 / sw, th as f64 / sh);
                    let ratio = if mode == "fit" { rw.min(rh) } else { rw.max(rh) };
                    if ratio > 1.0 {
                        24
                    } else {
                        1
                    }
                }
            };
            assert_within(&actual, &expected, bound, &format!("{kind}/{mode}"));
        }
    }
}

/// A sanity check on the fixtures themselves: a stale or truncated golden set would
/// otherwise make the tests above vacuous.
#[test]
fn the_golden_set_is_complete() {
    let dir = fixtures().join("golden");
    let count = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "png"))
        .count();
    // 3 shapes x 4 effects + 5 target shapes x 5 fit modes
    assert_eq!(count, 37, "expected 37 golden images in {}", dir.display());
}
