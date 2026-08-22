//! The state the engine carries between calls: the apply lock, the wallpaper
//! history, the rotation timer, and the live-but-unsaved settings.
//!
//! Ports the parts of `Engine` in `rpc.py` that are *not* a method — `_apply_lock`,
//! `_history`/`_history_idx`, `_watch_timer`/`_watch_lock`, and the mutation of the
//! cached config that `set_effect` relies on. [`crate::Core`] is a thin dispatcher
//! over one of these.
//!
//! Three behaviours here are load-bearing and easy to "improve" by mistake:
//!
//! 1. **The apply lock never queues.** `rpc.py` takes it with `blocking=False`, so a
//!    second concurrent apply is refused with `busy` rather than waiting. Two hotkey
//!    presses in a row must not composite twice; `tokio::sync::Mutex::lock().await`
//!    would silently turn a refusal into a delay.
//! 2. **The rotation timer re-arms after the tick, not on a fixed schedule.**
//!    `threading.Timer` is one-shot and `_watch_tick` schedules the next one once the
//!    apply has returned, so the real period is interval + work time.
//!    `tokio::time::interval` catches up on missed ticks instead, which on a slow
//!    machine would composite back-to-back forever.
//! 3. **A failed tick must not stop the rotation.** Python logs it, emits an `error`
//!    event with `source: "watch"`, and re-arms anyway.
//!
//! ## Why the settings are read fresh, with a small overlay on top
//!
//! `Engine._config()` in `rpc.py` caches, and `set_effect` works by *mutating that
//! cache* — the effect goes live without being written to disk. A cache here would be
//! actively wrong while the sidecar still exists: Python writes `settings.toml`
//! behind our back whenever `video_start` toggles `video.enabled`, and our copy would
//! go stale. So the file stays the single source of truth and the only thing held in
//! memory is [`Session::overrides`] — the sparse set of values a session has changed
//! but not saved. It survives until `save_config` adopts or replaces it.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Map, Value};

use crate::apply::{self, WallpaperSetter};
use crate::scroll::{self, ScrollHook};
use crate::video::{self, VideoPlayer};
use crate::{blocking, config, effects, gallery, monitor, CoreError, EventSink};

/// How many applied image sets the "previous wallpaper" hotkey can step back
/// through. `_HISTORY_LIMIT` in `rpc.py`.
const HISTORY_LIMIT: usize = 50;

/// A call back into the half of the engine that is still Python.
///
/// Built when the port advanced one unit at a time and a few of them straddled:
/// `save_config` had to know whether the video player was running while the player was
/// still Python's. Now that video has moved, **one caller is left** —
/// [`Session::notify_sidecar_of_config_change`], keeping a cache fresh for a sidecar
/// whose only remaining method is `notify`, which does not read the configuration.
///
/// So this is already dead weight in practice. It survives to phase 9 rather than
/// being unpicked here because removing it means removing the sidecar plumbing it
/// rides on, which is that phase's whole job.
pub trait Bridge: Send + Sync {
    fn call(&self, method: &str, params: Value) -> BridgeFuture;
}

pub type BridgeFuture = Pin<Box<dyn Future<Output = Result<Value, String>> + Send>>;

/// Applied selections, oldest first, with a cursor for stepping back.
#[derive(Default)]
struct History {
    entries: Vec<Vec<String>>,
    /// -1 when nothing has been applied yet, matching `_history_idx`.
    cursor: i64,
}

impl History {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            cursor: -1,
        }
    }

    /// Record an applied selection, dropping anything ahead of the cursor.
    ///
    /// Stepping back and then applying something new forks the history — the
    /// abandoned future is discarded, exactly as `del self._history[idx + 1:]` does.
    fn push(&mut self, images: &[String]) {
        self.entries.truncate((self.cursor + 1).max(0) as usize);
        self.entries.push(images.to_vec());
        if self.entries.len() > HISTORY_LIMIT {
            self.entries.remove(0);
        }
        self.cursor = self.entries.len() as i64 - 1;
    }

    /// The selection one step back, without moving the cursor onto it yet.
    fn peek_previous(&self) -> Option<Vec<String>> {
        if self.cursor <= 0 {
            return None;
        }
        self.entries.get((self.cursor - 1) as usize).cloned()
    }

    fn step_back(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }
}

/// The rotation timer's shared state.
///
/// `running` mirrors `_watch_timer is not None`: it stays true for the whole of a
/// tick, so `watch_status` reports "watching" while an apply is in flight and the
/// tick knows it may re-arm afterwards.
#[derive(Default)]
struct Watch {
    running: bool,
    interval: i64,
    /// Set to `true` to end the loop. Held so a stop can interrupt a long sleep
    /// instead of leaving a task parked for an hour.
    stop: Option<tokio::sync::watch::Sender<bool>>,
}

pub struct Session {
    events: Arc<dyn EventSink>,
    bridge: Mutex<Option<Arc<dyn Bridge>>>,
    setter: Arc<dyn WallpaperSetter>,
    /// Settings changed for this session but not written — today only
    /// `display.effect`, set by `set_effect`. Section -> key -> value.
    overrides: Mutex<Map<String, Value>>,
    /// Never `.lock().await`. See the module docs.
    apply_lock: tokio::sync::Mutex<()>,
    history: Mutex<History>,
    watch: Mutex<Watch>,
    /// The video wallpaper. Owns a thread that owns the desktop-layer windows.
    video: VideoPlayer,
    /// The modifier+wheel hook. Owns two threads of its own while it is running.
    scroll: ScrollHook,
}

