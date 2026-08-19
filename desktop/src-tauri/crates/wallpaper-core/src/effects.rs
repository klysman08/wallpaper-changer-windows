//! Pillow's image maths, reimplemented.
//!
//! Ports `wallpaper.py::apply_effect`. The `image` crate has no equivalent of
//! `ImageEnhance`, whose whole model is *interpolate between the picture and a
//! degenerate version of it*, so the four effects are built from primitives here.
//!
//! Every rule below was measured against the installed Pillow rather than inferred,
//! because each one is a place where a reasonable guess is wrong:
//!
//! - **Greyscale is ITU-R 601-2, in fixed point.** `(R*19595 + G*38470 + B*7471 +
//!   32768) >> 16`, verified exact. The `image` crate's `grayscale` uses Rec. 709
//!   (0.2126/0.7152/0.0722), which makes `bw` and `vintage` visibly wrong.
//! - **`Image.blend` truncates, it does not round.** `a + alpha*(b - a)` cast to
//!   `u8`, clamped. With `alpha > 1` — which `hdr` uses — it extrapolates past both
//!   endpoints and the clamp is what keeps it in range.
//! - **A 3x3 kernel does not touch the 1-pixel border.** Edge pixels are copied
//!   through unchanged. Convolving them lights up all four edges in a diff.
//! - **The kernel is pre-divided into `f32`.** Pillow divides each weight by the
//!   scale once, accumulates row by row in `f32`, then rounds with `+0.5` and
//!   truncates. That is not the same as accumulating integers and dividing at the
//!   end: the rounding error pushes an exact `.5` slightly low, so it rounds *down*
//!   where a clean `round_half_up` would round up.
//!
//! Measured agreement with Pillow on random 3x3 patches: SMOOTH exact (3000/3000),
//! DETAIL 98.8% exact with the remainder differing by exactly 1 at half-way values.

use image::RgbImage;

use crate::{CoreError, ErrorKind};

/// The effects the engine offers, in the order `wallpaper.py` lists them.
pub const EFFECTS: &[&str] = &["normal", "bw", "vintage", "hdr"];

/// `#3b2a1a` — the shadow end of the vintage ramp.
const VINTAGE_BLACK: [u8; 3] = [0x3b, 0x2a, 0x1a];
/// `#d8c3a5` — the highlight end.
const VINTAGE_WHITE: [u8; 3] = [0xd8, 0xc3, 0xa5];

const SMOOTH: ([i32; 9], i32) = ([1, 1, 1, 1, 5, 1, 1, 1, 1], 13);
const DETAIL: ([i32; 9], i32) = ([0, -1, 0, -1, 10, -1, 0, -1, 0], 6);

/// One channel of luminance, ITU-R 601-2 in fixed point — Pillow's `convert("L")`.
#[inline]
pub fn luma(r: u8, g: u8, b: u8) -> u8 {
    ((r as u32 * 19595 + g as u32 * 38470 + b as u32 * 7471 + 32768) >> 16) as u8
}

/// The picture as grey, still in RGB — Pillow's `convert("L").convert("RGB")`.
fn to_grey_rgb(image: &RgbImage) -> RgbImage {
    let mut out = image.clone();
    for pixel in out.pixels_mut() {
        let l = luma(pixel[0], pixel[1], pixel[2]);
        *pixel = image::Rgb([l, l, l]);
    }
    out
}

/// The mean of the luminance histogram, rounded as `int(mean + 0.5)`.
fn mean_luma(image: &RgbImage) -> u8 {
    let mut total: u64 = 0;
    for pixel in image.pixels() {
        total += luma(pixel[0], pixel[1], pixel[2]) as u64;
    }
    let count = (image.width() as u64 * image.height() as u64).max(1);
    ((total as f64 / count as f64) + 0.5) as u8
}

