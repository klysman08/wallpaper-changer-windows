//! Grid layout, image fitting, selection, and composing the collage.
//!
//! Ports `wallpaper.py`'s layout and composition half plus `image_utils.py`'s fitting
//! and selection. Every number here is integer arithmetic copied literally: an
//! off-by-one shifts a crop, and a changed grid moves pictures between screens.
//!
//! [`plan_collage`] is the single source of truth for which image lands in which
//! rectangle. `compose_collage` draws from it and the `preview` RPC returns it as
//! `cells`, so the UI can lay a hit target over every picture without knowing the
//! grid rules. A second implementation on either side would drift the moment the
//! column table changes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use image::{imageops::FilterType, RgbImage};
use serde_json::{json, Value};

use crate::monitor::{virtual_desktop, Monitor};
use crate::CoreError;

/// Columns per image count, then `ceil(sqrt(n))`. Hard-coded rather than derived,
/// because the shapes were chosen by eye.
fn columns_for(n: usize) -> usize {
    match n {
        1 => 1,
        2 => 2,
        3 => 2,
        4 => 2,
        5 => 3,
        6 => 3,
        7 => 4,
        8 => 4,
        9 => 3,
        other => ((other as f64).sqrt().ceil() as usize).max(1),
    }
}

/// One cell of the grid: position and size in pixels within a single screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Cell {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

/// Split a `w` x `h` area into `n` cells.
///
/// Two quirks are deliberate. `h / rows` truncates, so a height that does not divide
/// evenly leaves a black strip along the bottom rather than stretching a row. And a
/// final row with fewer cells than the others is **centred**, using the leftover of
/// `w / row_cols`.
fn grid_layout(n: usize, w: i32, h: i32) -> Vec<Cell> {
    let cols = columns_for(n);
    let rows = n.div_ceil(cols);
    let cell_h = h / rows as i32;

    let mut cells = Vec::with_capacity(n);
    let mut placed = 0usize;
    for r in 0..rows {
        let row_cols = cols.min(n - placed);
        if row_cols == 0 {
            break;
        }
        let cell_w = w / row_cols as i32;
        let offset_x = (w - row_cols as i32 * cell_w) / 2;
        for c in 0..row_cols {
            cells.push(Cell {
                x: offset_x + c as i32 * cell_w,
                y: r as i32 * cell_h,
                width: cell_w,
                height: cell_h,
            });
            placed += 1;
        }
    }
    cells
}

/// Where each image of a collage lands on the virtual desktop.
///
/// Coordinates are pixels within the composite — relative to the top-left of the
/// virtual desktop rather than to any one screen.
pub fn plan_collage(monitors: &[Monitor], count: usize, same_for_all: bool) -> Vec<Value> {
    let Ok((min_x, min_y, _, _)) = virtual_desktop(monitors) else {
        return Vec::new();
    };
    let mut cells = Vec::new();
    let mut image_index = 0usize;
    for monitor in monitors {
        for (j, cell) in grid_layout(count, monitor.width, monitor.height).into_iter().enumerate() {
            cells.push(json!({
                "monitor": monitor.index,
                // Which entry of the image list fills this cell. With same_for_all
                // every monitor repeats the same short list.
                "image_index": if same_for_all { j } else { image_index },
                "x": (monitor.x - min_x) + cell.x,
                "y": (monitor.y - min_y) + cell.y,
                "width": cell.width,
                "height": cell.height,
            }));
            if !same_for_all {
                image_index += 1;
            }
        }
    }
    cells
}

// ── fitting ──────────────────────────────────────────────────────────────────