impl Session {
    pub fn new(events: Arc<dyn EventSink>, setter: Arc<dyn WallpaperSetter>) -> Self {
        let events_for_scroll = Arc::clone(&events);
        Self {
            events,
            bridge: Mutex::new(None),
            setter,
            overrides: Mutex::new(Map::new()),
            apply_lock: tokio::sync::Mutex::new(()),
            history: Mutex::new(History::new()),
            watch: Mutex::new(Watch::default()),
            video: VideoPlayer::new(Arc::clone(&events_for_scroll)),
            scroll: ScrollHook::new(events_for_scroll),
        }
    }

    pub fn set_bridge(&self, bridge: Arc<dyn Bridge>) {
        *self.bridge.lock().unwrap_or_else(|e| e.into_inner()) = Some(bridge);
    }

    pub fn emit(&self, event: &str, data: Value) {
        self.events.emit(event, data);
    }

    // ── configuration ────────────────────────────────────────────────────────

    /// The live configuration: what is on disk, with this session's unsaved changes
    /// laid over it.
    pub fn config(&self) -> Result<Value, CoreError> {
        let mut cfg = config::load_config(None)?;
        let overrides = self.overrides.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(base) = cfg.as_object_mut() {
            overlay(base, &overrides);
        }
        Ok(cfg)
    }

    /// The configuration with a caller-supplied draft applied per section.
    ///
    /// Mirrors `Engine._merged`: the preview and the save dialog send settings the
    /// user has not saved, and they must win without dropping the sections they do
    /// not mention.
    pub fn merged(&self, draft: Option<&Value>) -> Result<Value, CoreError> {
        let mut cfg = self.config()?;
        let (Some(draft), Some(base)) = (draft.and_then(Value::as_object), cfg.as_object_mut())
        else {
            return Ok(cfg);
        };
        overlay(base, draft);
        Ok(cfg)
    }

    /// Change a setting for this session only, the way `set_effect` does.
    fn set_override(&self, section: &str, key: &str, value: Value) {
        let mut overrides = self.overrides.lock().unwrap_or_else(|e| e.into_inner());
        overrides
            .entry(section.to_string())
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .map(|table| table.insert(key.to_string(), value));
    }

    fn clear_overrides(&self) {
        self.overrides
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }

    /// Where the composed wallpaper is written, created if it is not there yet.
    fn output_dir(&self, cfg: &Value) -> Result<PathBuf, CoreError> {
        let dir = config::resolve_output_dir(cfg);
        std::fs::create_dir_all(&dir)
            .map_err(|e| CoreError::io(format!("Could not create {}: {e}", dir.display())))?;
        Ok(dir)
    }

    /// Write one session flag straight to `settings.toml`.
    ///
    /// Ports `Engine._remember`. Rotation is toggled from four places — the window,
    /// the tray, a global hotkey, the CLI — and the next launch is supposed to come
    /// up the way the user left it. Persisting on the toggle, rather than waiting for
    /// an explicit Save, is what makes that true even if the window is never opened.
    ///
    /// Losing the flag only costs the restore on the next launch, so a failure is
    /// logged into the error event rather than failing the call.
    async fn remember(&self, section: &str, key: &str, value: Value) {
        let Ok(mut cfg) = self.config() else { return };
        if cfg.pointer(&format!("/{section}/{key}")) == Some(&value) {
            return;
        }
        if let Some(base) = cfg.as_object_mut() {
            base.entry(section.to_string())
                .or_insert_with(|| Value::Object(Map::new()))
                .as_object_mut()
                .map(|table| table.insert(key.to_string(), value));
        }
        if config::save_config(&cfg, None).is_ok() {
            self.notify_sidecar_of_config_change().await;
        }
    }

    /// Tell the sidecar to drop its cached configuration.
    ///
    /// Transitional. `Engine._config()` in `rpc.py` caches, and Python no longer sees
    /// its own writes now that the core owns them — without this, the next
    /// `_remember` over there would write a stale copy back over what we just saved,
    /// silently undoing a rotation toggle.
    async fn notify_sidecar_of_config_change(&self) {
        let bridge = self
            .bridge
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(bridge) = bridge {
            let _ = bridge.call("_reload_config", json!({})).await;
        }
    }

    // ── applying ─────────────────────────────────────────────────────────────

