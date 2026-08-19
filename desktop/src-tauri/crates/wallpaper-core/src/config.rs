//! Where the user's files live, and reading and writing `settings.toml`.
//!
//! Ports `config.py`. Two things here are load-bearing beyond the obvious:
//!
//! **User files live outside the installation.** Everything used to sit inside the
//! install directory, which works in a checkout and fails the moment the app is under
//! `C:\Program Files` and the process is unprivileged. Configuration, rotation
//! history and transparency live in `%APPDATA%\WallpaperChanger`; composed wallpapers
//! and saved collages live in `%LOCALAPPDATA%\WallpaperChanger`, because they are
//! large and machine-specific and have no business in a roaming profile.
//!
//! **The writer preserves comments, unlike the Python one.** `save_config` in
//! `config.py` rebuilds the file from a dict, which drops every comment in the
//! shipped `settings.toml` on the first save, along with any top-level key that is
//! not a table. Using `toml_edit` keeps both. That is a deliberate behaviour change:
//! nothing depended on the file being normalised, and the comments explain settings
//! the UI does not surface.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::{Map, Value};
use toml_edit::{DocumentMut, Item};

use crate::CoreError;

pub const APP_NAME: &str = "WallpaperChanger";

/// Files that belong to the user rather than the installation, migrated out of the
/// old in-install `config/` directory on first run.
const USER_FILES: &[&str] = &["settings.toml", "state.json", "transparency.json"];

/// Serialises writes, as `config.py`'s `_SAVE_LOCK` does. An interrupted process must
/// never leave `settings.toml` half-written, and two threads must not interleave.
static SAVE_LOCK: Mutex<()> = Mutex::new(());

// ── locations ────────────────────────────────────────────────────────────────

/// The installation root, used only to find the legacy `config/` migration seed.
///
/// Installed, that is the directory holding the executable. In a checkout the
/// executable is somewhere under `target/`, so fall back to the compile-time crate
/// location — the same trick `engine.rs` uses to find the repo for `uv run`, and
/// guarded the same way by checking the directory actually exists.
pub fn project_root() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            if dir.join("config").is_dir() {
                return dir.to_path_buf();
            }
        }
    }
    // crates/wallpaper-core -> crates -> src-tauri -> desktop -> repo root
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .map(Path::to_path_buf);
    match checkout {
        Some(root) if root.join("config").is_dir() => root,
        _ => std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from(".")),
    }
}

fn dir_from_env(override_var: &str, base_var: &str, home_tail: &[&str]) -> PathBuf {
    if let Ok(path) = std::env::var(override_var) {
        if !path.is_empty() {
            return PathBuf::from(path);
        }
    }
    let root = match std::env::var(base_var) {
        Ok(base) if !base.is_empty() => PathBuf::from(base),
        _ => {
            let mut home = home_dir();
            for part in home_tail {
                home.push(part);
            }
            home
        }
    };
    root.join(APP_NAME)
}

fn home_dir() -> PathBuf {
    std::env::var("USERPROFILE")
        .ok()
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// `%APPDATA%\WallpaperChanger` — settings, rotation state, transparency, gallery.
///
/// `WALLPAPER_CHANGER_CONFIG_DIR` overrides it, which is how the test suites keep
/// off real user files.
pub fn user_config_dir() -> PathBuf {
    dir_from_env(
        "WALLPAPER_CHANGER_CONFIG_DIR",
        "APPDATA",
        &["AppData", "Roaming"],
    )
}

/// `%LOCALAPPDATA%\WallpaperChanger` — composed wallpapers and saved collages.
pub fn user_data_dir() -> PathBuf {
    dir_from_env(
        "WALLPAPER_CHANGER_DATA_DIR",
        "LOCALAPPDATA",
        &["AppData", "Local"],
    )
}

pub fn default_config_path() -> PathBuf {
    user_config_dir().join("settings.toml")
}

pub fn state_file() -> PathBuf {
    user_config_dir().join("state.json")
}

pub fn transparency_file() -> PathBuf {
    user_config_dir().join("transparency.json")
}

fn resolve_under_data(raw: Option<&str>, fallback: &str) -> PathBuf {
    // Empty means the default, not an empty path: the field exists to be changed
    // from the UI, and clearing it by accident must not write to the data root.
    let raw = raw.filter(|s| !s.is_empty()).unwrap_or(fallback);
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        path
    } else {
        user_data_dir().join(path)
    }
}

