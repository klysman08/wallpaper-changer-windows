//! Folder listing and the two ways a picture reaches the webview.
//!
//! Ports the listing half of `image_utils.py`, `scan_video_folder` from
//! `video_wallpaper.py`, and `get_thumbnails` / `get_image_preview` from `rpc.py`.
//! Selection (`pick_images`) and fitting (`fit_image`) belong to the composition
//! phase and are not here.
//!
//! The webview cannot read local files, so a picture only ever reaches it as base64.
//! That is why these live in the engine at all.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageReader};
use serde_json::{json, Value};

use crate::CoreError;

/// Image extensions the engine will open. Mirrors `image_utils.SUPPORTED`.
pub const SUPPORTED: &[&str] = &["jpg", "jpeg", "png", "bmp", "webp"];

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

/// Round half away from zero, then clamp to at least one pixel.
///
/// `round()` in Python 3 is half-to-even, so 2.5 becomes 2 rather than 3. The
/// difference only shows on an exact .5, but it is free to match.
fn round_half_even(value: f64) -> i64 {
    let floor = value.floor();
    let diff = value - floor;
    let floor = floor as i64;
    if diff > 0.5 {
        floor + 1
    } else if diff < 0.5 {
        floor
    } else if floor % 2 == 0 {
        floor
    } else {
        floor + 1
    }
}

fn open_image(path: &str) -> Result<DynamicImage, CoreError> {
    if !Path::new(path).exists() {
        return Err(CoreError::not_found(format!("Image not found: {path}")));
    }
    ImageReader::open(path)
        .map_err(|e| CoreError::invalid(format!("Could not read the image: {e}")))?
        .with_guessed_format()
        .map_err(|e| CoreError::invalid(format!("Could not read the image: {e}")))?
        .decode()
        .map_err(|e| CoreError::invalid(format!("Could not read the image: {e}")))
}

fn encode_jpeg(image: &DynamicImage, quality: u8) -> Result<Vec<u8>, CoreError> {
    let rgb = image.to_rgb8();
    let mut buffer = Vec::new();
    JpegEncoder::new_with_quality(&mut Cursor::new(&mut buffer), quality)
        .encode_image(&rgb)
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
            if n == 0.0 { 0.0 } else { (aspect - x / n).abs() }
        }) as f64;
    }
    (x as u32, y as u32)
}

/// `list_folder_images` — every supported image in a folder.
pub fn list_folder_images(folder: &str) -> Value {
    let images = list_images(folder);
    json!({
        "count": images.len(),
        "images": images.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>(),
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
pub fn get_thumbnails(paths: &[String], size: i64) -> Value {
    let box_size = size.clamp(32, 512) as u32;
    let mut out = serde_json::Map::new();
    for raw in paths {
        let Ok(image) = open_image(raw) else { continue };
        let (w, h) = thumbnail_size(image.width(), image.height(), box_size);
        let thumb = if (w, h) == (image.width(), image.height()) {
            image
        } else {
            image.resize_exact(w, h, image::imageops::FilterType::Lanczos3)
        };
        let Ok(bytes) = encode_jpeg(&thumb, 75) else { continue };
        out.insert(raw.clone(), Value::String(to_base64(&bytes)));
    }
    json!({ "thumbnails": out })
}

/// `get_image_preview` — one picture, large enough to actually look at.
///
/// Downscale only: a small picture is sent as it is rather than blown up into a
/// bigger payload.
pub fn get_image_preview(path: &str, max_width: i64) -> Result<Value, CoreError> {
    let box_width = max_width.clamp(64, 4096) as u32;
    let image = open_image(path)?;

    let shown = if image.width() > box_width {
        let height = round_half_even(image.height() as f64 * box_width as f64 / image.width() as f64)
            .max(1) as u32;
        image.resize_exact(box_width, height, image::imageops::FilterType::Lanczos3)
    } else {
        image
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
        let bytes = base64::engine::general_purpose::STANDARD.decode(encoded).unwrap();
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
        assert_eq!(get_image_preview(&path, 1400).unwrap_err().kind(), ErrorKind::Invalid);
    }

    #[test]
    fn round_half_even_matches_python() {
        assert_eq!(round_half_even(2.5), 2, "Python rounds half to even");
        assert_eq!(round_half_even(3.5), 4);
        assert_eq!(round_half_even(2.4), 2);
        assert_eq!(round_half_even(2.6), 3);
    }
}
