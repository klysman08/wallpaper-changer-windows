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
//! - **A downscale may differ by 2.** `fast_image_resize`'s Lanczos3 and Pillow's
//!   LANCZOS are different implementations of the same filter; measured, they agree to
//!   within two levels when shrinking.
//! - **An upscale may differ more.** That is where the two genuinely diverge; the
//!   golden image, not the bound, is what pins it.
//!
//! **The goldens are Pillow's output and are never regenerated from Rust.** Doing that
//! would leave them comparing the port against itself, and `src/wallpaper_changer/` is
//! on its way out, so there is no second chance to produce them. When the composition
//! changes deliberately, what moves is the *bound* — re-derived from
//! `fit_drift_against_the_goldens` rather than loosened until the failure stops. If one
//! of these fails without such a change, the question is what changed in the
//! composition, not how to make the number go away.
//!
//! The downscale bound went from 1 to 2 when the resampler was swapped from
//! `image::imageops` to `fast_image_resize`, which took a preview from 2.31 s to
//! 0.336 s. Every case was measured, not just the one that failed first: pure
//! downscales moved 1 -> 2, the two `fit` downscales stayed at 1, upscales stayed
//! inside the existing 24 (worst 22), and a same-size resize became **exactly 0**
//! where it had been allowed 1. Two levels out of 255 is below anything visible in a
//! wallpaper, which is what makes the trade worth taking.

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
                        2
                    }
                }
                _ => {
                    let (rw, rh) = (tw as f64 / sw, th as f64 / sh);
                    let ratio = if mode == "fit" { rw.min(rh) } else { rw.max(rh) };
                    if ratio > 1.0 {
                        24
                    } else if ratio == 1.0 {
                        // Resizing to the size it already is must be the identity, and
                        // measurably is. Held exact so a resampler that quietly stops
                        // short-circuiting shows up here rather than in a wallpaper.
                        0
                    } else {
                        2
                    }
                }
            };
            assert_within(&actual, &expected, bound, &format!("{kind}/{mode}"));
        }
    }
}

/// Print how far every fit case sits from its golden, instead of asserting.
///
/// For the one job the assertions cannot do: when the resampler is deliberately
/// changed, the bounds above have to be re-derived from measurement, and picking them
/// by watching which assertion fails first is how a tolerance quietly becomes "big
/// enough to pass" rather than "as tight as the arithmetic allows".
///
/// ```text
/// cargo test -p wallpaper-core --test golden -- --ignored --nocapture drift
/// ```
#[test]
#[ignore = "reports numbers rather than asserting them"]
fn fit_drift_against_the_goldens() {
    let src = source("fit-source.png");
    println!("{:<14} {:<8} {:>9} {:>12}", "case", "mode", "max delta", "differing");
    for (kind, tw, th) in [
        ("shrink-both", 200, 150),
        ("narrow-tall", 120, 400),
        ("wide-short", 500, 100),
        ("grow-both", 800, 600),
        ("same-size", 400, 300),
    ] {
        for mode in ["fill", "fit", "stretch", "center", "span"] {
            let actual = collage::fit_image(&src, tw, th, mode);
            let (worst, differing) = compare(&actual, &golden(&format!("fit-{kind}-{mode}.png")));
            println!("{kind:<14} {mode:<8} {worst:>9} {differing:>12}");
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