/// `paths.output_folder`, where the composed BMP is written.
pub fn resolve_output_dir(cfg: &Value) -> PathBuf {
    resolve_under_data(cfg.pointer("/paths/output_folder").and_then(Value::as_str), "output")
}

/// `paths.saved_folder`, where "Save as image" puts collages the user keeps.
pub fn resolve_saved_dir(cfg: &Value) -> PathBuf {
    resolve_under_data(cfg.pointer("/paths/saved_folder").and_then(Value::as_str), "saved")
}

/// Resolve a possibly-relative path against `root` (default: the project root).
///
/// Python calls `.resolve()`, which also normalises `.` and `..`. This does the same
/// lexically rather than by touching the filesystem, so a folder the user has not
/// created yet still resolves.
pub fn resolve_path(raw: &str, root: Option<&Path>) -> PathBuf {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        return normalise(&path);
    }
    let base = root.map(Path::to_path_buf).unwrap_or_else(project_root);
    normalise(&base.join(path))
}

/// Collapse `.` and `..` without consulting the filesystem.
fn normalise(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

// ── migration ────────────────────────────────────────────────────────────────

/// Copy the user's files out of the old in-install `config/` directory, once.
///
/// Best-effort and always reversible: never overwrites what is already in
/// `%APPDATA%`, and never removes the original.
pub fn migrate_legacy_files() {
    let legacy = project_root().join("config");
    if !legacy.is_dir() {
        return;
    }
    let target_dir = user_config_dir();
    for name in USER_FILES {
        let source = legacy.join(name);
        let target = target_dir.join(name);
        if !source.is_file() || target.exists() {
            continue;
        }
        if std::fs::create_dir_all(&target_dir).is_err() {
            return;
        }
        // A failed copy just means the defaults are used; it must not be fatal.
        let _ = std::fs::copy(&source, &target);
    }
}

// ── TOML <-> JSON ────────────────────────────────────────────────────────────

fn toml_item_to_json(item: &Item) -> Option<Value> {
    match item {
        Item::None => None,
        Item::Value(value) => Some(toml_value_to_json(value)),
        Item::Table(table) => {
            let mut map = Map::new();
            for (key, child) in table.iter() {
                if let Some(value) = toml_item_to_json(child) {
                    map.insert(key.to_string(), value);
                }
            }
            Some(Value::Object(map))
        }
        Item::ArrayOfTables(tables) => Some(Value::Array(
            tables
                .iter()
                .map(|t| toml_item_to_json(&Item::Table(t.clone())).unwrap_or(Value::Null))
                .collect(),
        )),
    }
}

fn toml_value_to_json(value: &toml_edit::Value) -> Value {
    use toml_edit::Value as V;
    match value {
        V::String(s) => Value::String(s.value().clone()),
        V::Integer(i) => Value::Number((*i.value()).into()),
        V::Float(f) => serde_json::Number::from_f64(*f.value())
            .map(Value::Number)
            .unwrap_or(Value::Null),
        V::Boolean(b) => Value::Bool(*b.value()),
        // TOML dates have no JSON counterpart; the protocol is JSON, so they cross
        // as strings. Nothing in settings.toml uses one.
        V::Datetime(d) => Value::String(d.value().to_string()),
        V::Array(array) => Value::Array(array.iter().map(toml_value_to_json).collect()),
        V::InlineTable(table) => Value::Object(
            table
                .iter()
                .map(|(k, v)| (k.to_string(), toml_value_to_json(v)))
                .collect(),
        ),
    }
}

fn json_to_toml_value(value: &Value) -> Option<toml_edit::Value> {
    use toml_edit::Value as V;
    Some(match value {
        Value::Null => return None, // TOML has no null; the key is dropped instead
        Value::Bool(b) => V::from(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                V::from(i)
            } else {
                V::from(n.as_f64()?)
            }
        }
        Value::String(s) => V::from(s.as_str()),
        Value::Array(items) => {
            let mut array = toml_edit::Array::new();
            for item in items {
                array.push(json_to_toml_value(item)?);
            }
            V::Array(array)
        }
        Value::Object(fields) => {
            let mut table = toml_edit::InlineTable::new();
            for (key, child) in fields {
                if let Some(v) = json_to_toml_value(child) {
                    table.insert(key, v);
                }
            }
            V::InlineTable(table)
        }
    })
}

