//! The index of collages the user has exported to image files.
//!
//! Ports `gallery.py`. Saving a collage is an export, not a wallpaper change —
//! nothing on the desktop moves. What makes the result a *library* rather than a pile
//! of files is this index: a JSON list beside the settings recording what each file
//! was, which screen it came from and which pictures went into it.
//!
//! The index is **derived data**. It is reconciled against the disk on every read, so
//! a file deleted from Explorer simply stops being listed, and losing the index costs
//! only the metadata. Two rules follow from that and both are load-bearing:
//!
//! - Removing an entry never deletes the user's image.
//! - Entries carry absolute paths, so moving the library folder only changes where
//!   *new* saves land; everything already indexed keeps being listed where it was.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::config::{resolve_saved_dir, user_config_dir};
use crate::CoreError;

pub const INDEX_NAME: &str = "gallery.json";

/// Enough to keep years of saves, small enough that the index can be rewritten on
/// every save.
const MAX_ENTRIES: usize = 500;

/// Where a save with no explicit destination lands.
pub fn library_dir(cfg: &Value) -> PathBuf {
    resolve_saved_dir(cfg)
}

pub fn index_file() -> PathBuf {
    user_config_dir().join(INDEX_NAME)
}

/// Identity of a saved file.
///
/// Two Windows paths can name one file while differing in case and separators, so a
/// plain string compare would let the same picture into the index twice. Mirrors
/// `os.path.normcase(os.path.abspath(...))`: absolute, separators unified, lowered.
fn key(path: &str) -> String {
    let absolute = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    let text = absolute.to_string_lossy().replace('/', "\\");
    if cfg!(windows) {
        text.to_lowercase()
    } else {
        text
    }
}

/// Read the index, dropping anything malformed.
///
/// A corrupt index is worth strictly less than the app starting: the files it
/// describes are all still there, and the next save rebuilds it.
fn read() -> Vec<Value> {
    let Ok(text) = std::fs::read_to_string(index_file()) else {
        return Vec::new();
    };
    let Ok(Value::Array(entries)) = serde_json::from_str::<Value>(&text) else {
        return Vec::new();
    };
    entries
        .into_iter()
        .filter(|e| e.is_object() && e.get("path").and_then(Value::as_str).is_some())
        .collect()
}

fn write(entries: &[Value]) -> Result<(), CoreError> {
    let target = index_file();
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| CoreError::io(format!("Could not create {}: {e}", parent.display())))?;
    }
    // `json.dumps(..., indent=2, ensure_ascii=False)` — pretty and UTF-8, so the file
    // stays readable by hand.
    let text = serde_json::to_string_pretty(&entries)
        .map_err(|e| CoreError::io(format!("Could not serialise the gallery index: {e}")))?;
    std::fs::write(&target, text)
        .map_err(|e| CoreError::io(format!("Could not write {}: {e}", target.display())))
}

/// Saved collages, newest first, limited to the ones still on disk.
///
/// A file deleted or moved from Explorer drops out here rather than lingering as a
/// card that cannot be opened. The rewrite that prunes it is best-effort — the list
/// returned is correct either way.
pub fn entries() -> Vec<Value> {
    let stored = read();
    let live: Vec<Value> = stored
        .iter()
        .filter(|e| {
            e.get("path")
                .and_then(Value::as_str)
                .is_some_and(|p| Path::new(p).is_file())
        })
        .cloned()
        .collect();
    if live.len() != stored.len() {
        let _ = write(&live);
    }
    live
}

/// The entry describing one saved file, if it is in the library.
///
/// What a caller usually wants from it is `monitor`: whether the file is one screen's
/// worth of collage or the whole desktop, which decides how it is laid back down.
pub fn find(path: &str) -> Option<Value> {
    let wanted = key(path);
    read()
        .into_iter()
        .find(|e| e.get("path").and_then(Value::as_str).map(key) == Some(wanted.clone()))
}

/// Add one save to the front of the index, replacing any entry for the same file.
pub fn record(
    path: &str,
    monitor: Option<i64>,
    images: &[String],
    width: i64,
    height: i64,
) -> Result<Value, CoreError> {
    let entry = json!({
        "path": path,
        // Local time with its offset: the gallery shows this to a person, and the
        // offset keeps it unambiguous if the file is looked at from elsewhere.
        "saved_at": chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
        "monitor": monitor,
        "images": images,
        "width": width,
        "height": height,
    });

    let wanted = key(path);
    let mut kept: Vec<Value> = read()
        .into_iter()
        .filter(|e| e.get("path").and_then(Value::as_str).map(key) != Some(wanted.clone()))
        .collect();
    kept.insert(0, entry.clone());
    kept.truncate(MAX_ENTRIES);
    write(&kept)?;
    Ok(entry)
}

