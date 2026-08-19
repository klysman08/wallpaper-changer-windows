//! The native engine core.
//!
//! This crate is taking over from the Python sidecar (`wallpaper_changer.rpc`) one
//! state-ownership unit at a time. [`Core::dispatch`] answers the methods that have
//! been ported and returns [`Dispatch::NotPorted`] for the rest, which the shell
//! forwards to the sidecar. The application therefore builds and runs at every commit
//! of the migration, and the webview cannot tell which side answered.
//!
//! Two rules hold the seam together:
//!
//! 1. **`NotPorted` is not an error.** If "not ported" were encoded as an `Err`, a
//!    genuine failure here would silently re-run in Python — and for `apply_wallpaper`
//!    that means composing and setting the wallpaper twice.
//! 2. **Events keep the sidecar's envelope.** [`EventSink`] implementations emit
//!    `{"event": name, "data": data}`, byte-compatible with what `dispatch_line`
//!    forwards today.
//!
//! When the sidecar is finally deleted, the fallthrough arm in the shell becomes the
//! allowlist gate: anything still `NotPorted` is by definition an unknown method.

use std::panic::{AssertUnwindSafe, UnwindSafe};
use std::sync::Arc;

use serde_json::{json, Value};

mod error;
pub mod collage;
pub mod compose;
pub mod config;
pub mod effects;
pub mod gallery;
pub mod i18n;
pub mod images;
pub mod monitor;
pub mod selection;
pub mod startup;

pub use error::{CoreError, ErrorKind};
pub use monitor::{get_monitors, virtual_desktop, Monitor};

/// Every method the engine answers, in the order `Engine._METHODS` lists them.
///
/// The Python allowlist is still the live gate during the migration; this is the
/// checklist the port works through, and the set the conformance corpus asserts
/// against. It becomes the gate itself once the sidecar is gone.
pub const METHODS: &[&str] = &[
    "ping",
    "get_capabilities",
    "get_config",
    "save_config",
    "get_monitors",
    "get_translations",
    "list_folder_images",
    "get_thumbnails",
    "get_image_preview",
    "suggest_collage_path",
    "save_collage",
    "list_saved_collages",
    "apply_saved_collage",
    "forget_saved_collage",
    "apply_wallpaper",
    "apply_default_wallpaper",
    "apply_previous_wallpaper",
    "set_effect",
    "preview",
    "watch_start",
    "watch_stop",
    "watch_status",
    "watch_toggle",
    "list_windows",
    "set_window_opacity",
    "get_foreground_window",
    "toggle_foreground_opacity",
    "get_opacity_settings",
    "save_opacity_settings",
    "reapply_opacity_settings",
    "scroll_transparency_status",
    "sync_scroll_transparency",
    "scan_videos",
    "video_start",
    "video_stop",
    "video_next",
    "video_prev",
    "video_set_sound",
    "video_toggle",
    "video_toggle_sound",
    "video_status",
    "get_startup_enabled",
    "set_startup_enabled",
    "notify",
    "shutdown",
];

/// The protocol version reported by `ping` and `get_capabilities`.
pub const PROTOCOL_VERSION: u64 = 1;

/// What [`Core::dispatch`] decided about a request.
#[derive(Debug)]
pub enum Dispatch {
    /// The core owns this method and has answered it.
    Handled(Result<Value, CoreError>),
    /// Not ported yet. The caller must forward it to the Python sidecar.
    ///
    /// Deliberately a distinct variant rather than an error value — see the module
    /// documentation for why conflating them is dangerous.
    NotPorted,
}

impl Dispatch {
    /// Convenience for handlers that succeeded.
    pub fn ok(value: Value) -> Self {
        Dispatch::Handled(Ok(value))
    }

    /// Convenience for handlers that failed.
    pub fn err(error: CoreError) -> Self {
        Dispatch::Handled(Err(error))
    }

    pub fn is_not_ported(&self) -> bool {
        matches!(self, Dispatch::NotPorted)
    }
}

/// Where unsolicited engine events go.
///
/// The core raises events without knowing what is listening, so it stays free of any
/// dependency on Tauri. The shell implements this by emitting on the webview's
/// `engine-event` channel; tests implement it by recording.
pub trait EventSink: Send + Sync {
    fn emit(&self, event: &str, data: Value);
}

