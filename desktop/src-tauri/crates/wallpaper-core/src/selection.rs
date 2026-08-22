//! Choosing which pictures go into the next collage, and remembering what was shown.
//!
//! Ports `image_utils.pick_images` and its JSON state. Two methods:
//!
//! - **sequential** — newest file first by modification time, resuming from a stored
//!   cursor per folder.
//! - **random** — no repeats until the folder has been exhausted, tracked as a list
//!   of filenames per folder.
//!
//! ## The state key is a compatibility surface
//!
//! `state.json` is keyed by the folder's resolved path, and for random selection by
//! that path plus `":random_history"`. Python builds it with `str(Path(folder)
//! .resolve())`, which on Windows returns `C:\Users\me\Pictures`.
//!
//! Rust's `std::fs::canonicalize` returns the extended-length form,
//! `\\?\C:\Users\me\Pictures`. Using it would produce a key that matches nothing,
//! silently resetting every user's rotation history — they would start seeing
//! repeats with no error anywhere. [`dunce::canonicalize`] strips that prefix, and
//! `state_key_matches_python` pins the result against a literal.
//!
//! `.resolve()` also corrects a path's casing to what is on disk, which
//! `dunce::canonicalize` does too. Both fall back to the path as given when the
//! folder does not exist.

use std::path::{Path, PathBuf};

use rand::seq::SliceRandom;
use serde_json::{Map, Value};

use crate::{config, images, CoreError};

/// The identity of a folder in `state.json`.
///
/// Canonicalised when the folder exists, so two spellings of one path share a
/// history; left as given when it does not, matching `Path.resolve()`'s
/// non-strict behaviour.
pub fn folder_key(folder: &Path) -> String {
    dunce::canonicalize(folder)
        .unwrap_or_else(|_| folder.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

fn history_key(folder: &Path) -> String {
    format!("{}:random_history", folder_key(folder))
}

fn load_state(path: &Path) -> Map<String, Value> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
}

fn save_state(path: &Path, state: &Map<String, Value>) -> Result<(), CoreError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| CoreError::io(format!("Could not create {}: {e}", parent.display())))?;
    }
    let text = serde_json::to_string_pretty(state)
        .map_err(|e| CoreError::io(format!("Could not serialise the selection state: {e}")))?;
    std::fs::write(path, text)
        .map_err(|e| CoreError::io(format!("Could not write {}: {e}", path.display())))
}

/// Images newest-first by modification time.
fn by_date_desc(folder: &Path) -> Vec<PathBuf> {
    let mut found = images::list_images(folder);
    found.sort_by_key(|p| {
        std::fs::metadata(p)
            .and_then(|m| m.modified())
            .ok()
            .map(std::cmp::Reverse)
    });
    found
}

/// Pick `count` images from `folder`.
///
/// `state_file` defaults to the real `state.json`; the preview and the save dialog
/// pass a throwaway path so browsing does not consume the rotation.
pub fn pick_images(
    folder: &Path,
    count: usize,
    method: &str,
    state_file: Option<&Path>,
) -> Result<Vec<PathBuf>, CoreError> {
    let state_path = state_file
        .map(Path::to_path_buf)
        .unwrap_or_else(config::state_file);

    if method == "sequential" {
        return pick_sequential(folder, count, &state_path);
    }
    pick_random(folder, count, &state_path)
}

fn pick_sequential(
    folder: &Path,
    count: usize,
    state_path: &Path,
) -> Result<Vec<PathBuf>, CoreError> {
    let available = by_date_desc(folder);
    if available.is_empty() {
        return Err(CoreError::not_found(format!(
            "No images in: {}",
            folder.display()
        )));
    }

    let mut state = load_state(state_path);
    let key = folder_key(folder);
    let cursor = state.get(&key).and_then(Value::as_u64).unwrap_or(0) as usize;

    // Walk on from where we stopped, wrapping around the folder.
    let picked: Vec<PathBuf> = (0..count)
        .map(|i| available[(cursor + i) % available.len()].clone())
        .collect();

    state.insert(
        key,
        Value::from(((cursor + count) % available.len()) as u64),
    );
    save_state(state_path, &state)?;
    Ok(picked)
}