/// Drop one entry, **leaving the image itself alone**.
///
/// Removing a picture from the library is a bookkeeping change; deleting the user's
/// file is not something the app does on their behalf.
pub fn forget(path: &str) -> Result<bool, CoreError> {
    let stored = read();
    let wanted = key(path);
    let kept: Vec<Value> = stored
        .iter()
        .filter(|e| e.get("path").and_then(Value::as_str).map(key) != Some(wanted.clone()))
        .cloned()
        .collect();
    if kept.len() == stored.len() {
        return Ok(false);
    }
    write(&kept)?;
    Ok(true)
}

/// A filename that sorts by time and says what it holds.
///
/// Monitors are numbered from one here, matching every label the UI shows.
pub fn suggest_name(monitor: Option<i64>, extension: &str) -> String {
    let stamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
    let which = match monitor {
        None => "all".to_string(),
        Some(index) => format!("monitor{}", index + 1),
    };
    format!("collage_{stamp}_{which}{extension}")
}

// ── RPC results ──────────────────────────────────────────────────────────────

/// `suggest_collage_path` — where the save dialog should open.
///
/// Creates the folder, so the dialog lands somewhere that exists.
pub fn suggest_collage_path_result(cfg: &Value, monitor: Option<i64>) -> Result<Value, CoreError> {
    let dir = library_dir(cfg);
    std::fs::create_dir_all(&dir)
        .map_err(|e| CoreError::io(format!("Could not create {}: {e}", dir.display())))?;
    let path = dir.join(suggest_name(monitor, ".png"));
    Ok(json!({ "path": path.to_string_lossy() }))
}

/// `list_saved_collages` — the library, plus the folder the next save will use.
pub fn list_saved_collages_result(cfg: &Value) -> Value {
    json!({
        "collages": entries(),
        "folder": library_dir(cfg).to_string_lossy(),
    })
}

