//! Folder listing and the two ways a picture reaches the webview.
//!
//! Ports the listing half of `image_utils.py`, `scan_video_folder` from
//! `video_wallpaper.py`, and `get_thumbnails` / `get_image_preview` from `rpc.py`.
//! Selection (`pick_images`) and fitting (`fit_image`) belong to the composition
//! phase and are not here.
//!
//! The webview cannot read local files, so a picture only ever reaches it as base64.
//! That is why these live in the engine at all.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageReader, RgbImage};
use serde_json::{json, Value};

use crate::CoreError;

/// Image extensions the engine will open. Mirrors `image_utils.SUPPORTED`.
pub const SUPPORTED: &[&str] = &["jpg", "jpeg", "png", "bmp", "webp"];

/// Image formats the engine deliberately cannot open, tallied so it can say so.
///
/// Not decoding these is a choice: HEIC needs libheif and AVIF needs dav1d, both C
/// libraries, and neither is worth carrying next to a 112 MB libmpv. But *silently*
/// omitting them is not the same choice. 302 of 4948 files in one real folder were
/// invisible to the app with nothing said about it, which is what "it does not
/// support different formats" turns out to mean from the outside.
///
/// Only formats a picture is plausibly stored in belong here. A folder also holds
/// `.txt` and `.ini`, and reporting those as skipped pictures would be noise.
pub const UNSUPPORTED: &[&str] = &[
    "heic", "heif", "avif", "jxl", "tif", "tiff", "gif", "ico", "svg", "psd", "tga",
    // Camera raw. Each vendor has its own, and none of them is a normal image file.
    "cr2", "cr3", "nef", "arw", "dng", "orf", "rw2", "raf", "srw", "pef",
];

/// Video extensions. Mirrors `video_wallpaper.VIDEO_EXTENSIONS`.
pub const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mkv", "avi", "mov", "wmv", "webm", "m4v"];

fn has_extension(path: &Path, allowed: &[&str]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .is_some_and(|e| allowed.contains(&e.as_str()))
}

/// Images directly inside `folder`, in the order the filesystem reports them.
///
/// Two details are load-bearing and deliberately copied from `list_images`:
/// sub-folders are not descended into, and the result is **not sorted** — the order
/// is whatever `read_dir` yields, matching `Path.iterdir()`. A missing folder is an
/// empty list rather than an error, because the settings screen lists a folder the
/// user is still typing.
pub fn list_images(folder: impl AsRef<Path>) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder.as_ref()) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| has_extension(p, SUPPORTED))
        .collect()
}

/// Videos directly inside `folder`, **sorted**, files only.
///
/// `scan_video_folder` sorts and checks `is_file()` where `list_images` does
/// neither; the playlist order the user sees depends on it.
pub fn list_videos(folder: impl AsRef<Path>) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder.as_ref()) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .filter(|p| has_extension(p, VIDEO_EXTENSIONS))
        .collect();
    // Python sorts `Path` objects, which compares their string form. UTF-8 byte
    // order and Unicode code-point order agree, so a plain sort matches.
    found.sort();
    found
}

/// Round to the nearest integer, breaking an exact tie towards the even neighbour.
///
/// `round()` in Python 3 is half-to-even, so 2.5 becomes 2 rather than 3. The
/// difference only shows on an exact .5, but it is free to match. Callers clamp the
/// result themselves — a rounded-to-zero dimension is their problem, not this one.
fn round_half_even(value: f64) -> i64 {
    let floor = value.floor();
    let diff = value - floor;
    let floor = floor as i64;
    if diff == 0.5 {
        // The half-to-even case, and the only one that differs from round-half-up.
        if floor % 2 == 0 {
            floor
        } else {
            floor + 1
        }
    } else if diff > 0.5 {
        floor + 1
    } else {
        floor
    }
}