fn pick_random(folder: &Path, count: usize, state_path: &Path) -> Result<Vec<PathBuf>, CoreError> {
    let available = images::list_images(folder);
    if available.is_empty() {
        return Err(CoreError::not_found(format!(
            "No images in: {}",
            folder.display()
        )));
    }

    let mut state = load_state(state_path);
    let key = history_key(folder);

    // The history stores *filenames*, not paths. Changing that silently resets every
    // user's cycle, so it is copied exactly.
    let mut shown: Vec<String> = state
        .get(&key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    let name_of = |p: &PathBuf| {
        p.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    };

    let mut unseen: Vec<PathBuf> = available
        .iter()
        .filter(|p| !shown.contains(&name_of(p)))
        .cloned()
        .collect();

    // Not enough left to fill the collage: the cycle is over, start again.
    if unseen.len() < count {
        shown.clear();
        unseen = available.clone();
    }

    let mut rng = rand::thread_rng();
    let picked: Vec<PathBuf> = if count >= unseen.len() {
        // Take everything left, then top up with repeats rather than failing.
        let mut picked = unseen.clone();
        while picked.len() < count {
            picked.push(
                available
                    .choose(&mut rng)
                    .cloned()
                    .unwrap_or_else(|| available[0].clone()),
            );
        }
        picked
    } else {
        unseen.choose_multiple(&mut rng, count).cloned().collect()
    };

    shown.extend(picked.iter().map(name_of));
    state.insert(key, Value::from(shown));
    save_state(state_path, &state)?;
    Ok(picked)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Sandbox;

    fn folder_with(sandbox: &Sandbox, names: &[&str]) -> PathBuf {
        let dir = sandbox.dir.join("pics");
        std::fs::create_dir_all(&dir).unwrap();
        for name in names {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        dir
    }

    /// The trap this module exists to avoid. `std::fs::canonicalize` would return
    /// `\\?\C:\...` and match nothing in an existing `state.json`.
    #[test]
    fn the_state_key_has_no_extended_length_prefix() {
        let sandbox = Sandbox::new("statekey");
        let dir = folder_with(&sandbox, &["a.png"]);
        let key = folder_key(&dir);

        assert!(
            !key.starts_with(r"\\?\"),
            "the key must match Python's Path.resolve(): got {key}"
        );
        assert!(key.contains("pics"), "got {key}");
        // The history key is the folder key plus a fixed suffix, byte for byte.
        assert_eq!(history_key(&dir), format!("{key}:random_history"));
    }

    /// A folder that does not exist still produces a key rather than failing, which
    /// is what `Path.resolve()` does without `strict=True`.
    #[test]
    fn a_missing_folder_still_produces_a_key() {
        let key = folder_key(Path::new(r"Z:\not\here"));
        assert!(key.contains("not"), "got {key}");
    }

    #[test]
    fn an_empty_folder_is_not_found() {
        let sandbox = Sandbox::new("empty");
        let dir = folder_with(&sandbox, &[]);
        let state = sandbox.dir.join("state.json");
        let err = pick_images(&dir, 2, "random", Some(&state)).unwrap_err();
        assert_eq!(err.kind(), crate::ErrorKind::NotFound);
    }

    #[test]
    fn sequential_resumes_where_it_stopped() {
        let sandbox = Sandbox::new("seq");
        let dir = folder_with(&sandbox, &["a.png", "b.png", "c.png", "d.png"]);
        let state = sandbox.dir.join("state.json");

        let first = pick_images(&dir, 2, "sequential", Some(&state)).unwrap();
        let second = pick_images(&dir, 2, "sequential", Some(&state)).unwrap();

        assert_eq!(first.len(), 2);
        assert_eq!(second.len(), 2);
        // Four distinct files across two calls of two.
        let mut all: Vec<_> = first.iter().chain(second.iter()).collect();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), 4, "the cursor did not advance");
    }

    #[test]
    fn sequential_wraps_around_a_short_folder() {
        let sandbox = Sandbox::new("seqwrap");
        let dir = folder_with(&sandbox, &["only.png"]);
        let state = sandbox.dir.join("state.json");
        let picked = pick_images(&dir, 3, "sequential", Some(&state)).unwrap();
        assert_eq!(picked.len(), 3, "a short folder must repeat, not fail");
    }

    /// The point of the random mode: everything is shown once before anything
    /// repeats.
    #[test]
    fn random_shows_everything_before_repeating() {
        let sandbox = Sandbox::new("rand");
        let names: Vec<String> = (0..8).map(|i| format!("{i}.png")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let dir = folder_with(&sandbox, &refs);
        let state = sandbox.dir.join("state.json");

        let mut seen = Vec::new();
        for _ in 0..4 {
            seen.extend(pick_images(&dir, 2, "random", Some(&state)).unwrap());
        }
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), 8, "a picture repeated before the cycle ended");
    }

    #[test]
    fn random_starts_a_new_cycle_once_the_folder_is_exhausted() {
        let sandbox = Sandbox::new("randcycle");
        let dir = folder_with(&sandbox, &["a.png", "b.png", "c.png", "d.png"]);
        let state = sandbox.dir.join("state.json");

        for _ in 0..3 {
            let picked = pick_images(&dir, 2, "random", Some(&state)).unwrap();
            assert_eq!(
                picked.len(),
                2,
                "the cycle must restart rather than run dry"
            );
        }
    }

    /// The history is a list of bare filenames under `<folder>:random_history`.
    /// Anything else and an existing `state.json` stops being understood.
    #[test]
    fn the_history_stores_filenames_under_the_documented_key() {
        let sandbox = Sandbox::new("randshape");
        let dir = folder_with(&sandbox, &["one.png", "two.png", "three.png", "four.png"]);
        let state_path = sandbox.dir.join("state.json");

        pick_images(&dir, 2, "random", Some(&state_path)).unwrap();

        let state = load_state(&state_path);
        let history = state[&history_key(&dir)]
            .as_array()
            .expect("a history array");
        assert_eq!(history.len(), 2);
        for entry in history {
            let name = entry.as_str().unwrap();
            assert!(
                !name.contains('\\') && !name.contains('/'),
                "expected a bare name, got {name}"
            );
            assert!(name.ends_with(".png"));
        }
    }

    /// A throwaway state file is how the preview browses without consuming the
    /// rotation the desktop is following.
    #[test]
    fn a_throwaway_state_file_leaves_the_real_one_alone() {
        let sandbox = Sandbox::new("throwaway");
        let dir = folder_with(&sandbox, &["a.png", "b.png", "c.png", "d.png"]);
        let real = sandbox.dir.join("state.json");
        let scratch = sandbox.dir.join("preview.json");

        pick_images(&dir, 2, "random", Some(&real)).unwrap();
        let before = std::fs::read_to_string(&real).unwrap();

        pick_images(&dir, 2, "random", Some(&scratch)).unwrap();
        assert_eq!(std::fs::read_to_string(&real).unwrap(), before);
    }

    #[test]
    fn a_corrupt_state_file_is_treated_as_empty() {
        let sandbox = Sandbox::new("corruptstate");
        let dir = folder_with(&sandbox, &["a.png", "b.png"]);
        let state = sandbox.dir.join("state.json");
        std::fs::write(&state, "not json").unwrap();
        assert_eq!(
            pick_images(&dir, 2, "random", Some(&state)).unwrap().len(),
            2
        );
    }
}