/// Scale and crop a picture into a `target_w` x `target_h` box.
///
/// Modes, matching `image_utils.fit_image`:
/// - `fill` — scale to cover, centre-crop the excess
/// - `fit` — scale to contain, letterbox with black
/// - `stretch` — resize to the exact size, distorting
/// - `center` — no scaling, centred on black
/// - `span` — an alias of `fill`
///
/// All the arithmetic truncates, exactly as Python's `int()` and `//` do.
pub fn fit_image(source: &RgbImage, target_w: i32, target_h: i32, mode: &str) -> RgbImage {
    let mode = if mode == "span" { "fill" } else { mode };
    let (tw, th) = (target_w.max(1) as u32, target_h.max(1) as u32);
    let (sw, sh) = source.dimensions();

    if mode == "stretch" {
        return image::imageops::resize(source, tw, th, FilterType::Lanczos3);
    }

    if mode == "center" {
        let mut canvas = RgbImage::new(tw, th);
        let offset_x = (tw as i32 - sw as i32) / 2;
        let offset_y = (th as i32 - sh as i32) / 2;
        paste(&mut canvas, source, offset_x, offset_y);
        return canvas;
    }

    let ratio_w = tw as f64 / sw as f64;
    let ratio_h = th as f64 / sh as f64;
    let ratio = if mode == "fill" {
        ratio_w.max(ratio_h)
    } else {
        ratio_w.min(ratio_h)
    };
    // Python: int(src * ratio) — truncation, not rounding.
    let new_w = ((sw as f64 * ratio) as u32).max(1);
    let new_h = ((sh as f64 * ratio) as u32).max(1);
    let scaled = image::imageops::resize(source, new_w, new_h, FilterType::Lanczos3);

    if mode == "fill" {
        let left = (new_w as i32 - tw as i32) / 2;
        let top = (new_h as i32 - th as i32) / 2;
        crop(&scaled, left, top, tw, th)
    } else {
        let mut canvas = RgbImage::new(tw, th);
        let offset_x = (tw as i32 - new_w as i32) / 2;
        let offset_y = (th as i32 - new_h as i32) / 2;
        paste(&mut canvas, &scaled, offset_x, offset_y);
        canvas
    }
}

/// Paste `source` at `(x, y)`, clipping anything outside the canvas.
///
/// Pillow's `paste` silently clips rather than failing, and `center` mode relies on
/// that for a picture larger than its cell.
pub(crate) fn paste(canvas: &mut RgbImage, source: &RgbImage, x: i32, y: i32) {
    let (cw, ch) = canvas.dimensions();
    let (sw, sh) = source.dimensions();
    for sy in 0..sh {
        let ty = y + sy as i32;
        if ty < 0 || ty >= ch as i32 {
            continue;
        }
        for sx in 0..sw {
            let tx = x + sx as i32;
            if tx < 0 || tx >= cw as i32 {
                continue;
            }
            canvas.put_pixel(tx as u32, ty as u32, *source.get_pixel(sx, sy));
        }
    }
}

/// Crop a `width` x `height` rectangle, filling anything out of bounds with black —
/// which is what Pillow's `crop` does.
fn crop(source: &RgbImage, left: i32, top: i32, width: u32, height: u32) -> RgbImage {
    let (sw, sh) = source.dimensions();
    let mut out = RgbImage::new(width, height);
    for y in 0..height {
        let sy = top + y as i32;
        if sy < 0 || sy >= sh as i32 {
            continue;
        }
        for x in 0..width {
            let sx = left + x as i32;
            if sx < 0 || sx >= sw as i32 {
                continue;
            }
            out.put_pixel(x, y, *source.get_pixel(sx as u32, sy as u32));
        }
    }
    out
}

/// The part of a composite that lands on one screen.
pub fn crop_to_monitor(
    canvas: &RgbImage,
    monitors: &[Monitor],
    index: usize,
) -> Result<RgbImage, CoreError> {
    let target = monitors
        .iter()
        .find(|m| m.index == index)
        .ok_or_else(|| CoreError::invalid(format!("No monitor #{}.", index + 1)))?;
    let (min_x, min_y, _, _) = virtual_desktop(monitors)?;
    Ok(crop(
        canvas,
        target.x - min_x,
        target.y - min_y,
        target.width.max(1) as u32,
        target.height.max(1) as u32,
    ))
}

// ── composition ──────────────────────────────────────────────────────────────

