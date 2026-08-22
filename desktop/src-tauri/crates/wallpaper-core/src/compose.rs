//! The two RPC methods that compose a collage without touching the desktop.
//!
//! Ports `rpc.py`'s `preview` and `save_collage`. Both go through
//! [`crate::collage::compose_collage`], and both point the selection state at a
//! throwaway file so that looking at a collage never consumes the rotation the
//! desktop is following.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use image::{codecs::png::PngEncoder, ImageEncoder, RgbImage};
use serde_json::{json, Value};

use crate::collage::{
    collage_count, compose_collage, crop_to_monitor, images_on, plan_collage, same_for_all,
};
use crate::{gallery, monitor, CoreError};

/// Where a preview's selection state goes.
///
/// Deliberately outside the config directory: browsing previews must not advance
/// the no-repeat cycle the applied wallpaper follows.
fn preview_state_file() -> PathBuf {
    std::env::temp_dir().join("wallpaper_changer_preview_state.json")
}

/// Formats `save_collage` will write, keyed by extension. Mirrors `_SAVE_FORMATS`.
fn format_for(extension: &str) -> Option<image::ImageFormat> {
    match extension.to_ascii_lowercase().as_str() {
        "png" => Some(image::ImageFormat::Png),
        "jpg" | "jpeg" => Some(image::ImageFormat::Jpeg),
        "bmp" => Some(image::ImageFormat::Bmp),
        // WebP is decode-only in the `image` crate, so it is not offered here.
        // Python accepted it; see the phase notes.
        _ => None,
    }
}

/// `preview` — render the collage to a base64 PNG, leaving the desktop alone.
pub fn preview(cfg: &Value, max_width: i64, images: Option<&[String]>) -> Result<Value, CoreError> {
    let monitors = monitor::get_monitors()?;
    if monitors.is_empty() {
        return Err(CoreError::no_monitors("No monitors detected."));
    }

    let state = preview_state_file();
    let (canvas, used) = compose_collage(cfg, &monitors, images, Some(&state))?;

    // Reported in *composite* pixels, alongside the picture they describe, so the UI
    // can lay a hit target over every image without knowing the grid rules. Computed
    // before the downscale below.
    let cells = plan_collage(&monitors, collage_count(cfg), same_for_all(cfg));

    let shown = if max_width > 0 && canvas.width() > max_width as u32 {
        let ratio = max_width as f64 / canvas.width() as f64;
        let height = ((canvas.height() as f64 * ratio) as u32).max(1);
        crate::images::resize_lanczos3(&canvas, max_width as u32, height)
    } else {
        canvas
    };

    Ok(json!({
        "png_base64": encode_png(&shown)?,
        "width": shown.width(),
        "height": shown.height(),
        "images": used,
        "cells": cells,
    }))
}

fn encode_png(image: &RgbImage) -> Result<String, CoreError> {
    let mut bytes = Vec::new();
    PngEncoder::new(&mut Cursor::new(&mut bytes))
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|e| CoreError::io(format!("Could not encode the preview: {e}")))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}

/// `save_collage` — write the collage to an image file, leaving the desktop alone.
///
/// Composed at full resolution rather than from the preview's downscaled PNG: the
/// preview is sized for a window, and a saved picture should be worth keeping.
/// `monitor` saves one screen's share; `None` saves the whole virtual desktop.
pub fn save_collage(
    cfg: &Value,
    images: Option<&[String]>,
    monitor_index: Option<i64>,
    path: Option<&str>,
) -> Result<Value, CoreError> {
    let monitors = monitor::get_monitors()?;
    if monitors.is_empty() {
        return Err(CoreError::no_monitors("No monitors detected."));
    }
    if let Some(index) = monitor_index {
        if !monitors.iter().any(|m| m.index as i64 == index) {
            return Err(CoreError::invalid(format!("No monitor #{}.", index + 1)));
        }
    }

    let state = preview_state_file();
    let (canvas, used) = compose_collage(cfg, &monitors, images, Some(&state))?;

    let (canvas, used) = match monitor_index {
        Some(index) => (
            crop_to_monitor(&canvas, &monitors, index as usize)?,
            images_on(cfg, &monitors, index as usize, &used),
        ),
        None => (canvas, used),
    };

    let target = match path {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => gallery::library_dir(cfg).join(gallery::suggest_name(monitor_index, ".png")),
    };

    let extension = target
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_default();
    let format = format_for(&extension).ok_or_else(|| {
        let shown = if extension.is_empty() {
            target
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        } else {
            format!(".{extension}")
        };
        CoreError::invalid(format!("Unsupported image format: {shown}"))
    })?;

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| CoreError::io(format!("Could not save the image: {e}")))?;
    }
    write_image(&canvas, &target, format)?;

    let entry = gallery::record(
        &target.to_string_lossy(),
        monitor_index,
        &used,
        canvas.width() as i64,
        canvas.height() as i64,
    )?;
    Ok(json!({ "collage": entry }))
}