/// `Image.blend(a, b, alpha)` — `a + alpha*(b - a)`, truncated and clamped.
///
/// Truncation, not rounding: measured against Pillow. `alpha` above 1 extrapolates
/// beyond `b`, which is exactly what the `hdr` enhancements rely on.
fn blend(a: &RgbImage, b: &RgbImage, alpha: f64) -> RgbImage {
    let mut out = a.clone();
    for (pixel, (from, to)) in out.pixels_mut().zip(a.pixels().zip(b.pixels())) {
        for c in 0..3 {
            let value = from[c] as f64 + alpha * (to[c] as f64 - from[c] as f64);
            pixel[c] = value.clamp(0.0, 255.0) as u8; // `as` truncates toward zero
        }
    }
    out
}

/// A flat picture of one grey level, the degenerate image `Contrast` interpolates
/// from.
fn flat(width: u32, height: u32, level: u8) -> RgbImage {
    RgbImage::from_pixel(width, height, image::Rgb([level, level, level]))
}

/// `ImageOps.colorize(grey, black, white)` — a per-channel linear ramp LUT.
///
/// The ramp covers 0..=254 with floor division; index 255 is the white point itself,
/// because Pillow builds the low ramp over `range(whitepoint - blackpoint)` and then
/// appends the high-end values separately.
fn colorize(grey: &RgbImage, black: [u8; 3], white: [u8; 3]) -> RgbImage {
    let mut lut = [[0u8; 256]; 3];
    for (channel, table) in lut.iter_mut().enumerate() {
        let (lo, hi) = (black[channel] as i32, white[channel] as i32);
        for (i, slot) in table.iter_mut().enumerate().take(255) {
            *slot = (lo + (i as i32 * (hi - lo)).div_euclid(255)) as u8;
        }
        table[255] = white[channel];
    }
    let mut out = grey.clone();
    for pixel in out.pixels_mut() {
        let level = pixel[0] as usize; // grey: all three channels are equal
        *pixel = image::Rgb([lut[0][level], lut[1][level], lut[2][level]]);
    }
    out
}

/// A 3x3 convolution with Pillow's arithmetic.
///
/// The border is copied through untouched, the weights are pre-divided into `f32`,
/// and each row is accumulated as one expression before being added to the running
/// total — all three matter for matching byte for byte.
fn filter_3x3(image: &RgbImage, kernel: [i32; 9], scale: i32) -> RgbImage {
    let (width, height) = image.dimensions();
    let mut out = image.clone();
    if width < 3 || height < 3 {
        return out; // every pixel is a border pixel
    }

    let k: Vec<f32> = kernel.iter().map(|&w| w as f32 / scale as f32).collect();

    for y in 1..height - 1 {
        for x in 1..width - 1 {
            for c in 0..3 {
                let at = |dx: i32, dy: i32| {
                    image.get_pixel((x as i32 + dx) as u32, (y as i32 + dy) as u32)[c] as f32
                };
                let row = |dy: i32, k0: f32, k1: f32, k2: f32| {
                    at(-1, dy) * k0 + at(0, dy) * k1 + at(1, dy) * k2
                };
                let mut sum = 0f32;
                sum += row(-1, k[0], k[1], k[2]);
                sum += row(0, k[3], k[4], k[5]);
                sum += row(1, k[6], k[7], k[8]);

                out.get_pixel_mut(x, y)[c] = if sum <= 0.0 {
                    0
                } else if sum >= 255.0 {
                    255
                } else {
                    (sum + 0.5) as u8
                };
            }
        }
    }
    out
}

/// `ImageEnhance.Color(im).enhance(f)`.
fn enhance_color(image: &RgbImage, factor: f64) -> RgbImage {
    blend(&to_grey_rgb(image), image, factor)
}

/// `ImageEnhance.Contrast(im).enhance(f)` — interpolates against a flat image of the
/// mean luminance.
fn enhance_contrast(image: &RgbImage, factor: f64) -> RgbImage {
    let degenerate = flat(image.width(), image.height(), mean_luma(image));
    blend(&degenerate, image, factor)
}