    /// Claim the right to apply, or refuse.
    ///
    /// **Never blocks.** `rpc.py` takes this lock with `blocking=False`, so a second
    /// concurrent apply is told `busy` rather than queued behind the first. Two
    /// hotkey presses in quick succession should not composite twice, and waiting
    /// here would turn a clean refusal into a stall the caller cannot see.
    fn begin_apply(&self) -> Result<tokio::sync::MutexGuard<'_, ()>, CoreError> {
        self.apply_lock
            .try_lock()
            .map_err(|_| CoreError::busy("An apply is already running."))
    }

    /// Compose and apply the collage, holding the apply lock for the duration.
    ///
    /// The composition runs on a blocking thread: a 5760x2160 canvas takes long
    /// enough that leaving it on an async worker would stall every other call.
    async fn compose_and_apply(
        self: &Arc<Self>,
        cfg: Value,
        preset: Option<Vec<String>>,
    ) -> Result<(PathBuf, Vec<String>), CoreError> {
        let _guard = self.begin_apply()?;
        let monitors = monitor::get_monitors()?;
        let output_dir = self.output_dir(&cfg)?;
        let setter = Arc::clone(&self.setter);

        blocking(move || {
            apply::apply_collage(&cfg, &monitors, &output_dir, preset.as_deref(), &*setter)
        })
        .await
    }

    /// `apply_wallpaper` — compose the collage and put it on the desktop.
    pub async fn apply_wallpaper(
        self: &Arc<Self>,
        draft: Option<&Value>,
        preset: Option<Vec<String>>,
    ) -> Result<Value, CoreError> {
        let cfg = self.merged(draft)?;
        let (out, images) = self.compose_and_apply(cfg, preset).await?;
        // Outside the lock, exactly as in `rpc.py`: the history describes what was
        // applied, and recording it is not part of the critical section.
        self.history
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(&images);
        Ok(self.announce(out, images))
    }

    /// `apply_previous_wallpaper` — replay the selection shown before this one.
    ///
    /// The selection is replayed rather than re-picked, so stepping back is
    /// deterministic; the cursor only moves once the apply has succeeded.
    pub async fn apply_previous_wallpaper(
        self: &Arc<Self>,
        draft: Option<&Value>,
    ) -> Result<Value, CoreError> {
        let previous = self
            .history
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .peek_previous()
            .ok_or_else(|| CoreError::no_history("No previous wallpaper in history."))?;

        let cfg = self.merged(draft)?;
        let (out, _) = self.compose_and_apply(cfg, Some(previous.clone())).await?;
        self.history
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .step_back();
        Ok(self.announce(out, previous))
    }

    /// `apply_default_wallpaper` — the configured single picture, on every screen.
    ///
    /// Deliberately outside the apply lock and outside the history, as in `rpc.py`:
    /// it composes nothing from the rotation and there is no selection to replay.
    pub async fn apply_default_wallpaper(
        self: &Arc<Self>,
        draft: Option<&Value>,
    ) -> Result<Value, CoreError> {
        let cfg = self.merged(draft)?;
        let raw = cfg
            .pointer("/paths/default_wallpaper")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if raw.is_empty() {
            return Err(CoreError::not_configured(
                "No default wallpaper is configured.",
            ));
        }
        let path = PathBuf::from(&raw);
        if !path.exists() {
            return Err(CoreError::not_found(format!(
                "Default wallpaper not found: {raw}"
            )));
        }

        let monitors = monitor::get_monitors()?;
        let output_dir = self.output_dir(&cfg)?;
        let fit_mode = string_at(&cfg, "/display/fit_mode", "fill");
        let effect = string_at(&cfg, "/display/effect", "normal");
        let setter = Arc::clone(&self.setter);

        let out = blocking(move || {
            apply::apply_single(&path, &monitors, &output_dir, &fit_mode, &effect, &*setter)
        })
        .await?;
        Ok(self.announce(out, vec![raw]))
    }

    /// `apply_saved_collage` — put an exported picture back on the desktop as it was.
    ///
    /// Nothing is recomposed and no effect is applied on top: the file already
    /// carries whichever effect was active when it was made. How it is laid down
    /// follows what the library says it is — a whole-desktop export spans every
    /// screen, a single-screen crop is placed on each screen at that screen's size.
    ///
    /// Deliberately *not* pushed onto the history: that history replays image
    /// *selections* through the collage composer, and a flattened picture put through
    /// it would come back as a collage of itself.
    pub async fn apply_saved_collage(self: &Arc<Self>, path: &str) -> Result<Value, CoreError> {
        let target = PathBuf::from(path);
        if !target.is_file() {
            return Err(CoreError::not_found(format!(
                "Saved collage not found: {path}"
            )));
        }
        let monitors = monitor::get_monitors()?;
        if monitors.is_empty() {
            return Err(CoreError::no_monitors("No monitors detected."));
        }

        // An unknown file is treated as a full-desktop picture: that is what the app
        // exports unless asked otherwise, and spanning is the gentler mistake.
        let whole_desktop = gallery::find(path)
            .map(|entry| entry.get("monitor").map(Value::is_null).unwrap_or(true))
            .unwrap_or(true);

        let cfg = self.config()?;
        let output_dir = self.output_dir(&cfg)?;
        let fit_mode = string_at(&cfg, "/display/fit_mode", "fill");
        let setter = Arc::clone(&self.setter);

        let _guard = self.begin_apply()?;
        let placed = target.clone();
        let out = blocking(move || {
            if whole_desktop {
                apply::apply_desktop(&placed, &monitors, &output_dir, &fit_mode, &*setter)
            } else {
                apply::apply_single(
                    &placed,
                    &monitors,
                    &output_dir,
                    &fit_mode,
                    "normal",
                    &*setter,
                )
            }
        })
        .await?;

        Ok(self.announce(out, vec![path.to_string()]))
    }

    /// `set_effect` — switch the visual effect and re-apply immediately.
    ///
    /// Live but not persisted, as it was in the GUI: saving stays an explicit action.
    pub async fn set_effect(self: &Arc<Self>, effect: &str) -> Result<Value, CoreError> {
        if !effects::EFFECTS.contains(&effect) {
            return Err(CoreError::invalid(format!("Unknown effect: {effect}")));
        }
        self.set_override("display", "effect", Value::String(effect.to_string()));
        let mut result = self.apply_wallpaper(None, None).await?;
        result["effect"] = Value::String(effect.to_string());
        Ok(result)
    }

    /// Build the `wallpaper_applied` payload and raise the event.
    fn announce(&self, out: PathBuf, images: Vec<String>) -> Value {
        let result = json!({
            "output": out.to_string_lossy(),
            "images": images,
        });
        self.emit("wallpaper_applied", result.clone());
        result
    }

    // ── rotation timer ───────────────────────────────────────────────────────

    pub fn watch_status(&self) -> Value {
        let watch = self.watch.lock().unwrap_or_else(|e| e.into_inner());
        json!({ "watching": watch.running })
    }

    fn is_watching(&self) -> bool {
        self.watch.lock().unwrap_or_else(|e| e.into_inner()).running
    }

    /// `watch_start` — begin (or restart) the rotation.
    pub async fn watch_start(self: &Arc<Self>, interval: Option<i64>) -> Result<Value, CoreError> {
        let cfg = self.config()?;
        let configured = cfg
            .pointer("/general/interval")
            .and_then(Value::as_i64)
            .unwrap_or(300);
        // `max(1, int(interval or ...))`: a zero or absent interval falls back to the
        // configured one, and nothing below a second is allowed.
        let secs = interval.filter(|v| *v != 0).unwrap_or(configured).max(1);

        {
            let mut watch = self.watch.lock().unwrap_or_else(|e| e.into_inner());
            stop_locked(&mut watch);
            let (tx, rx) = tokio::sync::watch::channel(false);
            watch.running = true;
            watch.interval = secs;
            watch.stop = Some(tx);
            spawn_ticker(Arc::clone(self), secs, rx);
        }

        self.remember("general", "rotation_active", Value::Bool(true))
            .await;
        Ok(json!({ "watching": true, "interval": secs }))
    }

    /// Silence the rotation on the way out of the process.
    ///
    /// Deliberately *not* `watch_stop`: that persists `rotation_active = false`, and
    /// the whole point of the flag is that a rotation left running comes back on the
    /// next launch. This only ends the task, so an apply cannot still be compositing
    /// and calling `SystemParametersInfoW` while the process tears down. Sync, because
    /// it runs from the exit handler where there is no runtime to await on.
    pub fn stop_rotation_for_exit(&self) {
        let mut watch = self.watch.lock().unwrap_or_else(|e| e.into_inner());
        stop_locked(&mut watch);
    }

    /// `watch_stop` — end the rotation. A tick already in flight still finishes.
    pub async fn watch_stop(&self) -> Result<Value, CoreError> {
        {
            let mut watch = self.watch.lock().unwrap_or_else(|e| e.into_inner());
            stop_locked(&mut watch);
        }
        self.remember("general", "rotation_active", Value::Bool(false))
            .await;
        Ok(json!({ "watching": false }))
    }

    /// `watch_toggle` — start the rotation if idle, stop it if running.
    pub async fn watch_toggle(self: &Arc<Self>) -> Result<Value, CoreError> {
        if self.is_watching() {
            self.watch_stop().await
        } else {
            self.watch_start(None).await
        }
    }

    /// Bring the rotation back up if the user left it running.
    ///
    /// The sidecar's `restore_session` used to do this half; it cannot any more,
    /// because the timer lives here now. Returns whether the rotation was restarted.
    pub async fn restore_rotation(self: &Arc<Self>) -> bool {
        let Ok(cfg) = self.config() else { return false };
        if cfg
            .pointer("/general/rotation_active")
            .and_then(Value::as_bool)
            != Some(true)
        {
            return false;
        }
        self.watch_start(None).await.is_ok()
    }

    /// One rotation tick. A failure must not stop the rotation.
    async fn tick(self: &Arc<Self>) {
        if let Err(e) = self.apply_wallpaper(None, None).await {
            self.emit(
                "error",
                json!({ "source": "watch", "message": e.message() }),
            );
        }
    }

    // ── modifier + wheel transparency ────────────────────────────────────────

    /// `sync_scroll_transparency` — match the hook to the saved settings.
    ///
    /// Called at start-up and after every save, so turning the switch off really
    /// removes the system-wide hook rather than leaving it installed until the next
    /// restart. Synchronous even though `save_config` awaits it: starting or stopping
    /// joins two threads that do nothing but a file write, and the settings screen is
    /// waiting on the answer to draw its badge.
    pub fn sync_scroll_transparency(&self) -> Result<Value, CoreError> {
        let cfg = self.config()?;
        let modifier = scroll::normalize_modifier(
            cfg.pointer("/hotkeys/scroll_modifier")
                .and_then(Value::as_str),
        );
        if cfg
            .pointer("/hotkeys/scroll_enabled")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            self.scroll.start(modifier);
        } else {
            self.scroll.stop();
        }
        self.scroll_transparency_status()
    }

    /// `scroll_transparency_status` — what the hook is actually doing.
    ///
    /// `enabled` is what the configuration asks for; `running` is whether the hook is
    /// installed. They differ when it could not be installed at all, and that gap is
    /// the thing the interface needs to surface.
    pub fn scroll_transparency_status(&self) -> Result<Value, CoreError> {
        let cfg = self.config()?;
        let mut status = self.scroll.status();
        let enabled = cfg
            .pointer("/hotkeys/scroll_enabled")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if let Some(status) = status.as_object_mut() {
            status.insert("enabled".to_string(), json!(enabled));
            status.insert("modifiers".to_string(), json!(scroll::SUPPORTED_MODIFIERS));
            status.insert("step".to_string(), json!(scroll::STEP));
        }
        Ok(status)
    }

    /// Remove the hook on the way out of the process.
    ///
    /// A `WH_MOUSE_LL` hook is system-wide, so leaving it behind would mean every
    /// wheel event on the machine still being routed through a dead process. This
    /// also flushes whatever scroll was still inside the save debounce.
    pub fn stop_scroll_for_exit(&self) {
        self.scroll.stop();
    }

    // ── video wallpaper ──────────────────────────────────────────────────────

    /// `video_status` — what the player is doing, and whether it could play at all.
    pub fn video_status(&self) -> Value {
        video::status_result(&self.video)
    }

    /// `video_start` — play the configured folder on every screen.
    pub async fn video_start(&self, draft: Option<&Value>) -> Result<Value, CoreError> {
        let cfg = self.merged(draft)?;
        let (videos, loop_playlist, sound, monitors) = video::start_inputs(&cfg)?;
        self.video.start(videos, loop_playlist, sound, monitors)?;
        // Session state, not a preference: the next launch comes back the way the user
        // left it. Written the moment it changes, from wherever it changed.
        self.remember("video", "enabled", Value::Bool(true)).await;
        Ok(self.video_status())
    }

    /// `video_stop` — tear the player and its host windows down.
    pub async fn video_stop(&self) -> Result<Value, CoreError> {
        self.video.stop();
        self.remember("video", "enabled", Value::Bool(false)).await;
        Ok(self.video_status())
    }

    /// `video_next` / `video_prev` — move every screen to the same playlist entry.
    pub fn video_step(&self, direction: i64) -> Result<Value, CoreError> {
        self.video.step(direction);
        Ok(self.video_status())
    }

    /// `video_set_sound` — turn audio on or off while it plays.
    pub fn video_set_sound(&self, enabled: bool) -> Result<Value, CoreError> {
        self.video.set_sound(enabled);
        Ok(self.video_status())
    }

    /// `video_toggle` — start if stopped, stop if playing.
    pub async fn video_toggle(&self, draft: Option<&Value>) -> Result<Value, CoreError> {
        if self.video.is_running() {
            self.video_stop().await
        } else {
            self.video_start(draft).await
        }
    }

    /// `video_toggle_sound` — flip audio, live if something is playing.
    ///
    /// Like `set_effect`, the new value is a session override rather than a write: the
    /// hotkey should not rewrite `settings.toml` on every press, and `save_config` will
    /// adopt it if the user saves.
    pub fn video_toggle_sound(&self) -> Result<Value, CoreError> {
        let enabled = !self
            .config()?
            .pointer("/video/sound")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        self.set_override("video", "sound", Value::Bool(enabled));
        if self.video.is_running() {
            self.video.set_sound(enabled);
        }
        let mut status = self.video_status();
        if let Some(status) = status.as_object_mut() {
            status.insert("sound".to_string(), Value::Bool(enabled));
        }
        Ok(status)
    }

    /// Bring back what was running when the app was last closed.
    ///
    /// Both halves, and one event. `restore_session` in `rpc.py` did this on a daemon
    /// thread at start-up — never through `dispatch`, so the strangler seam could not
    /// see it — which is why each half had to move the moment its unit was ported.
    pub async fn restore_session(self: &Arc<Self>) -> Value {
        let rotation = self.restore_rotation().await;

        let wanted = self
            .config()
            .ok()
            .and_then(|cfg| cfg.pointer("/video/enabled").and_then(Value::as_bool))
            .unwrap_or(false);
        let mut video = false;
        if wanted && video::has_mpv() {
            match self.video_start(None).await {
                Ok(_) => video = true,
                // A video that will not come back must not stop the app from starting.
                Err(e) => log::warn!("could not restore the video wallpaper: {}", e.message()),
            }
        }

        let restored = json!({ "rotation": rotation, "video": video });
        self.emit("session_restored", restored.clone());
        restored
    }

    /// Tear the desktop layer down on the way out of the process.
    ///
    /// The host windows are children of WORKERW. Leaving them behind strands them on
    /// the desktop until Explorer restarts, so this has to run on every exit path.
    pub fn stop_video_for_exit(&self) {
        self.video.stop();
    }

    // ── saving the configuration ─────────────────────────────────────────────

    /// `save_config` — persist the client's settings and adopt them.
    ///
    /// The session flags are overwritten with what is actually running rather than
    /// what the client sent. A hotkey or the tray can have flipped rotation since the
    /// window read the config, and taking the client's copy would let a stale draft
    /// switch it back off on the next launch. `video.enabled` still has to be asked
    /// for: the player is the sidecar's.
    pub async fn save_config(&self, incoming: &Value) -> Result<Value, CoreError> {
        let Some(sections) = incoming.as_object() else {
            return Err(CoreError::invalid("Configuration must be an object."));
        };

        let current = self.config()?;
        let config_path = current
            .get("_config_path")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| config::default_config_path().to_string_lossy().into_owned());

        let mut merged = Map::new();
        for (section, values) in sections {
            if section.starts_with('_') {
                continue;
            }
            merged.insert(section.clone(), values.clone());
        }
        merged.insert("_config_path".into(), Value::String(config_path.clone()));

        set_in(
            &mut merged,
            "general",
            "rotation_active",
            json!(self.is_watching()),
        );
        set_in(
            &mut merged,
            "video",
            "enabled",
            json!(self.video.is_running()),
        );

        let merged = Value::Object(merged);
        config::save_config(&merged, None)?;
        // The client's settings are now what is on disk, so a session override would
        // only shadow them.
        self.clear_overrides();

        self.notify_sidecar_of_config_change().await;
        // The scroll hook holds a system-wide hook, so it has to follow the saved
        // settings immediately rather than waiting for a restart.
        let _ = self.sync_scroll_transparency();

        Ok(json!({ "saved": true, "config_path": config_path }))
    }
}