/// An [`EventSink`] that drops everything. For headless tests and the stdio CLI's
/// non-interactive paths.
pub struct NullSink;

impl EventSink for NullSink {
    fn emit(&self, _event: &str, _data: Value) {}
}

/// Turn a panic into an `internal` error envelope.
///
/// `rpc.py` wraps every method in a catch-all, so a corrupt image fails one call
/// rather than killing the engine. In-process a panic would take down the tray, the
/// hotkeys and the window with it, so the release profile unwinds and ported handlers
/// run inside this guard to restore the same contract.
pub fn guard<T>(f: impl FnOnce() -> Result<T, CoreError> + UnwindSafe) -> Result<T, CoreError> {
    match std::panic::catch_unwind(f) {
        Ok(result) => result,
        Err(payload) => Err(CoreError::internal(panic_message(&payload))),
    }
}

/// Best-effort text from a panic payload. `panic!` produces `&str` or `String`;
/// anything else is opaque and only its existence can be reported.
fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        format!("internal error: {s}")
    } else if let Some(s) = payload.downcast_ref::<String>() {
        format!("internal error: {s}")
    } else {
        "internal error: engine panicked".to_string()
    }
}

/// Shared test scaffolding.
///
/// The directory overrides are environment variables, and the environment is
/// process-global while `cargo test` runs modules in parallel threads. One lock per
/// module is not enough: a `config` test clearing the variables mid-run would send a
/// `gallery` test at the real `%APPDATA%`. Every test that touches them takes *this*
/// lock.
#[cfg(test)]
pub(crate) mod testing {
    use std::path::PathBuf;
    use std::sync::{Mutex, MutexGuard};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// A temporary `%APPDATA%`/`%LOCALAPPDATA%` pair, restored on drop.
    pub struct Sandbox {
        pub dir: PathBuf,
        _guard: MutexGuard<'static, ()>,
    }