// ── load / save ──────────────────────────────────────────────────────────────

/// Read `settings.toml`, injecting `_config_path` the way `load_config` does.
///
/// Passing `None` migrates the legacy files first, so a first run after upgrading
/// finds the settings that were in the install directory before deciding the file is
/// missing.
pub fn load_config(path: Option<&Path>) -> Result<Value, CoreError> {
    let target = match path {
        Some(p) => p.to_path_buf(),
        None => {
            migrate_legacy_files();
            default_config_path()
        }
    };
    if !target.exists() {
        return Err(CoreError::not_found(format!(
            "Configuration file not found: {}",
            target.display()
        )));
    }
    let text = std::fs::read_to_string(&target)
        .map_err(|e| CoreError::io(format!("Could not read {}: {e}", target.display())))?;
    let document: DocumentMut = text
        .parse()
        .map_err(|e| CoreError::invalid(format!("{} is not valid TOML: {e}", target.display())))?;

    let mut config = match toml_item_to_json(document.as_item()) {
        Some(Value::Object(map)) => map,
        _ => Map::new(),
    };
    config.insert(
        "_config_path".to_string(),
        Value::String(target.to_string_lossy().into_owned()),
    );
    Ok(Value::Object(config))
}

/// Write `cfg` back to disk, keeping the file's comments and layout.
///
/// Applies the configuration onto the existing document rather than regenerating it:
/// keys are added or updated, keys the caller dropped are removed from their table,
/// and everything else — comments, ordering, tables the app does not model — is left
/// exactly as it was.
///
/// Keys beginning with `_` are internal (`_config_path`) and are never written.
pub fn save_config(cfg: &Value, path: Option<&Path>) -> Result<(), CoreError> {
    let target = match path {
        Some(p) => p.to_path_buf(),
        None => cfg
            .get("_config_path")
            .and_then(Value::as_str)
            .map(PathBuf::from)
            .unwrap_or_else(default_config_path),
    };

    let existing = std::fs::read_to_string(&target).unwrap_or_default();
    let mut document: DocumentMut = existing.parse().unwrap_or_default();

    let Some(sections) = cfg.as_object() else {
        return Err(CoreError::invalid("Configuration must be an object."));
    };

    for (section, values) in sections {
        if section.starts_with('_') {
            continue;
        }
        let Some(fields) = values.as_object() else {
            // `save_config` in config.py skips non-table top-level entries entirely.
            // Skipping them here too leaves whatever the file already had in place.
            continue;
        };

        if !document.contains_table(section) {
            document[section] = Item::Table(toml_edit::Table::new());
        }
        let Some(table) = document[section].as_table_mut() else {
            continue;
        };

        for (key, value) in fields {
            match json_to_toml_value(value) {
                Some(v) => table[key] = Item::Value(v),
                None => {
                    table.remove(key);
                }
            }
        }

        // Drop keys the caller no longer carries, so a removed setting does not
        // linger — the Python writer rebuilt the section from the dict.
        let stale: Vec<String> = table
            .iter()
            .map(|(k, _)| k.to_string())
            .filter(|k| !fields.contains_key(k))
            .collect();
        for key in stale {
            table.remove(&key);
        }
    }

    write_atomically(&target, document.to_string().as_bytes())
}