/// Lanczos3 resampling, SIMD, for every resize the engine does.
///
/// The one resampler in the crate: composition, thumbnails and the preview downscale
/// all come here, so they cannot drift apart.
///
/// `image::imageops::resize` is scalar Rust and was where composition spent its time —
/// decoding a preview's four pictures is about 6% of it and resampling is nearly all
/// the rest, which put the port at 2.3x Pillow despite doing the same work. Pillow's
/// own resize is SIMD, and `fast_image_resize` implements the same normalised
/// separable convolution, so this is closer to Pillow's arithmetic than what it
/// replaces as well as being faster.
///
/// Falls back to the scalar path if the resizer refuses a size, because a preview that
/// is slow is better than a preview that is missing.
pub fn resize_lanczos3(source: &RgbImage, width: u32, height: u32) -> RgbImage {
    use fast_image_resize::images::Image as FirImage;
    use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};

    let (width, height) = (width.max(1), height.max(1));
    let scalar =
        || image::imageops::resize(source, width, height, image::imageops::FilterType::Lanczos3);

    let Ok(src) = FirImage::from_vec_u8(
        source.width(),
        source.height(),
        source.as_raw().clone(),
        PixelType::U8x3,
    ) else {
        return scalar();
    };
    let mut dst = FirImage::new(width, height, PixelType::U8x3);
    let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3));
    if Resizer::new().resize(&src, &mut dst, &options).is_err() {
        return scalar();
    }
    RgbImage::from_raw(width, height, dst.into_vec()).unwrap_or_else(scalar)
}

/// Decode an image file, deciding the format from its **content**.
///
/// The single loader for the whole crate, and the content sniff is the reason it
/// exists. `image::open` documents that it picks the format "from the path's file
/// extension" — so a `.jpeg` that is really a WebP goes to the JPEG decoder and comes
/// back as `Illegal start bytes: 5249` (`RI`, the front of a RIFF header). That is not
/// a rare curiosity: 470 of 4948 files in one real wallpaper folder, 9.5%, had an
/// extension that lied, in every direction — JPEGs named `.webp`, WebPs named `.jpeg`
/// and `.png`, PNGs named `.jpeg`.
///
/// Pillow sniffs content in `Image.open`, so the Python engine never saw this. The
/// port did, and unevenly: thumbnails used this function and worked, while composing
/// used `image::open` and failed on the same file — which is exactly what a user sees
/// as "the picker looks fine but preview and apply are broken".
pub fn open_image(path: impl AsRef<Path>) -> Result<DynamicImage, CoreError> {
    let path = path.as_ref();
    if !path.exists() {
        return Err(CoreError::not_found(format!(
            "Image not found: {}",
            path.display()
        )));
    }
    let unreadable = |e: &dyn std::fmt::Display| {
        CoreError::invalid(format!("Could not read {}: {e}", path.display()))
    };
    ImageReader::open(path)
        .map_err(|e| unreadable(&e))?
        .with_guessed_format()
        .map_err(|e| unreadable(&e))?
        .decode()
        .map_err(|e| unreadable(&e))
}

fn encode_jpeg(rgb: &RgbImage, quality: u8) -> Result<Vec<u8>, CoreError> {
    let mut buffer = Vec::new();
    JpegEncoder::new_with_quality(&mut Cursor::new(&mut buffer), quality)
        .encode_image(rgb)
        .map_err(|e| CoreError::invalid(format!("Could not encode the image: {e}")))?;
    Ok(buffer)
}

fn to_base64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// The dimensions `Image.thumbnail` would choose for a `box`x`box` bounding square.
///
/// Preserves aspect ratio, never enlarges, and never returns a zero dimension.
/// Pillow picks whichever of floor/ceil lands closer to the source aspect, which is
/// why this is not simply a floor.
fn thumbnail_size(width: u32, height: u32, box_size: u32) -> (u32, u32) {
    if width <= box_size && height <= box_size {
        return (width, height);
    }
    let aspect = width as f64 / height as f64;
    let (mut x, mut y) = (box_size as f64, box_size as f64);
    let round_aspect = |number: f64, error: &dyn Fn(f64) -> f64| -> u32 {
        let (low, high) = (number.floor(), number.ceil());
        let pick = if error(high) < error(low) { high } else { low };
        (pick as u32).max(1)
    };
    if x / y >= aspect {
        x = round_aspect(y * aspect, &|n: f64| (aspect - n / y).abs()) as f64;
    } else {
        y = round_aspect(x / aspect, &|n: f64| {
            if n == 0.0 {
                0.0
            } else {
                (aspect - x / n).abs()
            }
        }) as f64;
    }
    (x as u32, y as u32)
}

