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

pub mod apply;
pub mod collage;
pub mod compose;
pub mod config;
pub mod effects;
mod error;
pub mod gallery;
pub mod i18n;
pub mod images;
pub mod monitor;
pub mod scroll;
pub mod selection;
pub mod session;
pub mod startup;
pub mod transparency;
pub mod video;
pub mod workerw;

pub use apply::{WallpaperSetter, WindowsSetter};
pub use error::{CoreError, ErrorKind};
pub use monitor::{get_monitors, virtual_desktop, Monitor};
pub use session::{Bridge, BridgeFuture, Session};

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

    /// An [`EventSink`](crate::EventSink) that keeps what it was given, so a test can
    /// assert on the events a call raised as well as on its return value.
    #[derive(Default)]
    pub struct Recorder {
        pub events: Mutex<Vec<(String, serde_json::Value)>>,
    }

    impl crate::EventSink for Recorder {
        fn emit(&self, event: &str, data: serde_json::Value) {
            self.events
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push((event.to_string(), data));
        }
    }

    impl Recorder {
        /// Every payload raised under `name`, in order.
        pub fn of(&self, name: &str) -> Vec<serde_json::Value> {
            self.events
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .filter(|(event, _)| event == name)
                .map(|(_, data)| data.clone())
                .collect()
        }
    }
}

/// The engine's method dispatch, over the state in [`Session`].
///
/// Everything that outlives a single call — the apply lock, the wallpaper history,
/// the rotation timer, the unsaved settings — lives in the session behind an `Arc`,
/// so the rotation task can hold it without holding the dispatcher.
pub struct Core {
    session: Arc<Session>,
}

impl Core {
    /// The real engine: applies wallpapers through Windows.
    pub fn new(events: Arc<dyn EventSink>) -> Self {
        Self::with_setter(events, Arc::new(WindowsSetter))
    }

    /// The engine with the desktop swapped out, for tests.
    pub fn with_setter(events: Arc<dyn EventSink>, setter: Arc<dyn WallpaperSetter>) -> Self {
        Self {
            session: Arc::new(Session::new(events, setter)),
        }
    }

    /// The state behind the dispatcher, for the shell's startup and teardown paths.
    pub fn session(&self) -> &Arc<Session> {
        &self.session
    }

    /// Give the core a way to reach the methods the sidecar still owns.
    ///
    /// Only `save_config` needs it today, to ask whether the video player is running
    /// and to tell Python to re-read the file we just wrote. It disappears with the
    /// sidecar.
    pub fn set_bridge(&self, bridge: Arc<dyn Bridge>) {
        self.session.set_bridge(bridge);
    }

    /// Raise an unsolicited engine event.
    pub fn emit(&self, event: &str, data: Value) {
        self.session.emit(event, data);
    }

    fn config(&self) -> Result<Value, CoreError> {
        self.session.config()
    }