/// Compose the collage canvas **without** touching the desktop.
///
/// `preset_images` replays an exact selection — what a preview reported — instead of
/// picking a fresh one. `state_file` lets a caller that must not disturb the rotation
/// history (the preview, and saving) point the selection state somewhere throwaway.
pub fn compose_collage(
    cfg: &Value,
    monitors: &[Monitor],
    preset_images: Option<&[String]>,
    state_file: Option<&Path>,
) -> Result<(RgbImage, Vec<String>), CoreError> {
    let folder = crate::config::resolve_path(
        cfg.pointer("/paths/wallpapers_folder")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        None,
    );
    let fit_mode = cfg
        .pointer("/display/fit_mode")
        .and_then(Value::as_str)
        .unwrap_or("fill")
        .to_string();
    let selection = cfg
        .pointer("/general/selection")
        .and_then(Value::as_str)
        .unwrap_or("random")
        .to_string();
    let count = collage_count(cfg);
    let same_for_all = same_for_all(cfg);

    let chosen: Vec<PathBuf> = match preset_images {
        Some(preset) if !preset.is_empty() => preset.iter().map(PathBuf::from).collect(),
        _ => {
            let wanted = if same_for_all { count } else { count * monitors.len() };
            crate::selection::pick_images(&folder, wanted, &selection, state_file)?
        }
    };
    if chosen.is_empty() {
        return Err(CoreError::not_found(format!(
            "No images in: {}",
            folder.display()
        )));
    }

    let (_, _, total_w, total_h) = virtual_desktop(monitors)?;
    let mut canvas = RgbImage::new(total_w.max(1) as u32, total_h.max(1) as u32);

    // With same_for_all every monitor repeats the same list, so the same picture is
    // fitted to the same cell size several times. Cache by (image, size).
    let mut fitted: HashMap<(usize, i32, i32), RgbImage> = HashMap::new();

    for cell in plan_collage(monitors, count, same_for_all) {
        let index = cell["image_index"].as_u64().unwrap_or(0) as usize;
        // A caller-supplied list can be shorter than the grid — the preview lets the
        // user edit the selection, and the count can change under it. Wrapping draws
        // a repeated picture instead of failing the whole composition.
        let source_index = index % chosen.len();
        let (w, h) = (
            cell["width"].as_i64().unwrap_or(0) as i32,
            cell["height"].as_i64().unwrap_or(0) as i32,
        );

        let key = (source_index, w, h);
        if !fitted.contains_key(&key) {
            let opened = image::open(&chosen[source_index])
                .map_err(|e| {
                    CoreError::invalid(format!(
                        "Could not read {}: {e}",
                        chosen[source_index].display()
                    ))
                })?
                .to_rgb8();
            fitted.insert(key, fit_image(&opened, w, h, &fit_mode));
        }
        let piece = &fitted[&key];
        paste(
            &mut canvas,
            piece,
            cell["x"].as_i64().unwrap_or(0) as i32,
            cell["y"].as_i64().unwrap_or(0) as i32,
        );
    }

    let effect = cfg
        .pointer("/display/effect")
        .and_then(Value::as_str)
        .unwrap_or("normal");
    let canvas = crate::effects::apply_effect(&canvas, effect)?;

    let used = chosen
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    Ok((canvas, used))
}

/// `general.collage_count`, at least 1.
pub fn collage_count(cfg: &Value) -> usize {
    cfg.pointer("/general/collage_count")
        .and_then(Value::as_i64)
        .unwrap_or(4)
        .max(1) as usize
}