    impl Sandbox {
        pub fn new(tag: &str) -> Self {
            let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let dir = std::env::temp_dir().join(format!("wc-test-{}-{tag}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join("cfg")).unwrap();
            std::fs::create_dir_all(dir.join("data")).unwrap();
            std::env::set_var("WALLPAPER_CHANGER_CONFIG_DIR", dir.join("cfg"));
            std::env::set_var("WALLPAPER_CHANGER_DATA_DIR", dir.join("data"));
            Self { dir, _guard: guard }
        }

        /// A real file on disk, so the gallery's reconciliation keeps it.
        pub fn file(&self, name: &str, contents: &[u8]) -> String {
            let path = self.dir.join(name);
            std::fs::write(&path, contents).unwrap();
            path.to_string_lossy().into_owned()
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            std::env::remove_var("WALLPAPER_CHANGER_CONFIG_DIR");
            std::env::remove_var("WALLPAPER_CHANGER_DATA_DIR");
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

/// The engine's state and method dispatch.
pub struct Core {
    events: Arc<dyn EventSink>,
}

impl Core {
    pub fn new(events: Arc<dyn EventSink>) -> Self {
        Self { events }
    }

    /// Raise an unsolicited engine event.
    pub fn emit(&self, event: &str, data: Value) {
        self.events.emit(event, data);
    }

    /// The live configuration, read fresh from disk.
    ///
    /// Deliberately **not** cached, unlike `Engine._config()` in `rpc.py`. While the
    /// sidecar still owns `save_config` and `_remember`, Python is writing
    /// `settings.toml` behind our back — a cache here would go stale the moment a
    /// hotkey toggled rotation. Re-reading a 2 KB file on the handful of calls that
    /// need it costs nothing, and the cache can come back with `save_config` in the
    /// phase that owns the session flags.
    fn config(&self) -> Result<Value, CoreError> {
        config::load_config(None)
    }

    /// The configuration with a caller-supplied overlay applied per section.
    ///
    /// Mirrors `Engine._merged`: the preview and the save dialog send an unsaved
    /// draft, and it must win over what is on disk without dropping the sections it
    /// does not mention.
    fn merged(&self, overlay: Option<&Value>) -> Result<Value, CoreError> {
        let mut cfg = self.config()?;
        let (Some(overlay), Some(base)) = (
            overlay.and_then(Value::as_object),
            cfg.as_object_mut(),
        ) else {
            return Ok(cfg);
        };
        for (section, values) in overlay {
            match (base.get_mut(section), values.as_object()) {
                (Some(Value::Object(existing)), Some(incoming)) => {
                    for (key, value) in incoming {
                        existing.insert(key.clone(), value.clone());
                    }
                }
                _ => {
                    base.insert(section.clone(), values.clone());
                }
            }
        }
        Ok(cfg)
    }

    /// Answer `method`, or decline it so the caller falls through to the sidecar.
    ///
    /// Unknown method names also return [`Dispatch::NotPorted`] rather than an error:
    /// while the sidecar exists it remains the allowlist gate, and letting it produce
    /// the rejection keeps one gate rather than two that can disagree.
    pub async fn dispatch(&self, method: &str, params: &Value) -> Dispatch {
        match method {
            "ping" => handled(|| {
                reject_unexpected(params, &[], "ping")?;
                Ok(json!({ "pong": true, "protocol": PROTOCOL_VERSION }))
            }),

            "get_monitors" => handled(|| {
                reject_unexpected(params, &[], "get_monitors")?;
                monitor::get_monitors_result()
            }),

            "list_folder_images" => handled(|| {
                reject_unexpected(params, &["folder"], "list_folder_images")?;
                Ok(images::list_folder_images(required_str(params, "folder")?))
            }),

            "scan_videos" => handled(|| {
                reject_unexpected(params, &["folder"], "scan_videos")?;
                Ok(images::scan_videos(required_str(params, "folder")?))
            }),

            "get_thumbnails" => handled(|| {
                reject_unexpected(params, &["paths", "size"], "get_thumbnails")?;
                let paths = required_str_list(params, "paths")?;
                let size = optional_i64(params, "size", 160)?;
                Ok(images::get_thumbnails(&paths, size))
            }),

            "get_image_preview" => handled(|| {
                reject_unexpected(params, &["path", "max_width"], "get_image_preview")?;
                let path = required_str(params, "path")?.to_string();
                let max_width = optional_i64(params, "max_width", 1400)?;
                images::get_image_preview(&path, max_width)
            }),

            "get_config" => handled(|| {
                reject_unexpected(params, &[], "get_config")?;
                Ok(config::get_config_result(&self.config()?))
            }),

            "get_translations" => handled(|| {
                reject_unexpected(params, &[], "get_translations")?;
                i18n::get_translations_result(&self.config()?)
            }),

            "get_startup_enabled" => handled(|| {
                reject_unexpected(params, &[], "get_startup_enabled")?;
                Ok(json!({ "enabled": startup::is_enabled() }))
            }),

            "set_startup_enabled" => handled(|| {
                reject_unexpected(params, &["enabled"], "set_startup_enabled")?;
                startup::set_enabled(required_bool(params, "enabled")?)?;
                Ok(json!({ "enabled": startup::is_enabled() }))
            }),

            "suggest_collage_path" => handled(|| {
                reject_unexpected(params, &["monitor", "config"], "suggest_collage_path")?;
                let cfg = self.merged(params.get("config"))?;
                gallery::suggest_collage_path_result(&cfg, optional_i64_opt(params, "monitor")?)
            }),

            "list_saved_collages" => handled(|| {
                reject_unexpected(params, &["config"], "list_saved_collages")?;
                Ok(gallery::list_saved_collages_result(&self.merged(params.get("config"))?))
            }),

            "forget_saved_collage" => handled(|| {
                reject_unexpected(params, &["path"], "forget_saved_collage")?;
                gallery::forget_saved_collage_result(required_str(params, "path")?)
            }),

            "preview" => handled(|| {
                reject_unexpected(params, &["config", "max_width", "images"], "preview")?;
                let cfg = self.merged(params.get("config"))?;
                compose::preview(
                    &cfg,
                    optional_i64(params, "max_width", 960)?,
                    optional_str_list(params, "images")?.as_deref(),
                )
            }),

            "save_collage" => handled(|| {
                reject_unexpected(
                    params,
                    &["config", "images", "monitor", "path"],
                    "save_collage",
                )?;
                let cfg = self.merged(params.get("config"))?;
                compose::save_collage(
                    &cfg,
                    optional_str_list(params, "images")?.as_deref(),
                    optional_i64_opt(params, "monitor")?,
                    params.get("path").and_then(Value::as_str),
                )
            }),

            // Each phase moves names out of this catch-all and into arms above it.
            _ => Dispatch::NotPorted,
        }
    }
}

// ── parameter extraction ─────────────────────────────────────────────────────
//
// `rpc.py` splats `params` as keyword arguments, so Python's own signature checking
// is what rejects a bad call, and a `TypeError` becomes `bad_params`. These helpers
// reproduce that: an unexpected key, a missing required key, or a value of the wrong
// shape all land on `bad_params` with a message naming the method.

/// Reject any key the method does not accept, the way a Python signature would.
///
/// Without this a typo in the webview would be silently ignored here while the
/// sidecar rejected it, so the two sides would disagree about the same request.
fn reject_unexpected(params: &Value, accepted: &[&str], method: &str) -> Result<(), CoreError> {
    let Some(object) = params.as_object() else {
        // A null or absent `params` is the no-arguments call.
        return if params.is_null() {
            Ok(())
        } else {
            Err(CoreError::bad_params(format!(
                "Bad params for {method}: params must be an object."
            )))
        };
    };
    for key in object.keys() {
        if !accepted.contains(&key.as_str()) {
            return Err(CoreError::bad_params(format!(
                "Bad params for {method}: unexpected keyword argument '{key}'"
            )));
        }
    }
    Ok(())
}

fn required_str<'a>(params: &'a Value, key: &str) -> Result<&'a str, CoreError> {
    match params.get(key) {
        Some(Value::String(s)) => Ok(s),
        Some(other) => Err(CoreError::bad_params(format!(
            "'{key}' must be a string, got {other}"
        ))),
        None => Err(CoreError::bad_params(format!(
            "missing a required argument: '{key}'"
        ))),
    }
}

fn required_str_list(params: &Value, key: &str) -> Result<Vec<String>, CoreError> {
    match params.get(key) {
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| {
                item.as_str().map(str::to_string).ok_or_else(|| {
                    CoreError::bad_params(format!("'{key}' must contain only strings"))
                })
            })
            .collect(),
        Some(other) => Err(CoreError::bad_params(format!(
            "'{key}' must be a list, got {other}"
        ))),
        None => Err(CoreError::bad_params(format!(
            "missing a required argument: '{key}'"
        ))),
    }
}

/// An optional list of strings. Absent and empty are different: `images: []` means
/// "no preset", the same as omitting it.
fn optional_str_list(params: &Value, key: &str) -> Result<Option<Vec<String>>, CoreError> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(items)) => {
            let list: Result<Vec<String>, CoreError> = items
                .iter()
                .map(|item| {
                    item.as_str().map(str::to_string).ok_or_else(|| {
                        CoreError::bad_params(format!("'{key}' must contain only strings"))
                    })
                })
                .collect();
            let list = list?;
            Ok(if list.is_empty() { None } else { Some(list) })
        }
        Some(other) => Err(CoreError::bad_params(format!(
            "'{key}' must be a list, got {other}"
        ))),
    }
}