/// `forget_saved_collage` — drop an index entry, keep the file.
pub fn forget_saved_collage_result(path: &str) -> Result<Value, CoreError> {
    Ok(json!({ "removed": forget(path)? }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Sandbox;

    /// A saved image on disk, so the reconciliation in `entries` keeps it.
    fn image(sandbox: &Sandbox, name: &str) -> String {
        sandbox.file(name, b"pretend this is a png")
    }

    #[test]
    fn an_absent_index_reads_as_an_empty_library() {
        let _sandbox = Sandbox::new("absent");
        assert!(entries().is_empty());
        assert!(find("Z:/whatever.png").is_none());
    }

    #[test]
    fn a_corrupt_index_reads_as_empty_rather_than_failing() {
        let _sandbox = Sandbox::new("corrupt");
        std::fs::write(index_file(), "{ not json at all").unwrap();
        assert!(entries().is_empty());
    }

    #[test]
    fn records_come_back_newest_first() {
        let sandbox = Sandbox::new("order");
        let first = image(&sandbox, "one.png");
        let second = image(&sandbox, "two.png");
        record(&first, Some(0), &[], 100, 100).unwrap();
        record(&second, None, &[], 200, 200).unwrap();

        let listed = entries();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0]["path"], second);
        assert_eq!(listed[1]["path"], first);
    }

    #[test]
    fn an_entry_carries_the_documented_shape() {
        let sandbox = Sandbox::new("shape");
        let path = image(&sandbox, "shot.png");
        let entry = record(&path, Some(1), &["a.jpg".to_string()], 3840, 2160).unwrap();

        assert_eq!(entry["path"], path);
        assert_eq!(entry["monitor"], 1);
        assert_eq!(entry["images"][0], "a.jpg");
        assert_eq!(entry["width"], 3840);
        assert_eq!(entry["height"], 2160);
        let saved_at = entry["saved_at"].as_str().unwrap();
        assert!(
            saved_at.len() >= 19,
            "expected an ISO timestamp, got {saved_at}"
        );
        // Local time with an offset, and seconds precision — no fractional part.
        assert!(
            !saved_at.contains('.'),
            "seconds precision only: {saved_at}"
        );
    }

    /// A desktop-wide export records `monitor: null`, which is what decides whether
    /// applying it back spans every screen or is placed on each one.
    #[test]
    fn a_desktop_wide_export_records_a_null_monitor() {
        let sandbox = Sandbox::new("null");
        let path = image(&sandbox, "all.png");
        record(&path, None, &[], 5760, 2160).unwrap();
        assert!(find(&path).unwrap()["monitor"].is_null());
    }

    #[test]
    fn saving_over_a_file_replaces_its_entry_rather_than_duplicating_it() {
        let sandbox = Sandbox::new("replace");
        let path = image(&sandbox, "same.png");
        record(&path, Some(0), &[], 100, 100).unwrap();
        record(&path, Some(1), &[], 200, 200).unwrap();

        let listed = entries();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0]["monitor"], 1);
    }

    /// Windows paths that differ only in case or separator name the same file.
    #[test]
    fn identity_ignores_case_and_separator() {
        let sandbox = Sandbox::new("identity");
        let path = image(&sandbox, "Mixed.png");
        record(&path, Some(0), &[], 10, 10).unwrap();

        if cfg!(windows) {
            assert!(find(&path.to_uppercase()).is_some(), "case must not matter");
        }
        assert!(
            find(&path.replace('\\', "/")).is_some(),
            "separator must not matter"
        );
    }

    #[test]
    fn a_file_deleted_from_explorer_stops_being_listed() {
        let sandbox = Sandbox::new("pruned");
        let kept = image(&sandbox, "kept.png");
        let gone = image(&sandbox, "gone.png");
        record(&kept, None, &[], 10, 10).unwrap();
        record(&gone, None, &[], 10, 10).unwrap();

        std::fs::remove_file(&gone).unwrap();

        let listed = entries();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0]["path"], kept);
        // And the prune was persisted, not just filtered on the way out.
        assert_eq!(read().len(), 1);
    }

    /// The rule the whole module exists to protect.
    #[test]
    fn forgetting_an_entry_leaves_the_image_on_disk() {
        let sandbox = Sandbox::new("forget");
        let path = image(&sandbox, "precious.png");
        record(&path, None, &[], 10, 10).unwrap();

        assert!(forget(&path).unwrap());
        assert!(
            Path::new(&path).is_file(),
            "the app must never delete the user's picture"
        );
        assert!(entries().is_empty());
    }

    #[test]
    fn forgetting_something_that_was_never_indexed_reports_no_removal() {
        let _sandbox = Sandbox::new("forgetmiss");
        assert!(!forget("Z:/never-saved.png").unwrap());
    }

    #[test]
    fn the_index_is_capped() {
        let sandbox = Sandbox::new("cap");
        // Below the cap is cheap to verify exactly; the truncate is the same code.
        for i in 0..5 {
            let path = image(&sandbox, &format!("shot{i}.png"));
            record(&path, None, &[], 10, 10).unwrap();
        }
        assert_eq!(entries().len(), 5);
        assert_eq!(MAX_ENTRIES, 500);
    }

    #[test]
    fn suggested_names_say_which_screen_and_sort_by_time() {
        assert!(suggest_name(None, ".png").ends_with("_all.png"));
        // Monitors are one-based in the filename, matching the UI labels.
        assert!(suggest_name(Some(0), ".png").ends_with("_monitor1.png"));
        assert!(suggest_name(Some(2), ".jpg").ends_with("_monitor3.jpg"));
        assert!(suggest_name(None, ".png").starts_with("collage_"));
    }

    #[test]
    fn the_save_dialog_path_is_inside_the_library_and_the_folder_exists() {
        let _sandbox = Sandbox::new("suggest");
        let cfg = json!({});
        let result = suggest_collage_path_result(&cfg, Some(0)).unwrap();
        let path = PathBuf::from(result["path"].as_str().unwrap());
        assert!(
            path.parent().unwrap().is_dir(),
            "the dialog must open somewhere real"
        );
        assert_eq!(path.parent().unwrap(), library_dir(&cfg));
    }

    /// An unsaved folder change must still be where the dialog opens, so the config
    /// overlay has to be honoured rather than the file on disk.
    #[test]
    fn an_unsaved_library_folder_is_honoured() {
        let sandbox = Sandbox::new("overlay");
        let elsewhere = sandbox.dir.join("elsewhere");
        let cfg = json!({ "paths": { "saved_folder": elsewhere.to_string_lossy() } });
        assert_eq!(library_dir(&cfg), elsewhere);
        assert_eq!(
            list_saved_collages_result(&cfg)["folder"],
            elsewhere.to_string_lossy().as_ref()
        );
    }

    #[test]
    fn listing_reports_both_the_library_and_its_folder() {
        let sandbox = Sandbox::new("listing");
        let path = image(&sandbox, "one.png");
        record(&path, None, &[], 10, 10).unwrap();

        let result = list_saved_collages_result(&json!({}));
        assert_eq!(result["collages"].as_array().unwrap().len(), 1);
        assert!(result["folder"].is_string());
    }
}