/// Write via a temporary file in the same directory, then rename.
///
/// `os.replace` on Windows is atomic within a volume, and so is `fs::rename` — which
/// is why the temporary must be a sibling of the target rather than in `%TEMP%`.
fn write_atomically(target: &Path, bytes: &[u8]) -> Result<(), CoreError> {
    use std::io::Write;

    let parent = target.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)
        .map_err(|e| CoreError::io(format!("Could not create {}: {e}", parent.display())))?;

    let _guard = SAVE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let name = target.file_name().map(|n| n.to_string_lossy().into_owned());
    let temp = parent.join(format!(
        ".{}.{}.tmp",
        name.as_deref().unwrap_or("settings.toml"),
        std::process::id()
    ));

    let result = (|| -> std::io::Result<()> {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_all()?; // the fsync before the rename, as config.py does
        drop(file);
        std::fs::rename(&temp, target)
    })();

    if let Err(e) = result {
        let _ = std::fs::remove_file(&temp);
        return Err(CoreError::io(format!(
            "Could not write {}: {e}",
            target.display()
        )));
    }
    Ok(())
}

/// The `get_config` RPC result: the configuration without internal keys, plus the
/// path it was read from.
pub fn get_config_result(cfg: &Value) -> Value {
    let public: Map<String, Value> = cfg
        .as_object()
        .map(|map| {
            map.iter()
                .filter(|(k, _)| !k.starts_with('_'))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        })
        .unwrap_or_default();
    let path = cfg
        .get("_config_path")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| default_config_path().to_string_lossy().into_owned());
    serde_json::json!({ "config": Value::Object(public), "config_path": path })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    use crate::testing::Sandbox;

    const SAMPLE: &str = r#"# Top comment that must survive a save.
[general]
mode = "collage"
interval = 300
# Why start_minimized exists.
start_minimized = false