fn required_bool(params: &Value, key: &str) -> Result<bool, CoreError> {
    match params.get(key) {
        Some(Value::Bool(b)) => Ok(*b),
        Some(other) => Err(CoreError::bad_params(format!(
            "'{key}' must be a boolean, got {other}"
        ))),
        None => Err(CoreError::bad_params(format!(
            "missing a required argument: '{key}'"
        ))),
    }
}

/// An optional whole number that is meaningfully absent — `monitor: null` means the
/// whole desktop, which is not the same as a default.
fn optional_i64_opt(params: &Value, key: &str) -> Result<Option<i64>, CoreError> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => n
            .as_i64()
            .map(Some)
            .ok_or_else(|| CoreError::bad_params(format!("'{key}' must be a whole number"))),
        Some(other) => Err(CoreError::bad_params(format!(
            "'{key}' must be a number, got {other}"
        ))),
    }
}

fn optional_i64(params: &Value, key: &str, default: i64) -> Result<i64, CoreError> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(Value::Number(n)) => n
            .as_i64()
            .or_else(|| n.as_f64().map(|f| f as i64))
            .ok_or_else(|| CoreError::bad_params(format!("'{key}' must be a whole number"))),
        Some(other) => Err(CoreError::bad_params(format!(
            "'{key}' must be a number, got {other}"
        ))),
    }
}