/// How many files in `folder` are pictures the engine cannot open, keyed by extension.
///
/// Sorted, because it is rendered as a list and `read_dir` order would reshuffle the
/// same folder between two looks at it.
pub fn unsupported_images(folder: impl AsRef<Path>) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(folder.as_ref()) else {
        return counts;
    };
    for path in entries.filter_map(Result::ok).map(|e| e.path()) {
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        let ext = ext.to_ascii_lowercase();
        if UNSUPPORTED.contains(&ext.as_str()) {
            *counts.entry(ext).or_insert(0) += 1;
        }
    }
    counts
}

/// `list_folder_images` — every supported image in a folder, and what was skipped.
///
/// `skipped` counts the pictures in formats the engine cannot decode and
/// `skipped_formats` names them. Both are present for a folder that has none, so the
/// interface renders the same shape either way.
pub fn list_folder_images(folder: &str) -> Value {
    let images = list_images(folder);
    let skipped = unsupported_images(folder);
    json!({
        "count": images.len(),
        "images": images.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>(),
        "skipped": skipped.values().sum::<usize>(),
        "skipped_formats": skipped,
    })
}

/// `scan_videos` — every playable video in a folder.
pub fn scan_videos(folder: &str) -> Value {
    let videos = list_videos(folder);
    json!({
        "count": videos.len(),
        "videos": videos.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>(),
    })
}

/// `get_thumbnails` — base64 JPEGs keyed by the path that produced them.
///
/// A file that will not open is left out rather than failing the batch: one
/// unreadable picture in a folder should still let the user see the rest.
///
/// **But not silently.** Dropping the failures without a word is what left a user
/// unable to find out *which* of five thousand pictures was bad, or why — the tile
/// simply never appeared. Each failure is logged and returned in `failed`, so the
/// answer is both in the app log and available to the interface.
pub fn get_thumbnails(paths: &[String], size: i64) -> Value {
    let box_size = size.clamp(32, 512) as u32;

    // Decoding dominates a batch — shrinking to 160 px was never the expensive part —
    // and each picture is independent, so the whole job goes across threads. The
    // picker asks in batches of 24, and this is what stands between a folder appearing
    // and a folder appearing eventually.
    let done = crate::parallel::map_bounded(paths, |raw| -> Result<String, CoreError> {
        let image = open_image(raw)?;
        let (w, h) = thumbnail_size(image.width(), image.height(), box_size);
        // To RGB before resizing, not after: the output is JPEG either way, and
        // `to_rgb8` drops alpha without compositing, so the channels that survive are
        // convolved identically whichever order it happens in.
        let rgb = image.to_rgb8();
        let thumb = if (w, h) == (rgb.width(), rgb.height()) {
            rgb
        } else {
            resize_lanczos3(&rgb, w, h)
        };
        Ok(to_base64(&encode_jpeg(&thumb, 75)?))
    });

    // A batch that panicked outright still answers with what it has rather than
    // failing the call: this method's contract is that one bad picture costs its own
    // tile and nothing else.
    let done = done.unwrap_or_default();
    let mut out = serde_json::Map::new();
    let mut failed = Vec::new();
    for (raw, thumb) in paths.iter().zip(done) {
        match thumb {
            Ok(thumb) => {
                out.insert(raw.clone(), Value::String(thumb));
            }
            Err(e) => {
                // `warn`, not `debug`: this is the only trace a picture that never
                // appears leaves behind, and the log is where a bug report starts.
                log::warn!("thumbnail failed for {raw}: {}", e.message());
                failed.push(json!({ "path": raw, "reason": e.message() }));
            }
        }
    }
    json!({ "thumbnails": out, "failed": failed })
}