[paths]
wallpapers_folder = "C:\\Pictures"
output_folder = "output"
"#;

    #[test]
    fn overrides_win_over_appdata() {
        let sandbox = Sandbox::new("override");
        assert_eq!(user_config_dir(), sandbox.dir.join("cfg"));
        assert_eq!(user_data_dir(), sandbox.dir.join("data"));
    }

    #[test]
    fn config_and_data_directories_are_separate() {
        let _sandbox = Sandbox::new("split");
        assert_ne!(user_config_dir(), user_data_dir());
        assert_eq!(default_config_path().file_name().unwrap(), "settings.toml");
        assert_eq!(state_file().parent(), Some(user_config_dir()).as_deref());
        assert_eq!(transparency_file().file_name().unwrap(), "transparency.json");
    }

    /// A relative folder resolves under the *data* directory, never the install
    /// directory — resolving from the install is exactly what breaks once the app
    /// lives under Program Files.
    #[test]
    fn relative_output_resolves_under_the_data_directory() {
        let _sandbox = Sandbox::new("relout");
        let cfg = json!({ "paths": { "output_folder": "output" } });
        assert_eq!(resolve_output_dir(&cfg), user_data_dir().join("output"));
    }

    #[test]
    fn absolute_output_is_taken_as_written() {
        let _sandbox = Sandbox::new("absout");
        let cfg = json!({ "paths": { "output_folder": "D:\\Shots" } });
        assert_eq!(resolve_output_dir(&cfg), PathBuf::from("D:\\Shots"));
    }

    /// An accidentally cleared field must fall back to the default subfolder rather
    /// than writing into the root of the data directory.
    #[test]
    fn an_empty_folder_setting_falls_back_to_the_default() {
        let _sandbox = Sandbox::new("empty");
        let cfg = json!({ "paths": { "saved_folder": "", "output_folder": "" } });
        assert_eq!(resolve_saved_dir(&cfg), user_data_dir().join("saved"));
        assert_eq!(resolve_output_dir(&cfg), user_data_dir().join("output"));
        // A missing key behaves the same as an empty one.
        assert_eq!(resolve_saved_dir(&json!({})), user_data_dir().join("saved"));
    }

    #[test]
    fn resolve_path_normalises_without_touching_the_disk() {
        let base = Path::new("C:\\base");
        assert_eq!(resolve_path("sub\\..\\other", Some(base)), PathBuf::from("C:\\base\\other"));
        assert_eq!(resolve_path(".\\here", Some(base)), PathBuf::from("C:\\base\\here"));
        assert_eq!(resolve_path("D:\\abs", Some(base)), PathBuf::from("D:\\abs"));
    }

    #[test]
    fn loading_a_missing_file_is_not_found() {
        let sandbox = Sandbox::new("missing");
        let err = load_config(Some(&sandbox.dir.join("nope.toml"))).unwrap_err();
        assert_eq!(err.kind(), crate::ErrorKind::NotFound);
    }

    #[test]
    fn load_exposes_the_path_it_read() {
        let sandbox = Sandbox::new("loadpath");
        let file = sandbox.dir.join("settings.toml");
        std::fs::write(&file, SAMPLE).unwrap();
        let cfg = load_config(Some(&file)).unwrap();
        assert_eq!(cfg["_config_path"], file.to_string_lossy().as_ref());
        assert_eq!(cfg["general"]["mode"], "collage");
        assert_eq!(cfg["general"]["interval"], 300);
        assert_eq!(cfg["general"]["start_minimized"], false);
        assert_eq!(cfg["paths"]["wallpapers_folder"], "C:\\Pictures");
    }

    /// The whole reason for `toml_edit`. The Python writer rebuilds the file from a
    /// dict and drops every comment on the first save.
    #[test]
    fn saving_preserves_comments() {
        let sandbox = Sandbox::new("comments");
        let file = sandbox.dir.join("settings.toml");
        std::fs::write(&file, SAMPLE).unwrap();

        let mut cfg = load_config(Some(&file)).unwrap();
        cfg["general"]["interval"] = json!(600);
        save_config(&cfg, None).unwrap();

        let written = std::fs::read_to_string(&file).unwrap();
        assert!(written.contains("# Top comment that must survive a save."));
        assert!(written.contains("# Why start_minimized exists."));
        assert!(written.contains("interval = 600"));
    }

    #[test]
    fn the_internal_path_key_is_never_written() {
        let sandbox = Sandbox::new("internal");
        let file = sandbox.dir.join("settings.toml");
        std::fs::write(&file, SAMPLE).unwrap();
        let cfg = load_config(Some(&file)).unwrap();
        save_config(&cfg, None).unwrap();
        let written = std::fs::read_to_string(&file).unwrap();
        assert!(!written.contains("_config_path"), "got:\n{written}");
    }

    /// Load, save, load again — the second load must equal the first. A writer that
    /// mangles a type or drops a key shows up here.
    #[test]
    fn a_save_round_trip_reaches_a_fixpoint() {
        let sandbox = Sandbox::new("fixpoint");
        let file = sandbox.dir.join("settings.toml");
        std::fs::write(&file, SAMPLE).unwrap();

        let first = load_config(Some(&file)).unwrap();
        save_config(&first, None).unwrap();
        let second = load_config(Some(&file)).unwrap();
        assert_eq!(first, second);

        save_config(&second, None).unwrap();
        let third = load_config(Some(&file)).unwrap();
        assert_eq!(second, third);
    }

    #[test]
    fn a_key_dropped_by_the_caller_is_removed_from_the_file() {
        let sandbox = Sandbox::new("drop");
        let file = sandbox.dir.join("settings.toml");
        std::fs::write(&file, SAMPLE).unwrap();

        let mut cfg = load_config(Some(&file)).unwrap();
        cfg["general"].as_object_mut().unwrap().remove("interval");
        save_config(&cfg, None).unwrap();

        let written = std::fs::read_to_string(&file).unwrap();
        assert!(!written.contains("interval"), "got:\n{written}");
        assert!(written.contains("mode = \"collage\""));
    }

    #[test]
    fn a_new_section_can_be_added() {
        let sandbox = Sandbox::new("newsection");
        let file = sandbox.dir.join("settings.toml");
        std::fs::write(&file, SAMPLE).unwrap();

        let mut cfg = load_config(Some(&file)).unwrap();
        cfg["video"] = json!({ "enabled": true, "folder": "D:\\Clips" });
        save_config(&cfg, None).unwrap();

        let reloaded = load_config(Some(&file)).unwrap();
        assert_eq!(reloaded["video"]["enabled"], true);
        assert_eq!(reloaded["video"]["folder"], "D:\\Clips");
    }

    /// Backslashes are the common case on Windows and must survive a round trip
    /// without doubling or collapsing.
    #[test]
    fn windows_paths_survive_a_round_trip() {
        let sandbox = Sandbox::new("backslash");
        let file = sandbox.dir.join("settings.toml");
        std::fs::write(&file, SAMPLE).unwrap();

        let mut cfg = load_config(Some(&file)).unwrap();
        cfg["paths"]["wallpapers_folder"] = json!("C:\\Users\\me\\My Pictures\\Wall");
        save_config(&cfg, None).unwrap();

        let reloaded = load_config(Some(&file)).unwrap();
        assert_eq!(reloaded["paths"]["wallpapers_folder"], "C:\\Users\\me\\My Pictures\\Wall");
    }

    #[test]
    fn saving_into_a_directory_that_does_not_exist_yet_creates_it() {
        let sandbox = Sandbox::new("mkdir");
        let file = sandbox.dir.join("deep").join("nested").join("settings.toml");
        let cfg = json!({ "general": { "mode": "collage" } });
        save_config(&cfg, Some(&file)).unwrap();
        assert!(file.is_file());
        assert_eq!(load_config(Some(&file)).unwrap()["general"]["mode"], "collage");
    }

    #[test]
    fn no_temporary_file_is_left_behind() {
        let sandbox = Sandbox::new("notemp");
        let file = sandbox.dir.join("settings.toml");
        std::fs::write(&file, SAMPLE).unwrap();
        save_config(&load_config(Some(&file)).unwrap(), None).unwrap();

        let leftovers: Vec<_> = std::fs::read_dir(&sandbox.dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "left behind {leftovers:?}");
    }

    /// The real shipped file, embedded so this holds wherever the tests run.
    const SHIPPED: &str = include_str!("../../../../../config/settings.toml");

    /// The file the installer seeds is the one that matters: it carries eleven
    /// explanatory comments the UI never surfaces, and the Python writer destroys
    /// every one of them on the first save.
    #[test]
    fn the_shipped_settings_file_survives_a_save_intact() {
        let sandbox = Sandbox::new("shipped");
        let file = sandbox.dir.join("settings.toml");
        std::fs::write(&file, SHIPPED).unwrap();

        let before = load_config(Some(&file)).unwrap();
        save_config(&before, None).unwrap();
        let after = load_config(Some(&file)).unwrap();

        assert_eq!(before, after, "a value changed across a save");

        let written = std::fs::read_to_string(&file).unwrap();
        let comments_before = SHIPPED.lines().filter(|l| l.trim_start().starts_with('#')).count();
        let comments_after = written.lines().filter(|l| l.trim_start().starts_with('#')).count();
        assert!(comments_before >= 10, "fixture should have comments to lose");
        assert_eq!(
            comments_before, comments_after,
            "comments were dropped; that is the whole reason for toml_edit"
        );

        // Spot-check the settings the UI depends on most.
        assert_eq!(after["general"]["mode"], "collage");
        assert_eq!(after["display"]["fit_mode"], "fill");
        assert!(after["hotkeys"].is_object());
        assert!(after["video"].is_object());
    }

    #[test]
    fn get_config_result_hides_internal_keys() {
        let cfg = json!({
            "_config_path": "C:\\cfg\\settings.toml",
            "general": { "mode": "collage" },
        });
        let result = get_config_result(&cfg);
        assert_eq!(result["config_path"], "C:\\cfg\\settings.toml");
        assert_eq!(result["config"]["general"]["mode"], "collage");
        assert!(result["config"].get("_config_path").is_none());
    }

    /// Migration copies and never overwrites, so an existing user file wins.
    #[test]
    fn migration_never_overwrites_an_existing_file() {
        let sandbox = Sandbox::new("migrate");
        let target_dir = user_config_dir();
        std::fs::create_dir_all(&target_dir).unwrap();
        std::fs::write(target_dir.join("settings.toml"), "[general]\nmode = \"mine\"\n").unwrap();

        migrate_legacy_files();

        let kept = std::fs::read_to_string(target_dir.join("settings.toml")).unwrap();
        assert!(kept.contains("mine"), "migration clobbered the user's file");
        let _ = sandbox;
    }
}