/// `ImageEnhance.Sharpness(im).enhance(f)` — interpolates against a SMOOTH-filtered
/// copy, so a factor above 1 extrapolates away from the blur.
fn enhance_sharpness(image: &RgbImage, factor: f64) -> RgbImage {
    let degenerate = filter_3x3(image, SMOOTH.0, SMOOTH.1);
    blend(&degenerate, image, factor)
}

/// Apply one of the four effects to the finished canvas.
pub fn apply_effect(canvas: &RgbImage, effect: &str) -> Result<RgbImage, CoreError> {
    Ok(match effect {
        "normal" => canvas.clone(),
        "bw" => to_grey_rgb(canvas),
        "vintage" => {
            let sepia = colorize(&to_grey_rgb(canvas), VINTAGE_BLACK, VINTAGE_WHITE);
            enhance_color(&sepia, 0.9)
        }
        "hdr" => {
            let enhanced = enhance_contrast(canvas, 1.35);
            let enhanced = enhance_sharpness(&enhanced, 1.45);
            filter_3x3(&enhanced, DETAIL.0, DETAIL.1)
        }
        other => {
            return Err(CoreError::new(
                ErrorKind::Invalid,
                format!("Unknown effect: {other}"),
            ))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, rgb: [u8; 3]) -> RgbImage {
        RgbImage::from_pixel(w, h, image::Rgb(rgb))
    }

    /// Measured against Pillow's `convert("L")`; Rec. 709 would give different
    /// numbers and make `bw` and `vintage` wrong.
    #[test]
    fn luma_is_itu_r_601_2() {
        assert_eq!(luma(0, 0, 0), 0);
        assert_eq!(luma(255, 255, 255), 255);
        assert_eq!(luma(255, 0, 0), 76);
        assert_eq!(luma(0, 255, 0), 150);
        assert_eq!(luma(0, 0, 255), 29);
        assert_eq!(luma(100, 150, 200), 141);
        assert_eq!(luma(37, 211, 89), 145);
    }

    /// Pillow truncates rather than rounding, which is worth a full point of drift
    /// on half the pixels if you get it wrong.
    #[test]
    fn blend_truncates_and_clamps() {
        let a = solid(1, 1, [0, 255, 100]);
        let b = solid(1, 1, [255, 0, 200]);

        let out = blend(&a, &b, 0.9);
        assert_eq!(out.get_pixel(0, 0).0, [229, 25, 190], "229.5 must truncate to 229");

        // alpha > 1 extrapolates past both endpoints; the clamp holds it in range.
        let out = blend(&a, &b, 1.35);
        assert_eq!(out.get_pixel(0, 0).0, [255, 0, 235]);
    }

    #[test]
    fn blend_at_one_returns_the_second_image() {
        let a = solid(2, 2, [10, 20, 30]);
        let b = solid(2, 2, [200, 100, 50]);
        assert_eq!(blend(&a, &b, 1.0).get_pixel(0, 0).0, [200, 100, 50]);
    }

    /// The border is copied through, which is the difference between a clean diff
    /// and one that lights up around all four edges.
    #[test]
    fn a_kernel_leaves_the_border_untouched() {
        let mut image = RgbImage::new(5, 5);
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            let v = ((x + y * 5) * 10) as u8;
            *pixel = image::Rgb([v, v, v]);
        }
        let out = filter_3x3(&image, SMOOTH.0, SMOOTH.1);
        for y in 0..5 {
            for x in 0..5 {
                if x == 0 || y == 0 || x == 4 || y == 4 {
                    assert_eq!(out.get_pixel(x, y), image.get_pixel(x, y), "border at {x},{y}");
                }
            }
        }
        // Centre of a linear ramp is its own average.
        assert_eq!(out.get_pixel(2, 2).0[0], 120);
    }

    /// An image with no interior is all border.
    #[test]
    fn a_tiny_image_passes_through_a_kernel_unchanged() {
        let image = solid(2, 2, [7, 8, 9]);
        assert_eq!(filter_3x3(&image, DETAIL.0, DETAIL.1), image);
    }

    #[test]
    fn detail_clamps_rather_than_wrapping() {
        let mut image = solid(3, 3, [0, 0, 0]);
        *image.get_pixel_mut(1, 1) = image::Rgb([255, 255, 255]);
        // 255*10/6 = 425 -> clamped, not wrapped to 169.
        assert_eq!(filter_3x3(&image, DETAIL.0, DETAIL.1).get_pixel(1, 1).0, [255, 255, 255]);

        let mut image = solid(3, 3, [255, 255, 255]);
        *image.get_pixel_mut(1, 1) = image::Rgb([0, 0, 0]);
        assert_eq!(filter_3x3(&image, DETAIL.0, DETAIL.1).get_pixel(1, 1).0, [0, 0, 0]);
    }

    #[test]
    fn colorize_maps_the_ends_of_the_ramp_to_the_palette() {
        let mut grey = RgbImage::new(2, 1);
        *grey.get_pixel_mut(0, 0) = image::Rgb([0, 0, 0]);
        *grey.get_pixel_mut(1, 0) = image::Rgb([255, 255, 255]);
        let out = colorize(&grey, VINTAGE_BLACK, VINTAGE_WHITE);
        assert_eq!(out.get_pixel(0, 0).0, VINTAGE_BLACK);
        assert_eq!(out.get_pixel(1, 0).0, VINTAGE_WHITE);
    }

    #[test]
    fn contrast_uses_the_mean_luminance() {
        let mut image = RgbImage::new(3, 1);
        *image.get_pixel_mut(0, 0) = image::Rgb([0, 0, 0]);
        *image.get_pixel_mut(1, 0) = image::Rgb([255, 255, 255]);
        *image.get_pixel_mut(2, 0) = image::Rgb([10, 20, 30]);
        // luma values 0, 255, 18 -> mean 91.0 -> 91
        assert_eq!(mean_luma(&image), 91);
    }

    #[test]
    fn normal_is_the_picture_itself() {
        let image = solid(4, 4, [12, 34, 56]);
        assert_eq!(apply_effect(&image, "normal").unwrap(), image);
    }

    #[test]
    fn bw_leaves_no_colour_behind() {
        let image = solid(4, 4, [200, 50, 25]);
        let out = apply_effect(&image, "bw").unwrap();
        for pixel in out.pixels() {
            assert_eq!(pixel[0], pixel[1]);
            assert_eq!(pixel[1], pixel[2]);
        }
        assert_eq!(out.get_pixel(0, 0).0[0], luma(200, 50, 25));
    }

    /// Vintage is warm by construction: the ramp's ends are a brown and a cream, so
    /// red must lead blue everywhere.
    #[test]
    fn vintage_is_warm_everywhere() {
        let image = solid(4, 4, [128, 128, 128]);
        let out = apply_effect(&image, "vintage").unwrap();
        let pixel = out.get_pixel(0, 0);
        assert!(pixel[0] > pixel[2], "expected a warm cast, got {pixel:?}");
    }

    #[test]
    fn hdr_runs_and_keeps_the_dimensions() {
        let mut image = RgbImage::new(8, 8);
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            *pixel = image::Rgb([(x * 30) as u8, (y * 30) as u8, 128]);
        }
        let out = apply_effect(&image, "hdr").unwrap();
        assert_eq!(out.dimensions(), (8, 8));
    }

    #[test]
    fn every_named_effect_is_accepted() {
        let image = solid(4, 4, [90, 90, 90]);
        for effect in EFFECTS {
            assert!(apply_effect(&image, effect).is_ok(), "{effect} was rejected");
        }
    }

    #[test]
    fn an_unknown_effect_is_invalid_and_says_so_like_python() {
        let err = apply_effect(&solid(2, 2, [0, 0, 0]), "sepia").unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Invalid);
        assert_eq!(err.to_string(), "Unknown effect: sepia");
    }
}