/// Stop the ticker, if any, leaving the lock held by the caller.
fn stop_locked(watch: &mut Watch) {
    if let Some(stop) = watch.stop.take() {
        let _ = stop.send(true);
    }
    watch.running = false;
}

/// The rotation loop.
///
/// Sleep, tick, sleep again — the next wait starts only once the apply has returned,
/// so the period is interval + work time. This is what `threading.Timer` re-armed
/// from `_watch_tick` does, and it is deliberately *not* `tokio::time::interval`,
/// which would fire immediately and then try to catch up on ticks it missed.
fn spawn_ticker(session: Arc<Session>, secs: i64, mut stop: tokio::sync::watch::Receiver<bool>) {
    tokio::spawn(async move {
        let period = Duration::from_secs(secs.max(1) as u64);
        loop {
            tokio::select! {
                _ = tokio::time::sleep(period) => {}
                _ = stop.changed() => break,
            }
            if *stop.borrow() {
                break;
            }
            session.tick().await;
            // A stop during the apply ends the rotation here, matching Python's
            // "re-arm only if the timer is still set" check after the tick.
            if *stop.borrow() {
                break;
            }
        }
    });
}

/// Merge `incoming` onto `base`, one section deep — the shape `_merged` uses.
fn overlay(base: &mut Map<String, Value>, incoming: &Map<String, Value>) {
    for (section, values) in incoming {
        if section.starts_with('_') {
            continue;
        }
        match (base.get_mut(section), values.as_object()) {
            (Some(Value::Object(existing)), Some(fields)) => {
                for (key, value) in fields {
                    existing.insert(key.clone(), value.clone());
                }
            }
            _ => {
                base.insert(section.clone(), values.clone());
            }
        }
    }
}