    fn merged(&self, overlay: Option<&Value>) -> Result<Value, CoreError> {
        self.session.merged(overlay)
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
                Ok(gallery::list_saved_collages_result(
                    &self.merged(params.get("config"))?,
                ))
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

            // ── apply, rotation and history ──────────────────────────────────
            //
            // These are async because they hold the apply lock across a
            // `spawn_blocking`. The panic guard the synchronous arms get from
            // `handled` is provided instead by `spawn_blocking` itself, which turns a
            // panic in the composition into a `JoinError` rather than unwinding
            // through the shell — what is left here is parameter shuffling.
            "apply_wallpaper" => {
                let outcome = async {
                    reject_unexpected(params, &["config", "images"], "apply_wallpaper")?;
                    let images = optional_str_list(params, "images")?;
                    self.session
                        .apply_wallpaper(params.get("config"), images)
                        .await
                }
                .await;
                Dispatch::Handled(outcome)
            }

            "apply_previous_wallpaper" => {
                let outcome = async {
                    reject_unexpected(params, &["config"], "apply_previous_wallpaper")?;
                    self.session
                        .apply_previous_wallpaper(params.get("config"))
                        .await
                }
                .await;
                Dispatch::Handled(outcome)
            }

            "apply_default_wallpaper" => {
                let outcome = async {
                    reject_unexpected(params, &["config"], "apply_default_wallpaper")?;
                    self.session
                        .apply_default_wallpaper(params.get("config"))
                        .await
                }
                .await;
                Dispatch::Handled(outcome)
            }

            "apply_saved_collage" => {
                let outcome = async {
                    reject_unexpected(params, &["path"], "apply_saved_collage")?;
                    let path = required_str(params, "path")?.to_string();
                    self.session.apply_saved_collage(&path).await
                }
                .await;
                Dispatch::Handled(outcome)
            }

            "set_effect" => {
                let outcome = async {
                    reject_unexpected(params, &["effect"], "set_effect")?;
                    let effect = required_str(params, "effect")?.to_string();
                    self.session.set_effect(&effect).await
                }
                .await;
                Dispatch::Handled(outcome)
            }

            "watch_start" => {
                let outcome = async {
                    reject_unexpected(params, &["interval"], "watch_start")?;
                    let interval = optional_i64_opt(params, "interval")?;
                    self.session.watch_start(interval).await
                }
                .await;
                Dispatch::Handled(outcome)
            }

            "watch_stop" => {
                let outcome = async {
                    reject_unexpected(params, &[], "watch_stop")?;
                    self.session.watch_stop().await
                }
                .await;
                Dispatch::Handled(outcome)
            }

            "watch_toggle" => {
                let outcome = async {
                    reject_unexpected(params, &[], "watch_toggle")?;
                    self.session.watch_toggle().await
                }
                .await;
                Dispatch::Handled(outcome)
            }

            "watch_status" => handled(|| {
                reject_unexpected(params, &[], "watch_status")?;
                Ok(self.session.watch_status())
            }),

            "save_config" => {
                let outcome = async {
                    reject_unexpected(params, &["config"], "save_config")?;
                    let incoming = params
                        .get("config")
                        .ok_or_else(|| {
                            CoreError::bad_params("missing a required argument: 'config'")
                        })?
                        .clone();
                    self.session.save_config(&incoming).await
                }
                .await;
                Dispatch::Handled(outcome)
            }

            // ── window transparency ──────────────────────────────────────────
            //
            // Unrelated to the wallpaper: a separate feature sharing the engine, with
            // its own file keyed by process name. All synchronous — the Win32 calls
            // are cheap and none of them composites anything.
            "list_windows" => handled(|| {
                reject_unexpected(params, &[], "list_windows")?;
                Ok(transparency::list_windows_result())
            }),

            "set_window_opacity" => handled(|| {
                reject_unexpected(params, &["hwnd", "alpha"], "set_window_opacity")?;
                let hwnd = required_i64(params, "hwnd")?;
                let alpha = required_i64(params, "alpha")?;
                Ok(transparency::set_window_opacity_result(hwnd, alpha))
            }),

            "get_foreground_window" => handled(|| {
                reject_unexpected(params, &[], "get_foreground_window")?;
                Ok(transparency::get_foreground_window_result())
            }),

            "toggle_foreground_opacity" => handled(|| {
                reject_unexpected(params, &[], "toggle_foreground_opacity")?;
                transparency::toggle_foreground_opacity_result()
            }),

            "get_opacity_settings" => handled(|| {
                reject_unexpected(params, &[], "get_opacity_settings")?;
                Ok(transparency::get_opacity_settings_result())
            }),

            "save_opacity_settings" => handled(|| {
                reject_unexpected(params, &["settings"], "save_opacity_settings")?;
                let settings = params.get("settings").ok_or_else(|| {
                    CoreError::bad_params("missing a required argument: 'settings'")
                })?;
                transparency::save_opacity_settings_result(settings)
            }),

            "reapply_opacity_settings" => handled(|| {
                reject_unexpected(params, &[], "reapply_opacity_settings")?;
                Ok(transparency::reapply_opacity_settings_result())
            }),

            "sync_scroll_transparency" => handled(|| {
                reject_unexpected(params, &[], "sync_scroll_transparency")?;
                self.session.sync_scroll_transparency()
            }),

            "scroll_transparency_status" => handled(|| {
                reject_unexpected(params, &[], "scroll_transparency_status")?;
                self.session.scroll_transparency_status()
            }),

            "get_capabilities" => handled(|| {
                reject_unexpected(params, &[], "get_capabilities")?;
                Ok(json!({
                    "protocol": PROTOCOL_VERSION,
                    "effects": effects::EFFECTS,
                    "has_mpv": video::has_mpv(),
                    "startup_enabled": startup::is_enabled(),
                    // Python asked whether pynput could be imported. The hook is
                    // native now, so on Windows there is nothing left to be missing.
                    "has_scroll_transparency": scroll::is_available(),
                }))
            }),

            "video_status" => handled(|| {
                reject_unexpected(params, &[], "video_status")?;
                Ok(self.session.video_status())
            }),

            "video_start" => {
                let outcome = async {
                    reject_unexpected(params, &["config"], "video_start")?;
                    self.session.video_start(params.get("config")).await
                }
                .await;
                Dispatch::Handled(outcome)
            }

            "video_stop" => {
                let outcome = async {
                    reject_unexpected(params, &[], "video_stop")?;
                    self.session.video_stop().await
                }
                .await;
                Dispatch::Handled(outcome)
            }

            "video_toggle" => {
                let outcome = async {
                    reject_unexpected(params, &["config"], "video_toggle")?;
                    self.session.video_toggle(params.get("config")).await
                }
                .await;
                Dispatch::Handled(outcome)
            }

            "video_next" => handled(|| {
                reject_unexpected(params, &[], "video_next")?;
                self.session.video_step(1)
            }),

            "video_prev" => handled(|| {
                reject_unexpected(params, &[], "video_prev")?;
                self.session.video_step(-1)
            }),

            "video_set_sound" => handled(|| {
                reject_unexpected(params, &["enabled"], "video_set_sound")?;
                self.session
                    .video_set_sound(required_bool(params, "enabled")?)
            }),

            "video_toggle_sound" => handled(|| {
                reject_unexpected(params, &[], "video_toggle_sound")?;
                self.session.video_toggle_sound()
            }),

            // `shutdown` is what the shell sends on the way out. The core tears the
            // desktop layer down; the sidecar is stopped separately by `Engine`, which
            // owns the pipe and does not route that through here.
            "shutdown" => handled(|| {
                reject_unexpected(params, &[], "shutdown")?;
                self.session.stop_rotation_for_exit();
                self.session.stop_scroll_for_exit();
                self.session.stop_video_for_exit();
                Ok(json!({ "bye": true }))
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

fn required_i64(params: &Value, key: &str) -> Result<i64, CoreError> {
    match params.get(key) {
        Some(Value::Number(n)) => n
            .as_i64()
            .or_else(|| n.as_f64().map(|f| f as i64))
            .ok_or_else(|| CoreError::bad_params(format!("'{key}' must be a whole number"))),
        Some(other) => Err(CoreError::bad_params(format!(
            "'{key}' must be a number, got {other}"
        ))),
        None => Err(CoreError::bad_params(format!(
            "missing a required argument: '{key}'"
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
pub fn handled<T: Into<Value>>(f: impl FnOnce() -> Result<T, CoreError>) -> Dispatch {
    Dispatch::Handled(guard(AssertUnwindSafe(f)).map(Into::into))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Never `Core::new` in a test: that one applies wallpapers through Windows, and
    /// `cargo test` runs on a machine with a desktop the developer is looking at.
    fn core() -> Core {
        Core::with_setter(
            Arc::new(NullSink),
            Arc::new(apply::testing::FakeSetter::default()),
        )
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
        "save_config",
        "apply_wallpaper",
        "apply_previous_wallpaper",
        "apply_default_wallpaper",
        "apply_saved_collage",
        "set_effect",
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
        "sync_scroll_transparency",
        "scroll_transparency_status",
        "get_capabilities",
        "video_status",
        "video_start",
        "video_stop",
        "video_toggle",
        "video_next",
        "video_prev",
        "video_set_sound",
        "video_toggle_sound",
        "shutdown",
    ];

    /// Which side answers each method, probed without letting any of them run.
    ///
    /// The parameter is a name no method accepts, so every ported arm stops at its
    /// `reject_unexpected` call and answers `bad_params`. That is still
    /// `Dispatch::Handled`, which is what this test is about — and it means the probe
    /// cannot composite a collage or change the desktop of the machine running
    /// `cargo test`. It also pins that every ported arm validates its parameters
    /// before doing any work.
    #[tokio::test]
    async fn ported_methods_answer_and_the_rest_fall_through() {
        let core = core();
        for method in METHODS {
            let decision = core
                .dispatch(method, &json!({ "__not_a_parameter": true }))
                .await;
            let expected_ported = PORTED.contains(method);
            assert_eq!(
                !decision.is_not_ported(),
                expected_ported,
                "{method}: PORTED says {expected_ported}, dispatch disagrees"
            );
            if let Dispatch::Handled(result) = decision {
                assert_eq!(
                    result.unwrap_err().kind(),
                    ErrorKind::BadParams,
                    "{method} accepted a parameter it does not have"
                );
            }
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
        assert!(core
            .dispatch("no_such_method", &Value::Null)
            .await
            .is_not_ported());
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
        core.emit(
            "wallpaper_applied",
            serde_json::json!({ "output": "a.bmp" }),
        );

        let seen = recorder.0.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].0, "wallpaper_applied");
        assert_eq!(seen[0].1["output"], "a.bmp");
    }
}