/// `get_image_preview` — one picture, large enough to actually look at.
///
/// Downscale only: a small picture is sent as it is rather than blown up into a
/// bigger payload.
pub fn get_image_preview(path: &str, max_width: i64) -> Result<Value, CoreError> {
    let box_width = max_width.clamp(64, 4096) as u32;
    let image = open_image(path)?;

    let rgb = image.to_rgb8();
    let shown = if rgb.width() > box_width {
        let height = round_half_even(rgb.height() as f64 * box_width as f64 / rgb.width() as f64)
            .max(1) as u32;
        resize_lanczos3(&rgb, box_width, height)
    } else {
        rgb
    };

    let bytes = encode_jpeg(&shown, 85)?;
    Ok(json!({
        "jpeg_base64": to_base64(&bytes),
        "width": shown.width(),
        "height": shown.height(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorKind;

    struct Dir(PathBuf);

    impl Dir {
        fn new(tag: &str) -> Self {
            let path = std::env::temp_dir().join(format!("wc-images-{}-{tag}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn touch(&self, name: &str) {
            std::fs::write(self.0.join(name), b"not really an image").unwrap();
        }
        fn write_png(&self, name: &str, w: u32, h: u32) -> String {
            let path = self.0.join(name);
            let buffer = image::RgbImage::from_fn(w, h, |x, y| {
                image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])
            });
            DynamicImage::ImageRgb8(buffer).save(&path).unwrap();
            path.to_string_lossy().into_owned()
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn listing_picks_up_supported_extensions_case_insensitively() {
        let dir = Dir::new("ext");
        for name in ["a.jpg", "b.JPEG", "c.png", "d.BMP", "e.webp"] {
            dir.touch(name);
        }
        for name in ["notes.txt", "clip.mp4", "no-extension"] {
            dir.touch(name);
        }
        let found = list_images(&dir.0);
        assert_eq!(found.len(), 5, "got {found:?}");
    }

    #[test]
    fn listing_a_missing_folder_is_empty_rather_than_an_error() {
        assert!(list_images("Z:/definitely-not-here").is_empty());
        assert!(list_videos("Z:/definitely-not-here").is_empty());
    }

    #[test]
    fn listing_does_not_descend_into_subfolders() {
        let dir = Dir::new("nested");
        dir.touch("top.png");
        std::fs::create_dir_all(dir.0.join("inner")).unwrap();
        std::fs::write(dir.0.join("inner").join("deep.png"), b"x").unwrap();
        assert_eq!(list_images(&dir.0).len(), 1);
    }

    /// `scan_video_folder` sorts where `list_images` does not — the playlist order
    /// the user steps through depends on it.
    #[test]
    fn videos_come_back_sorted() {
        let dir = Dir::new("videos");
        for name in ["c.mp4", "a.mkv", "b.webm"] {
            dir.touch(name);
        }
        let names: Vec<String> = list_videos(&dir.0)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["a.mkv", "b.webm", "c.mp4"]);
    }

    #[test]
    fn thumbnail_size_preserves_aspect_and_fits_the_box() {
        assert_eq!(thumbnail_size(1000, 500, 160), (160, 80));
        assert_eq!(thumbnail_size(500, 1000, 160), (80, 160));
        assert_eq!(thumbnail_size(1000, 1000, 160), (160, 160));
    }

    /// Pillow's `thumbnail` returns early when the picture already fits.
    #[test]
    fn thumbnail_size_never_enlarges() {
        assert_eq!(thumbnail_size(80, 40, 160), (80, 40));
        assert_eq!(thumbnail_size(160, 160, 160), (160, 160));
    }

    #[test]
    fn thumbnail_size_never_collapses_to_zero() {
        let (w, h) = thumbnail_size(4000, 3, 160);
        assert!(w >= 1 && h >= 1, "got {w}x{h}");
    }

    #[test]
    fn thumbnails_are_decodable_jpegs_within_the_box() {
        let dir = Dir::new("thumbs");
        let path = dir.write_png("wide.png", 800, 400);
        let result = get_thumbnails(&[path.clone()], 160);
        let encoded = result["thumbnails"][&path].as_str().expect("a thumbnail");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap();
        let decoded = image::load_from_memory(&bytes).expect("decodable JPEG");
        assert!(decoded.width() <= 160 && decoded.height() <= 160);
        assert_eq!((decoded.width(), decoded.height()), (160, 80));
    }

    #[test]
    fn an_unreadable_file_is_dropped_instead_of_failing_the_batch() {
        let dir = Dir::new("mixed");
        let good = dir.write_png("good.png", 40, 40);
        dir.touch("broken.png");
        let broken = dir.0.join("broken.png").to_string_lossy().into_owned();

        let result = get_thumbnails(&[good.clone(), broken.clone(), "Z:/gone.png".into()], 64);
        let thumbs = result["thumbnails"].as_object().unwrap();
        assert!(thumbs.contains_key(&good));
        assert!(!thumbs.contains_key(&broken));
        assert_eq!(thumbs.len(), 1);
    }

    #[test]
    fn thumbnail_size_is_clamped_to_the_documented_range() {
        let dir = Dir::new("clamp");
        let path = dir.write_png("big.png", 2000, 2000);
        let huge = get_thumbnails(&[path.clone()], 99_999);
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(huge["thumbnails"][&path].as_str().unwrap())
            .unwrap();
        assert_eq!(image::load_from_memory(&bytes).unwrap().width(), 512);
    }

    #[test]
    fn preview_only_ever_shrinks() {
        let dir = Dir::new("preview");
        let small = dir.write_png("small.png", 100, 50);
        let result = get_image_preview(&small, 1400).unwrap();
        assert_eq!(result["width"], 100, "a small picture must not be enlarged");
        assert_eq!(result["height"], 50);

        let big = dir.write_png("big.png", 3000, 1500);
        let result = get_image_preview(&big, 1400).unwrap();
        assert_eq!(result["width"], 1400);
        assert_eq!(result["height"], 700);
    }

    #[test]
    fn preview_of_a_missing_file_is_not_found() {
        let err = get_image_preview("Z:/no-such-image.png", 1400).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::NotFound);
        assert!(err.to_string().starts_with("Image not found:"));
    }

    /// A file that exists but is not an image is `invalid`, not `not_found` — the
    /// distinction is what lets the UI say something useful.
    #[test]
    fn preview_of_a_file_that_is_not_an_image_is_invalid() {
        let dir = Dir::new("garbage");
        dir.touch("fake.png");
        let path = dir.0.join("fake.png").to_string_lossy().into_owned();
        assert_eq!(
            get_image_preview(&path, 1400).unwrap_err().kind(),
            ErrorKind::Invalid
        );
    }

    /// A picture whose extension lies must still open, because a great many do.
    ///
    /// 470 of 4948 files in one real wallpaper folder — 9.5% — were named for a format
    /// they were not. `image::open` decides from the extension and hands a WebP to the
    /// JPEG decoder, which reports `Illegal start bytes: 5249`; the `RI` there is the
    /// front of a RIFF header. Pillow always sniffed the content, so this was a
    /// regression the port introduced, and it showed up only in composing because
    /// thumbnails already went through the sniffing path.
    #[test]
    fn a_file_whose_extension_lies_still_opens() {
        let dir = Dir::new("mislabelled");
        let png_bytes = {
            let buffer = image::RgbImage::from_fn(8, 8, |x, _| image::Rgb([x as u8 * 8, 40, 200]));
            let mut out = Vec::new();
            DynamicImage::ImageRgb8(buffer)
                .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
                .unwrap();
            out
        };
        // A PNG called .jpg — the same lie shape as a WebP called .jpeg.
        let liar = dir.0.join("actually-a-png.jpg");
        std::fs::write(&liar, &png_bytes).unwrap();

        // What the composer used to do, and why it failed on a tenth of the folder.
        assert!(
            image::open(&liar).is_err(),
            "image::open is supposed to trust the extension; if this ever starts \
             passing, the sniffing wrapper is no longer load-bearing"
        );

        let opened = open_image(&liar).expect("the sniffing loader must not care");
        assert_eq!((opened.width(), opened.height()), (8, 8));
    }

    #[test]
    fn round_half_even_matches_python() {
        assert_eq!(round_half_even(2.5), 2, "Python rounds half to even");
        assert_eq!(round_half_even(3.5), 4);
        assert_eq!(round_half_even(2.4), 2);
        assert_eq!(round_half_even(2.6), 3);
    }

    // ── the awkward-image corpus ─────────────────────────────────────────────
    //
    // Every format test above this line covers a format that *works*. That is the
    // gap a real folder walked straight through: the failures got in, and the suite
    // had nothing to say about them. These cover the shapes that are odd but valid,
    // and the shapes that are broken — the second group asserting an *error* rather
    // than a panic, because `get_thumbnails` and `compose_collage` both have to
    // survive them.
    //
    // Two forms are missing on purpose. A CMYK JPEG and a progressive JPEG cannot be
    // written by the `image` crate, so pinning them needs committed fixtures rather
    // than a generated file, and a fixture nobody can regenerate is what the goldens
    // already are. Noted rather than faked.

    /// Encode an image to `format` in memory, the way an awkward file arrives.
    fn encoded(source: &DynamicImage, format: image::ImageFormat) -> Vec<u8> {
        let mut out = Vec::new();
        source.write_to(&mut Cursor::new(&mut out), format).unwrap();
        out
    }

    #[test]
    fn odd_but_valid_colour_types_all_open() {
        let dir = Dir::new("colour-types");

        // Greyscale with alpha, 16-bit, and RGBA — none of which the rest of the
        // suite produces, and all of which `to_rgb8` has to flatten.
        let luma_a = DynamicImage::ImageLumaA8(image::GrayAlphaImage::from_fn(6, 4, |x, y| {
            image::LumaA([(x * 40) as u8, (y * 60) as u8])
        }));
        let deep = DynamicImage::ImageRgb16(image::ImageBuffer::from_fn(6, 4, |x, y| {
            image::Rgb([(x * 9000) as u16, (y * 9000) as u16, 65535])
        }));
        let rgba = DynamicImage::ImageRgba8(image::RgbaImage::from_fn(6, 4, |x, y| {
            image::Rgba([(x * 40) as u8, (y * 60) as u8, 10, 128])
        }));
        let grey = DynamicImage::ImageLuma8(image::GrayImage::from_fn(6, 4, |x, _| {
            image::Luma([(x * 40) as u8])
        }));

        for (name, bytes) in [
            ("grey-alpha.png", encoded(&luma_a, image::ImageFormat::Png)),
            ("sixteen-bit.png", encoded(&deep, image::ImageFormat::Png)),
            ("with-alpha.png", encoded(&rgba, image::ImageFormat::Png)),
            ("greyscale.jpg", encoded(&grey, image::ImageFormat::Jpeg)),
            ("lossless.webp", encoded(&rgba, image::ImageFormat::WebP)),
        ] {
            let path = dir.0.join(name);
            std::fs::write(&path, &bytes).unwrap();
            let opened = open_image(&path).unwrap_or_else(|e| panic!("{name}: {}", e.message()));
            assert_eq!((opened.width(), opened.height()), (6, 4), "{name}");
            // The whole pipeline flattens to RGB before it resizes, so that is the
            // conversion that has to survive, not merely the decode.
            assert_eq!(opened.to_rgb8().dimensions(), (6, 4), "{name}");
        }
    }

    #[test]
    fn broken_files_are_reported_rather_than_panicking() {
        let dir = Dir::new("broken");
        let whole = encoded(
            &DynamicImage::ImageRgb8(image::RgbImage::from_fn(32, 32, |x, y| {
                image::Rgb([x as u8, y as u8, 90])
            })),
            image::ImageFormat::Png,
        );

        let cases: Vec<(&str, Vec<u8>)> = vec![
            // Cut off mid-stream: the header sniffs as a PNG, the pixels are missing.
            ("truncated.png", whole[..whole.len() / 3].to_vec()),
            ("empty.png", Vec::new()),
            (
                "text-pretending.jpg",
                b"this is not an image at all".to_vec(),
            ),
            // A real format we carry no decoder for. It must fail as an image rather
            // than as a crash: `UNSUPPORTED` keeps it out of the listing, but a
            // pinned selection can still name one.
            (
                "photo.heic",
                b"\x00\x00\x00\x18ftypheic\x00\x00\x00\x00".to_vec(),
            ),
        ];

        for (name, bytes) in cases {
            let path = dir.0.join(name);
            std::fs::write(&path, &bytes).unwrap();
            let error = open_image(&path)
                .err()
                .unwrap_or_else(|| panic!("{name} should not have decoded"));
            assert_eq!(error.kind(), ErrorKind::Invalid, "{name}");
            assert!(
                error.message().contains(name),
                "{name}: the message must name the file, got {:?}",
                error.message()
            );
        }
    }

    #[test]
    fn a_broken_picture_costs_its_own_tile_and_says_which() {
        let dir = Dir::new("thumb-failures");
        let good = dir.write_png("fine.png", 40, 30);
        let bad = dir.0.join("torn.png").to_string_lossy().into_owned();
        std::fs::write(&bad, b"\x89PNG\r\n\x1a\n and then nothing").unwrap();

        let result = get_thumbnails(&[good.clone(), bad.clone()], 16);
        let thumbs = result["thumbnails"].as_object().unwrap();
        assert!(
            thumbs.contains_key(&good),
            "the readable picture still arrives"
        );
        assert!(!thumbs.contains_key(&bad));

        // The whole point: the failure is named, not merely absent.
        let failed = result["failed"].as_array().unwrap();
        assert_eq!(failed.len(), 1, "got {failed:?}");
        assert_eq!(failed[0]["path"].as_str(), Some(bad.as_str()));
        assert!(
            failed[0]["reason"].as_str().is_some_and(|r| !r.is_empty()),
            "a failure without a reason is the silence this replaced"
        );
    }

    #[test]
    fn a_batch_that_is_entirely_fine_reports_nothing_failed() {
        let dir = Dir::new("thumb-clean");
        let one = dir.write_png("a.png", 20, 20);
        let result = get_thumbnails(&[one], 16);
        assert_eq!(result["failed"].as_array().map(Vec::len), Some(0));
    }

    #[test]
    fn a_listing_says_how_many_pictures_it_could_not_open() {
        let dir = Dir::new("skipped");
        dir.touch("keep.png");
        dir.touch("keep2.jpg");
        for name in [
            "shot.heic",
            "other.HEIC",
            "raw.avif",
            "scan.tiff",
            "camera.cr2",
        ] {
            dir.touch(name);
        }
        // Not pictures, so not the user's missing wallpapers either.
        for name in ["notes.txt", "desktop.ini", "clip.mp4"] {
            dir.touch(name);
        }

        let listed = list_folder_images(&dir.0.to_string_lossy());
        assert_eq!(listed["count"].as_u64(), Some(2));
        assert_eq!(listed["skipped"].as_u64(), Some(5));

        let formats = listed["skipped_formats"].as_object().unwrap();
        // Case folds, so `.HEIC` and `.heic` are one entry of two rather than two.
        assert_eq!(formats["heic"].as_u64(), Some(2));
        assert_eq!(formats["avif"].as_u64(), Some(1));
        assert_eq!(formats.len(), 4, "got {formats:?}");
        assert!(
            !formats.contains_key("mp4"),
            "a video is not a skipped picture"
        );
    }

    #[test]
    fn a_folder_with_nothing_skipped_still_answers_the_question() {
        let dir = Dir::new("skipped-none");
        dir.touch("a.png");
        let listed = list_folder_images(&dir.0.to_string_lossy());
        // Present rather than absent: the interface renders one shape either way.
        assert_eq!(listed["skipped"].as_u64(), Some(0));
        assert!(listed["skipped_formats"].is_object());
    }

    #[test]
    fn no_extension_is_claimed_by_both_tables() {
        for ext in UNSUPPORTED {
            assert!(
                !SUPPORTED.contains(ext),
                "{ext} cannot be both openable and skipped"
            );
            assert!(
                !VIDEO_EXTENSIONS.contains(ext),
                "{ext} is a video, and videos are not skipped pictures"
            );
            assert_eq!(
                ext.to_ascii_lowercase(),
                **ext,
                "{ext} must be lowercase to match"
            );
        }
    }
}