fn set_in(map: &mut Map<String, Value>, section: &str, key: &str, value: Value) {
    map.entry(section.to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .map(|table| table.insert(key.to_string(), value));
}

fn string_at(cfg: &Value, pointer: &str, fallback: &str) -> String {
    cfg.pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or(fallback)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::testing::FakeSetter;
    use crate::testing::{Recorder, Sandbox};
    use crate::ErrorKind;

    /// A folder with no pictures in it, so an apply fails immediately instead of
    /// composing a desktop-sized canvas.
    ///
    /// What these tests are about is the machinery *around* the apply — the lock, the
    /// history, the timer, the flags. Composition itself has its own golden suite, and
    /// letting it run here would mean every case waited on a full-resolution canvas.
    ///
    /// The settings file is always written before anything reads it: `load_config`
    /// otherwise migrates the checkout's real `config/settings.toml` in, whose
    /// `wallpapers_folder` points at the developer's own pictures.
    fn empty_folder(sandbox: &Sandbox) -> String {
        let dir = sandbox.dir.join("nopics");
        std::fs::create_dir_all(&dir).unwrap();
        dir.to_string_lossy().replace('\\', "/")
    }

    fn write_settings(sandbox: &Sandbox, body: &str) {
        std::fs::write(sandbox.dir.join("cfg").join("settings.toml"), body).unwrap();
    }

    fn headless(events: Arc<Recorder>) -> Arc<Session> {
        Arc::new(Session::new(events, Arc::new(FakeSetter::default())))
    }

    // ── the apply lock, which must refuse rather than queue ──────────────────

    #[tokio::test]
    async fn a_second_apply_is_refused_while_the_first_holds_the_lock() {
        let session = headless(Arc::new(Recorder::default()));

        let held = session
            .begin_apply()
            .expect("the first apply takes the lock");
        let err = session.begin_apply().unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Busy);
        assert_eq!(err.message(), "An apply is already running.");

        // And it is a refusal, not a queue: the lock is free the instant it is let go.
        drop(held);
        assert!(session.begin_apply().is_ok());
    }

    /// `no_history` is answered from the ring alone — before the config is read, the
    /// monitors are enumerated, or anything is composed.
    #[tokio::test]
    async fn stepping_back_with_no_history_is_refused_before_any_work() {
        let setter = Arc::new(FakeSetter::default());
        let session = Arc::new(Session::new(Arc::new(Recorder::default()), setter.clone()));

        let err = session.apply_previous_wallpaper(None).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::NoHistory);
        assert_eq!(err.message(), "No previous wallpaper in history.");
        assert_eq!(setter.count(), 0, "nothing should have been applied");
    }

    // ── the rotation timer ───────────────────────────────────────────────────

    #[tokio::test]
    async fn rotation_reports_its_state_and_persists_the_flag() {
        let sandbox = Sandbox::new("watchflag");
        let pictures = empty_folder(&sandbox);
        let recorder = Arc::new(Recorder::default());
        write_settings(
            &sandbox,
            &format!("[general]\ninterval = 3600\nrotation_active = false\n\n[paths]\nwallpapers_folder = {pictures:?}\n"),
        );
        let session = headless(recorder);

        assert_eq!(session.watch_status()["watching"], false);

        let started = session.watch_start(Some(3600)).await.unwrap();
        assert_eq!(started["watching"], true);
        assert_eq!(started["interval"], 3600);
        assert_eq!(session.watch_status()["watching"], true);
        assert_eq!(
            session.config().unwrap()["general"]["rotation_active"],
            true,
            "the flag must reach settings.toml, or the next launch forgets"
        );

        assert_eq!(session.watch_stop().await.unwrap()["watching"], false);
        assert_eq!(
            session.config().unwrap()["general"]["rotation_active"],
            false
        );

        // Toggling is start/stop, driven by the live state rather than the file.
        assert_eq!(session.watch_toggle().await.unwrap()["watching"], true);
        assert_eq!(session.watch_toggle().await.unwrap()["watching"], false);
    }

    /// An interval of zero or none falls back to the configured one.
    #[tokio::test]
    async fn an_absent_interval_falls_back_to_the_configuration() {
        let sandbox = Sandbox::new("interval");
        let pictures = empty_folder(&sandbox);
        write_settings(
            &sandbox,
            &format!("[general]\ninterval = 900\n\n[paths]\nwallpapers_folder = {pictures:?}\n"),
        );
        let session = headless(Arc::new(Recorder::default()));

        assert_eq!(session.watch_start(None).await.unwrap()["interval"], 900);
        assert_eq!(session.watch_start(Some(0)).await.unwrap()["interval"], 900);
        assert_eq!(session.watch_start(Some(45)).await.unwrap()["interval"], 45);
        let _ = session.watch_stop().await;
    }

    /// The two rules that make rotation survivable: a tick that fails is reported and
    /// the timer carries on, and stopping actually stops it.
    ///
    /// The wallpapers folder is empty, so every tick fails fast rather than
    /// composing — which is what makes a real-time test of this bearable.
    #[tokio::test]
    async fn a_failing_tick_is_reported_and_the_rotation_carries_on() {
        let sandbox = Sandbox::new("ticks");
        let pictures = empty_folder(&sandbox);
        write_settings(
            &sandbox,
            &format!("[general]\ninterval = 1\nselection = \"sequential\"\n\n[paths]\nwallpapers_folder = {pictures:?}\n"),
        );
        let recorder = Arc::new(Recorder::default());
        let session = headless(recorder.clone());

        session.watch_start(Some(1)).await.unwrap();
        tokio::time::sleep(Duration::from_millis(2600)).await;

        let failures = recorder.of("error");
        assert!(
            failures.len() >= 2,
            "expected the timer to re-arm after a failed tick, saw {} tick(s)",
            failures.len()
        );
        assert_eq!(failures[0]["source"], "watch");
        assert!(
            session.watch_status()["watching"].as_bool().unwrap(),
            "a failing tick must not stop the rotation"
        );

        session.watch_stop().await.unwrap();
        let after_stop = recorder.of("error").len();
        tokio::time::sleep(Duration::from_millis(1500)).await;
        assert_eq!(
            recorder.of("error").len(),
            after_stop,
            "the timer kept ticking after it was stopped"
        );
    }

    /// Exiting stops the timer without forgetting that rotation was on.
    ///
    /// `watch_stop` would write `rotation_active = false`, which would mean quitting
    /// the app silently turns rotation off for the next launch.
    #[tokio::test]
    async fn stopping_for_exit_ends_the_ticker_but_keeps_the_flag() {
        let sandbox = Sandbox::new("exit-stop");
        let pictures = empty_folder(&sandbox);
        write_settings(
            &sandbox,
            &format!("[general]\ninterval = 1\nselection = \"sequential\"\n\n[paths]\nwallpapers_folder = {pictures:?}\n"),
        );
        let recorder = Arc::new(Recorder::default());
        let session = headless(recorder.clone());

        session.watch_start(Some(1)).await.unwrap();
        let settings = sandbox.dir.join("cfg").join("settings.toml");
        assert!(
            std::fs::read_to_string(&settings)
                .unwrap()
                .contains("rotation_active = true"),
            "watch_start should have remembered the rotation"
        );

        session.stop_rotation_for_exit();
        assert_eq!(session.watch_status()["watching"], false);

        let after_stop = recorder.of("error").len();
        tokio::time::sleep(Duration::from_millis(2500)).await;
        assert_eq!(
            recorder.of("error").len(),
            after_stop,
            "the timer kept ticking after the exit stop"
        );
        assert!(
            std::fs::read_to_string(&settings)
                .unwrap()
                .contains("rotation_active = true"),
            "exiting must not turn rotation off for the next launch"
        );
    }

    /// Rotation comes back the way the user left it, and only then.
    #[tokio::test]
    async fn rotation_is_restored_only_when_the_flag_says_so() {
        let sandbox = Sandbox::new("restore");
        let pictures = empty_folder(&sandbox);
        let settings = sandbox.dir.join("cfg").join("settings.toml");
        std::fs::write(
            &settings,
            format!("[general]\ninterval = 3600\nrotation_active = false\n\n[paths]\nwallpapers_folder = {pictures:?}\n"),
        )
        .unwrap();
        let idle = headless(Arc::new(Recorder::default()));
        assert!(!idle.restore_rotation().await);
        assert_eq!(idle.watch_status()["watching"], false);

        std::fs::write(
            &settings,
            format!("[general]\ninterval = 3600\nrotation_active = true\n\n[paths]\nwallpapers_folder = {pictures:?}\n"),
        )
        .unwrap();
        let resuming = headless(Arc::new(Recorder::default()));
        assert!(resuming.restore_rotation().await);
        assert_eq!(resuming.watch_status()["watching"], true);
        let _ = resuming.watch_stop().await;
    }

    // ── unsaved settings and saving them ─────────────────────────────────────

    #[tokio::test]
    async fn set_effect_changes_the_live_settings_without_writing_them() {
        let sandbox = Sandbox::new("effect");
        let pictures = empty_folder(&sandbox);
        write_settings(
            &sandbox,
            &format!("[display]\neffect = \"normal\"\nfit_mode = \"fill\"\n\n[paths]\nwallpapers_folder = {pictures:?}\n"),
        );
        let session = headless(Arc::new(Recorder::default()));

        // The apply fails (no pictures), but the override is set before it runs and
        // has to survive the failure — the effect hotkeys rely on that.
        let _ = session.set_effect("hdr").await;
        assert_eq!(session.config().unwrap()["display"]["effect"], "hdr");

        let on_disk =
            std::fs::read_to_string(sandbox.dir.join("cfg").join("settings.toml")).unwrap();
        assert!(
            on_disk.contains("effect = \"normal\""),
            "set_effect must not persist; saving stays an explicit action"
        );

        let err = session.set_effect("kaleidoscope").await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Invalid);
    }

    /// The session flags describe what is running *now*, not what the client thinks.
    /// A hotkey can have started the rotation since the window read the config, and
    /// taking the client's copy would switch it back off on the next launch.
    #[tokio::test]
    async fn saving_overwrites_the_session_flags_with_what_is_actually_running() {
        let sandbox = Sandbox::new("save");
        let pictures = empty_folder(&sandbox);
        write_settings(
            &sandbox,
            &format!("[general]\ninterval = 3600\nrotation_active = false\n\n[display]\neffect = \"normal\"\n\n[paths]\nwallpapers_folder = {pictures:?}\n\n[video]\nenabled = true\n"),
        );
        let session = headless(Arc::new(Recorder::default()));

        session.watch_start(Some(3600)).await.unwrap();
        let saved = session
            .save_config(&json!({
                "general": { "interval": 120, "rotation_active": false },
                "display": { "effect": "bw" },
                "_config_path": "ignored",
            }))
            .await
            .unwrap();
        assert_eq!(saved["saved"], true);

        let written = session.config().unwrap();
        assert_eq!(
            written["general"]["interval"], 120,
            "the client's edit lands"
        );
        assert_eq!(
            written["general"]["rotation_active"], true,
            "the live timer wins over the client's stale copy"
        );
        assert_eq!(
            written["video"]["enabled"], false,
            "the player is ours now, so a stale `enabled = true` is corrected rather \
             than preserved — the guess it used to be kept from making is gone"
        );
        let on_disk =
            std::fs::read_to_string(sandbox.dir.join("cfg").join("settings.toml")).unwrap();
        assert!(
            !on_disk.contains("_config_path"),
            "internal keys are never written"
        );

        let _ = session.watch_stop().await;
    }

    /// Saving adopts the client's settings wholesale, so a session override that is
    /// no longer true must not keep shadowing the file.
    #[tokio::test]
    async fn saving_clears_the_unsaved_overrides() {
        let sandbox = Sandbox::new("clear");
        let pictures = empty_folder(&sandbox);
        write_settings(
            &sandbox,
            &format!(
                "[display]\neffect = \"normal\"\n\n[paths]\nwallpapers_folder = {pictures:?}\n"
            ),
        );
        let session = headless(Arc::new(Recorder::default()));

        let _ = session.set_effect("hdr").await;
        assert_eq!(session.config().unwrap()["display"]["effect"], "hdr");

        session
            .save_config(&json!({ "display": { "effect": "vintage" } }))
            .await
            .unwrap();
        assert_eq!(session.config().unwrap()["display"]["effect"], "vintage");
    }

    // ── the history ring, which is pure logic ────────────────────────────────

    fn set(name: &str) -> Vec<String> {
        vec![name.to_string()]
    }

    #[test]
    fn the_cursor_follows_the_newest_entry() {
        let mut history = History::new();
        assert!(
            history.peek_previous().is_none(),
            "nothing to step back to yet"
        );
        history.push(&set("a"));
        assert!(
            history.peek_previous().is_none(),
            "one entry is not a history"
        );
        history.push(&set("b"));
        assert_eq!(history.peek_previous(), Some(set("a")));
    }

    #[test]
    fn stepping_back_then_applying_discards_the_abandoned_future() {
        let mut history = History::new();
        for name in ["a", "b", "c"] {
            history.push(&set(name));
        }
        history.step_back(); // cursor now on "b"
        history.push(&set("d"));

        assert_eq!(history.entries, vec![set("a"), set("b"), set("d")]);
        assert_eq!(history.cursor, 2);
        assert_eq!(history.peek_previous(), Some(set("b")));
    }

    #[test]
    fn the_ring_is_bounded_and_drops_the_oldest() {
        let mut history = History::new();
        for i in 0..HISTORY_LIMIT + 10 {
            history.push(&set(&i.to_string()));
        }
        assert_eq!(history.entries.len(), HISTORY_LIMIT);
        assert_eq!(
            history.entries[0],
            set("10"),
            "the oldest entries went first"
        );
        assert_eq!(history.cursor, HISTORY_LIMIT as i64 - 1);
    }

    #[test]
    fn stepping_back_stops_at_the_beginning() {
        let mut history = History::new();
        history.push(&set("a"));
        history.push(&set("b"));
        history.step_back();
        history.step_back();
        assert_eq!(history.cursor, 0);
        assert!(history.peek_previous().is_none());
    }

    // ── the config overlay ───────────────────────────────────────────────────

    #[test]
    fn an_overlay_merges_one_section_deep_and_skips_internal_keys() {
        let mut base = json!({
            "display": { "fit_mode": "fill", "effect": "normal" },
            "general": { "interval": 300 },
        })
        .as_object()
        .unwrap()
        .clone();

        let draft = json!({
            "display": { "effect": "hdr" },
            "_config_path": "should not survive",
            "paths": { "wallpapers_folder": "C:/pics" },
        })
        .as_object()
        .unwrap()
        .clone();

        overlay(&mut base, &draft);

        assert_eq!(base["display"]["effect"], "hdr");
        assert_eq!(
            base["display"]["fit_mode"], "fill",
            "untouched keys survive"
        );
        assert_eq!(
            base["general"]["interval"], 300,
            "untouched sections survive"
        );
        assert_eq!(base["paths"]["wallpapers_folder"], "C:/pics");
        assert!(!base.contains_key("_config_path"));
    }
}