/// Run a synchronous handler body under the panic guard.
///
/// Sugar for the shape ported handlers take, keeping `AssertUnwindSafe` in one place
/// rather than repeated at every call site.
pub fn handled<T: Into<Value>>(
    f: impl FnOnce() -> Result<T, CoreError>,
) -> Dispatch {
    Dispatch::Handled(guard(AssertUnwindSafe(f)).map(Into::into))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn core() -> Core {
        Core::new(Arc::new(NullSink))
    }

    /// The allowlist must stay in step with `Engine._METHODS` in `rpc.py`.
    #[test]
    fn allowlist_has_every_method() {
        assert_eq!(METHODS.len(), 45, "rpc.py's _METHODS has 45 entries");
        let mut sorted = METHODS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), METHODS.len(), "duplicate method name");
    }

    /// Methods the core has taken over. Every phase adds to this list, and the test
    /// below asserts the split is exactly as claimed — a method that quietly stopped
    /// answering would otherwise fall back to Python and look like it still worked.
    const PORTED: &[&str] = &[
        "ping",
        "get_monitors",
        "list_folder_images",
        "scan_videos",
        "get_thumbnails",
        "get_image_preview",
        "get_config",
        "get_translations",
        "get_startup_enabled",
        "set_startup_enabled",
        "suggest_collage_path",
        "list_saved_collages",
        "forget_saved_collage",
        "preview",
        "save_collage",
    ];

    #[tokio::test]
    async fn ported_methods_answer_and_the_rest_fall_through() {
        let core = core();
        for method in METHODS {
            let decision = core.dispatch(method, &json!({})).await;
            let expected_ported = PORTED.contains(method);
            assert_eq!(
                !decision.is_not_ported(),
                expected_ported,
                "{method}: PORTED says {expected_ported}, dispatch disagrees"
            );
        }
    }

    #[tokio::test]
    async fn ping_reports_the_protocol_version() {
        let core = core();
        let Dispatch::Handled(Ok(result)) = core.dispatch("ping", &json!({})).await else {
            panic!("ping did not answer");
        };
        assert_eq!(result["pong"], true);
        assert_eq!(result["protocol"], PROTOCOL_VERSION);
    }

    /// An unrecognised name must fall through too, so the sidecar stays the single
    /// gate while it exists.
    #[tokio::test]
    async fn unknown_methods_fall_through_to_the_sidecar() {
        let core = core();
        assert!(core.dispatch("no_such_method", &Value::Null).await.is_not_ported());
    }

    #[test]
    fn guard_passes_success_through() {
        let out = guard(|| Ok::<_, CoreError>(7));
        assert_eq!(out.unwrap(), 7);
    }

    #[test]
    fn guard_passes_ordinary_errors_through_unchanged() {
        let out: Result<(), _> = guard(|| Err(CoreError::not_found("gone.png")));
        let err = out.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::NotFound);
        assert_eq!(err.to_string(), "gone.png");
    }

    /// The whole point of the guard: a panic becomes an error envelope, not a dead app.
    #[test]
    fn guard_turns_a_panic_into_an_internal_error() {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {})); // keep the test output quiet
        let out: Result<(), _> = guard(|| panic!("decoder exploded"));
        std::panic::set_hook(previous);

        let err = out.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Internal);
        assert!(err.to_string().contains("decoder exploded"), "got {err}");
    }

    #[test]
    fn events_reach_the_sink_verbatim() {
        struct Recorder(Mutex<Vec<(String, Value)>>);
        impl EventSink for Recorder {
            fn emit(&self, event: &str, data: Value) {
                self.0.lock().unwrap().push((event.to_string(), data));
            }
        }

        let recorder = Arc::new(Recorder(Mutex::new(Vec::new())));
        let core = Core::new(recorder.clone());
        core.emit("wallpaper_applied", serde_json::json!({ "output": "a.bmp" }));

        let seen = recorder.0.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].0, "wallpaper_applied");
        assert_eq!(seen[0].1["output"], "a.bmp");
    }
}