/// `general.collage_same_for_all`.
pub fn same_for_all(cfg: &Value) -> bool {
    cfg.pointer("/general/collage_same_for_all")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// The subset of a selection that ends up on one screen.
///
/// Derived from the same layout `compose_collage` drew from, wrap-around included,
/// so a saved crop lists exactly the pictures inside it. First-seen order, no
/// repeats — the list is a caption, not a tally.
pub fn images_on(
    cfg: &Value,
    monitors: &[Monitor],
    monitor: usize,
    used: &[String],
) -> Vec<String> {
    if used.is_empty() {
        return Vec::new();
    }
    let cells = plan_collage(cfg_count_cells(cfg, monitors), collage_count(cfg), same_for_all(cfg));
    let mut seen = Vec::new();
    for cell in cells {
        if cell["monitor"].as_u64() != Some(monitor as u64) {
            continue;
        }
        let index = cell["image_index"].as_u64().unwrap_or(0) as usize;
        if !seen.contains(&index) {
            seen.push(index);
        }
    }
    seen.into_iter().map(|i| used[i % used.len()].clone()).collect()
}

/// Kept as a named helper so `images_on` reads as one expression.
fn cfg_count_cells<'a>(_cfg: &Value, monitors: &'a [Monitor]) -> &'a [Monitor] {
    monitors
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(index: usize, x: i32, y: i32, w: i32, h: i32) -> Monitor {
        Monitor { index, x, y, width: w, height: h, name: format!("D{index}") }
    }

    fn cells_of(n: usize, w: i32, h: i32) -> Vec<(i32, i32, i32, i32)> {
        grid_layout(n, w, h)
            .into_iter()
            .map(|c| (c.x, c.y, c.width, c.height))
            .collect()
    }

    #[test]
    fn the_column_table_is_the_one_from_python() {
        for (n, cols) in [(1, 1), (2, 2), (3, 2), (4, 2), (5, 3), (6, 3), (7, 4), (8, 4), (9, 3)] {
            assert_eq!(columns_for(n), cols, "n={n}");
        }
        // Beyond the table it is ceil(sqrt(n)).
        assert_eq!(columns_for(10), 4);
        assert_eq!(columns_for(16), 4);
        assert_eq!(columns_for(17), 5);
    }

    #[test]
    fn one_image_fills_the_screen() {
        assert_eq!(cells_of(1, 1920, 1080), [(0, 0, 1920, 1080)]);
    }

    #[test]
    fn four_images_make_a_two_by_two() {
        assert_eq!(
            cells_of(4, 1920, 1080),
            [(0, 0, 960, 540), (960, 0, 960, 540), (0, 540, 960, 540), (960, 540, 960, 540)]
        );
    }

    /// A height that does not divide evenly leaves a black strip at the bottom
    /// rather than stretching the last row.
    #[test]
    fn an_uneven_height_leaves_a_strip_at_the_bottom() {
        let cells = cells_of(2, 100, 101); // 2 cols, 1 row -> cell_h = 101
        assert_eq!(cells[0].3, 101);
        let cells = cells_of(3, 100, 101); // 2 cols, 2 rows -> cell_h = 50, 100 covered
        assert_eq!(cells[0].3, 50);
        assert_eq!(cells.last().unwrap().1 + cells.last().unwrap().3, 100);
    }

    /// Three images are two columns over two rows, and the lone last cell is centred.
    #[test]
    fn a_short_last_row_is_centred() {
        let cells = cells_of(3, 1000, 1000);
        assert_eq!(cells[0], (0, 0, 500, 500));
        assert_eq!(cells[1], (500, 0, 500, 500));
        // Last row has one cell of the full width, so the offset is zero.
        assert_eq!(cells[2], (0, 500, 1000, 500));

        // Five images: 3 columns, so the last row of two is offset.
        let cells = cells_of(5, 900, 900);
        assert_eq!(cells[3].0, (900 - 2 * 450) / 2);
        assert_eq!(cells[3].2, 450);
    }

    #[test]
    fn every_count_from_one_to_nine_produces_that_many_cells() {
        for n in 1..=9 {
            assert_eq!(cells_of(n, 1920, 1080).len(), n, "n={n}");
        }
    }

    #[test]
    fn plan_offsets_cells_by_the_monitor_origin() {
        let monitors = [mon(0, 0, 0, 100, 100), mon(1, 100, -50, 100, 100)];
        let cells = plan_collage(&monitors, 1, false);
        assert_eq!(cells.len(), 2);
        // min_y is -50, so the first screen sits 50 down inside the composite.
        assert_eq!(cells[0]["x"], 0);
        assert_eq!(cells[0]["y"], 50);
        assert_eq!(cells[1]["x"], 100);
        assert_eq!(cells[1]["y"], 0);
    }

    #[test]
    fn sharing_one_list_repeats_the_indices_per_monitor() {
        let monitors = [mon(0, 0, 0, 100, 100), mon(1, 100, 0, 100, 100)];
        let shared: Vec<u64> = plan_collage(&monitors, 2, true)
            .iter()
            .map(|c| c["image_index"].as_u64().unwrap())
            .collect();
        assert_eq!(shared, [0, 1, 0, 1]);

        let distinct: Vec<u64> = plan_collage(&monitors, 2, false)
            .iter()
            .map(|c| c["image_index"].as_u64().unwrap())
            .collect();
        assert_eq!(distinct, [0, 1, 2, 3]);
    }

    fn gradient(w: u32, h: u32) -> RgbImage {
        RgbImage::from_fn(w, h, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, 90]))
    }

    #[test]
    fn stretch_matches_the_box_exactly() {
        let out = fit_image(&gradient(100, 50), 80, 80, "stretch");
        assert_eq!(out.dimensions(), (80, 80));
    }

    #[test]
    fn fill_covers_the_box_and_span_is_its_alias() {
        let source = gradient(200, 100);
        let filled = fit_image(&source, 100, 100, "fill");
        assert_eq!(filled.dimensions(), (100, 100));
        assert_eq!(fit_image(&source, 100, 100, "span"), filled);
    }

    /// `fit` letterboxes: a wide picture in a square box leaves black top and bottom.
    #[test]
    fn fit_letterboxes_with_black() {
        let out = fit_image(&gradient(200, 100), 100, 100, "fit");
        assert_eq!(out.dimensions(), (100, 100));
        assert_eq!(out.get_pixel(50, 0).0, [0, 0, 0], "top should be letterboxed");
        assert_eq!(out.get_pixel(50, 99).0, [0, 0, 0], "bottom should be letterboxed");
        assert_ne!(out.get_pixel(50, 50).0, [0, 0, 0], "middle should be the picture");
    }

    #[test]
    fn center_does_not_scale() {
        let out = fit_image(&gradient(40, 20), 100, 100, "center");
        assert_eq!(out.dimensions(), (100, 100));
        assert_eq!(out.get_pixel(0, 0).0, [0, 0, 0], "corner is background");
        // The picture sits at ((100-40)/2, (100-20)/2) = (30, 40).
        assert_eq!(out.get_pixel(30, 40), gradient(40, 20).get_pixel(0, 0));
    }

    /// A picture bigger than the box in `center` mode is clipped, not resized.
    #[test]
    fn center_clips_an_oversized_picture() {
        let out = fit_image(&gradient(200, 200), 50, 50, "center");
        assert_eq!(out.dimensions(), (50, 50));
    }

    #[test]
    fn cropping_to_a_monitor_takes_its_rectangle() {
        let monitors = [mon(0, 0, 0, 4, 4), mon(1, 4, 0, 6, 4)];
        let canvas = RgbImage::from_fn(10, 4, |x, _| image::Rgb([x as u8, 0, 0]));
        let left = crop_to_monitor(&canvas, &monitors, 0).unwrap();
        assert_eq!(left.dimensions(), (4, 4));
        assert_eq!(left.get_pixel(0, 0).0[0], 0);

        let right = crop_to_monitor(&canvas, &monitors, 1).unwrap();
        assert_eq!(right.dimensions(), (6, 4));
        assert_eq!(right.get_pixel(0, 0).0[0], 4, "should start at the monitor origin");
    }

    #[test]
    fn cropping_to_a_monitor_that_is_not_there_is_invalid() {
        let monitors = [mon(0, 0, 0, 4, 4)];
        let canvas = RgbImage::new(4, 4);
        let err = crop_to_monitor(&canvas, &monitors, 7).unwrap_err();
        assert_eq!(err.kind(), crate::ErrorKind::Invalid);
        assert_eq!(err.to_string(), "No monitor #8.");
    }

    #[test]
    fn images_on_lists_each_picture_once_in_first_seen_order() {
        let monitors = [mon(0, 0, 0, 100, 100), mon(1, 100, 0, 100, 100)];
        let cfg = json!({ "general": { "collage_count": 2, "collage_same_for_all": false } });
        let used: Vec<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();

        assert_eq!(images_on(&cfg, &monitors, 0, &used), ["a", "b"]);
        assert_eq!(images_on(&cfg, &monitors, 1, &used), ["c", "d"]);
    }

    /// A selection shorter than the grid wraps, and the caption repeats with it.
    ///
    /// `dict.fromkeys` in Python dedupes the *cell indices*, not the paths they
    /// resolve to — four cells over a one-image selection give four identical
    /// entries, one per cell. Surprising, but it is the observed behaviour and the
    /// gallery caption is built from it.
    #[test]
    fn images_on_wraps_a_short_selection_once_per_cell() {
        let monitors = [mon(0, 0, 0, 100, 100)];
        let cfg = json!({ "general": { "collage_count": 4 } });
        let used = vec!["only.png".to_string()];
        assert_eq!(images_on(&cfg, &monitors, 0, &used).len(), 4);

        // Two pictures across four cells alternate rather than collapsing.
        let used = vec!["a.png".to_string(), "b.png".to_string()];
        assert_eq!(images_on(&cfg, &monitors, 0, &used), ["a.png", "b.png", "a.png", "b.png"]);
    }

    #[test]
    fn images_on_of_an_empty_selection_is_empty() {
        let monitors = [mon(0, 0, 0, 100, 100)];
        assert!(images_on(&json!({}), &monitors, 0, &[]).is_empty());
    }
}