fn write_image(
    canvas: &RgbImage,
    target: &Path,
    format: image::ImageFormat,
) -> Result<(), CoreError> {
    let file = std::fs::File::create(target)
        .map_err(|e| CoreError::io(format!("Could not save the image: {e}")))?;
    let mut writer = std::io::BufWriter::new(file);
    canvas
        .write_to(&mut writer, format)
        .map_err(|e| CoreError::io(format!("Could not save the image: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Sandbox;
    use crate::ErrorKind;

    fn seeded(sandbox: &Sandbox, count: usize) -> Value {
        let pics = sandbox.dir.join("pics");
        std::fs::create_dir_all(&pics).unwrap();
        for i in 0..count {
            let image = RgbImage::from_fn(60, 40, |x, y| {
                image::Rgb([(x * 4) as u8, (y * 6) as u8, (i * 40) as u8])
            });
            image.save(pics.join(format!("p{i}.png"))).unwrap();
        }
        json!({
            "paths": { "wallpapers_folder": pics.to_string_lossy() },
            "display": { "fit_mode": "fill", "effect": "normal" },
            "general": { "selection": "sequential", "collage_count": 2, "collage_same_for_all": false },
        })
    }

    #[test]
    fn unsupported_formats_are_rejected_and_supported_ones_are_not() {
        assert!(format_for("png").is_some());
        assert!(format_for("PNG").is_some());
        assert!(format_for("jpg").is_some());
        assert!(format_for("jpeg").is_some());
        assert!(format_for("bmp").is_some());
        assert!(format_for("gif").is_none());
        assert!(format_for("").is_none());
    }

    #[test]
    fn preview_returns_a_decodable_png_with_cells() {
        let sandbox = Sandbox::new("preview");
        let cfg = seeded(&sandbox, 4);
        let Ok(result) = preview(&cfg, 320, None) else {
            return; // no displays attached in this environment
        };

        let bytes = base64::engine::general_purpose::STANDARD
            .decode(result["png_base64"].as_str().unwrap())
            .unwrap();
        let decoded = image::load_from_memory(&bytes).expect("a decodable PNG");
        assert_eq!(decoded.width(), result["width"].as_u64().unwrap() as u32);
        assert!(decoded.width() <= 320, "the preview must respect max_width");
        assert!(!result["cells"].as_array().unwrap().is_empty());
        assert!(!result["images"].as_array().unwrap().is_empty());
    }

    /// The cells describe the *composite*, not the downscaled picture, so a UI can
    /// scale them itself. They must not shrink with max_width.
    #[test]
    fn preview_cells_are_in_composite_pixels() {
        let sandbox = Sandbox::new("cells");
        let cfg = seeded(&sandbox, 4);
        let (Ok(small), Ok(large)) = (preview(&cfg, 200, None), preview(&cfg, 1600, None)) else {
            return;
        };
        assert_eq!(small["cells"], large["cells"]);
    }

    #[test]
    fn preview_does_not_touch_the_rotation_history() {
        let sandbox = Sandbox::new("previewstate");
        let cfg = seeded(&sandbox, 4);
        let real_state = crate::config::state_file();
        if preview(&cfg, 200, None).is_err() {
            return;
        }
        assert!(
            !real_state.exists(),
            "the preview consumed the real selection state"
        );
    }

    #[test]
    fn saving_rejects_a_format_that_cannot_be_written() {
        let sandbox = Sandbox::new("badformat");
        let cfg = seeded(&sandbox, 2);
        let target = sandbox.dir.join("out.gif");
        match save_collage(&cfg, None, None, Some(&target.to_string_lossy())) {
            Err(e) if e.kind() == ErrorKind::NoMonitors => {}
            Err(e) => {
                assert_eq!(e.kind(), ErrorKind::Invalid);
                assert!(e.to_string().starts_with("Unsupported image format"), "{e}");
            }
            Ok(_) => panic!("a .gif should not have been written"),
        }
    }

    #[test]
    fn saving_rejects_a_monitor_that_is_not_attached() {
        let sandbox = Sandbox::new("badmonitor");
        let cfg = seeded(&sandbox, 2);
        let target = sandbox.dir.join("out.png");
        match save_collage(&cfg, None, Some(99), Some(&target.to_string_lossy())) {
            Err(e) if e.kind() == ErrorKind::NoMonitors => {}
            Err(e) => assert_eq!(e.kind(), ErrorKind::Invalid),
            Ok(_) => panic!("monitor 99 does not exist"),
        }
    }

    #[test]
    fn a_saved_collage_lands_on_disk_and_in_the_index() {
        let sandbox = Sandbox::new("save");
        let cfg = seeded(&sandbox, 4);
        let target = sandbox.dir.join("kept.png");
        let Ok(result) = save_collage(&cfg, None, None, Some(&target.to_string_lossy())) else {
            return;
        };

        assert!(target.is_file(), "the image was not written");
        let entry = &result["collage"];
        assert_eq!(entry["path"], target.to_string_lossy().as_ref());
        assert!(
            entry["monitor"].is_null(),
            "a desktop-wide save records a null monitor"
        );
        assert!(entry["width"].as_u64().unwrap() > 0);
        assert_eq!(gallery::entries().len(), 1);
    }

    /// The saved file is composed fresh at full resolution, not taken from the
    /// preview's window-sized PNG.
    #[test]
    fn a_saved_collage_is_full_resolution() {
        let sandbox = Sandbox::new("fullres");
        let cfg = seeded(&sandbox, 4);
        let target = sandbox.dir.join("full.png");
        let (Ok(saved), Ok(shown)) = (
            save_collage(&cfg, None, None, Some(&target.to_string_lossy())),
            preview(&cfg, 320, None),
        ) else {
            return;
        };
        let saved_width = saved["collage"]["width"].as_u64().unwrap();
        assert!(
            saved_width > shown["width"].as_u64().unwrap(),
            "the save should not be the preview's size"
        );
    }
}
