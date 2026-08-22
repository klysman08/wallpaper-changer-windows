//! Hold a modifier and scroll to fade the window under the cursor.
//!
//! Ports `scroll_transparency.py`, which drove the same feature through `pynput`.
//! The state it owns is a system-wide low-level mouse hook and the opacity map that
//! hook edits — the same `transparency.json` [`crate::transparency`] reads and writes.
//!
//! ## Three threads, and why
//!
//! A `WH_MOUSE_LL` callback runs on the thread that installed the hook, and Windows
//! gives it **`LowLevelHooksTimeout` milliseconds** — 300 by default — to return. Miss
//! that and the hook is silently removed: no error, no event, the feature simply
//! stops working until the app restarts. So the split is:
//!
//! - the **hook thread** installs the hook and pumps `GetMessageW`. Its callback does
//!   nothing but read the wheel delta, check the modifier, and push an integer down a
//!   channel — no file IO, no process handles, no window calls.
//! - the **worker thread** does everything expensive: resolving the process name
//!   (which opens a process handle), `SetLayeredWindowAttributes`, and the debounced
//!   write to disk.
//! - the **caller's thread** starts and stops them.
//!
//! Python did the process lookup inline on the hook thread behind a 64-entry cache.
//! That is a real hazard rather than a theoretical one: a fast scroll fires dozens of
//! events a second, and a cache miss on a slow-to-open process is exactly the kind of
//! stall that trips the timeout.
//!
//! ## Stopping is a chain of drops
//!
//! `PostThreadMessageW(tid, WM_QUIT)` ends the hook thread's message loop; the hook
//! thread unhooks and drops its end of the channel; the worker sees the channel
//! disconnect, flushes whatever save was pending, and returns. [`ScrollHook::stop`]
//! joins both, so when it returns the hook is really gone and the last edit is really
//! on disk — which is what `stop()` promised in Python.
//!
//! ## The saved map is never overwritten wholesale
//!
//! This is the bug phase 6 had to paper over with a `_reload_opacity_settings` RPC.
//! Python loaded the map once at `start()` and its debounced flush wrote that whole
//! snapshot back, so a fade saved from the window was silently reverted by the next
//! scroll. Here the flush is a **read-modify-write**: re-read the file, lay only the
//! keys this hook actually changed on top, write that. Concurrent edits from the
//! window survive without needing to be told about.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use crate::transparency;
use crate::EventSink;

/// Alpha applied per wheel notch.
pub const STEP: i64 = 5;

/// Never scroll a window past invisible-in-practice. Matches the slider's floor in
/// the UI, so anything reached by scrolling can also be dragged back.
pub const MIN_ALPHA: i64 = 20;
pub const MAX_ALPHA: i64 = 255;

pub const DEFAULT_MODIFIER: &str = "alt";

/// The choices the interface offers, in the order it should list them.
pub const SUPPORTED_MODIFIERS: &[&str] = &["alt", "ctrl", "shift", "win"];

/// How long to wait after the last tick before writing settings to disk.
const SAVE_DEBOUNCE: Duration = Duration::from_millis(600);

/// Window handles to remember process names for. Bounded so a long session that
/// cycles through many windows cannot grow the cache without limit.
const CACHE_LIMIT: usize = 64;

/// Return a supported modifier name, falling back to the default.
///
/// A bad value in `settings.toml` must not stop the engine from starting, so this
/// never fails — `normalize_modifier(Some("ctrl+alt"))` is `"alt"`, not an error.
pub fn normalize_modifier(name: Option<&str>) -> &'static str {
    let key = name.unwrap_or("").trim().to_ascii_lowercase();
    let key = match key.as_str() {
        "control" => "ctrl",
        "windows" | "super" | "meta" => "win",
        other => other,
    };
    SUPPORTED_MODIFIERS
        .iter()
        .find(|m| **m == key)
        .copied()
        .unwrap_or(DEFAULT_MODIFIER)
}

/// Alpha after scrolling *notches* from *current*, clamped to the usable range.
///
/// Scrolling up makes the window more opaque, which is the direction people expect
/// from "more".
pub fn next_alpha(current: i64, notches: i64) -> i64 {
    (current + notches * STEP).clamp(MIN_ALPHA, MAX_ALPHA)
}

/// Whether the hook can be installed at all.
///
/// Always true on Windows. Python asked whether `pynput` could be imported, because a
/// frozen build that missed the hidden import had to degrade rather than refuse to
/// start; there is no optional dependency here, so the only thing that can make this
/// false is not being on Windows.
pub fn is_available() -> bool {
    cfg!(windows)
}

/// The virtual-key codes that count as "held" for a modifier.
///
/// Windows has separate left and right keys; either satisfies the modifier.
fn modifier_keys(modifier: &str) -> &'static [u16] {
    match modifier {
        "ctrl" => &[0x11],      // VK_CONTROL
        "shift" => &[0x10],     // VK_SHIFT
        "win" => &[0x5B, 0x5C], // VK_LWIN, VK_RWIN
        _ => &[0x12],           // VK_MENU
    }
}

// ── the work a tick does ─────────────────────────────────────────────────────

/// The Win32 surface a tick needs, behind a trait so the pipeline can be tested
/// without a real hook, a real window, or a real desktop to fade.
pub trait Desktop: Send + Sync {
    fn foreground_window(&self) -> isize;
    fn process_name(&self, hwnd: isize) -> String;
    fn set_opacity(&self, hwnd: isize, alpha: i64);
}

/// The real thing, straight through to [`crate::transparency`].
pub struct RealDesktop;

impl Desktop for RealDesktop {
    fn foreground_window(&self) -> isize {
        transparency::foreground_window()
    }
    fn process_name(&self, hwnd: isize) -> String {
        transparency::process_name(hwnd)
    }
    fn set_opacity(&self, hwnd: isize, alpha: i64) {
        transparency::set_opacity(hwnd, alpha)
    }
}

/// Everything the worker thread carries between ticks.
struct Worker {
    events: Arc<dyn EventSink>,
    desktop: Arc<dyn Desktop>,
    /// The file as we last saw it. Only ever read from.
    known: Map<String, Value>,
    /// Our own edits since the last write. These win over `known`, and are the only
    /// keys a flush is allowed to change.
    pending: HashMap<String, i64>,
    /// When the debounce started. `None` means nothing is owed to disk.
    dirty_since: Option<Instant>,
    /// hwnd -> process name. Resolving one opens a process handle, and a fast scroll
    /// asks about the same window dozens of times a second.
    processes: HashMap<isize, String>,
}

impl Worker {
    fn new(events: Arc<dyn EventSink>, desktop: Arc<dyn Desktop>) -> Self {
        Self {
            events,
            desktop,
            known: transparency::load_settings(),
            pending: HashMap::new(),
            dirty_since: None,
            processes: HashMap::new(),
        }
    }

    /// The alpha a process is at right now: our unwritten edit, else the file, else
    /// fully opaque. "No saved setting" meaning 255 is what makes the first scroll
    /// down actually fade something.
    fn current_alpha(&self, process: &str) -> i64 {
        if let Some(pending) = self.pending.get(process) {
            return *pending;
        }
        self.known
            .get(process)
            .and_then(Value::as_i64)
            .unwrap_or(MAX_ALPHA)
    }

    fn process_for(&mut self, hwnd: isize) -> String {
        if let Some(cached) = self.processes.get(&hwnd) {
            return cached.clone();
        }
        let name = self.desktop.process_name(hwnd);
        if !name.is_empty() {
            if self.processes.len() > CACHE_LIMIT {
                self.processes.clear();
            }
            self.processes.insert(hwnd, name.clone());
        }
        name
    }

    /// One wheel notch that passed the modifier check.
    fn tick(&mut self, notches: i64) {
        let hwnd = self.desktop.foreground_window();
        if hwnd == 0 {
            return;
        }
        let process = self.process_for(hwnd);
        if process.is_empty() {
            return;
        }

        let current = self.current_alpha(&process);
        let alpha = next_alpha(current, notches);
        // Already at the floor or the ceiling: no window call, no save, no event.
        if alpha == current {
            return;
        }

        self.desktop.set_opacity(hwnd, alpha);
        self.pending.insert(process.clone(), alpha);
        self.dirty_since = Some(Instant::now());
        self.events.emit(
            "transparency_changed",
            json!({ "hwnd": hwnd, "process": process, "alpha": alpha }),
        );
    }

    /// How long until the pending save is due, or `None` if nothing is owed.
    fn due_in(&self) -> Option<Duration> {
        self.dirty_since
            .map(|since| SAVE_DEBOUNCE.saturating_sub(since.elapsed()))
    }

    /// Write our edits, without disturbing anything else in the file.
    ///
    /// Re-reading here rather than writing a snapshot is what lets the window and the
    /// scroll hook both own this file at once.
    fn flush(&mut self) {
        self.dirty_since = None;
        if self.pending.is_empty() {
            return;
        }
        let mut on_disk = transparency::load_settings();
        for (process, alpha) in self.pending.drain() {
            on_disk.insert(process, json!(alpha));
        }
        if let Err(e) = transparency::store_settings(&on_disk) {
            // The fade is already applied; failing to record it costs the user the
            // setting on the next launch, not the setting they can see.
            log_warn(&format!("could not save the scrolled opacity: {e}"));
        }
        self.known = on_disk;
    }
}

/// Drain a channel of wheel notches until it closes, saving on a quiet moment.
///
/// The debounce is Python's: 0.6 s after the *last* tick, not every 0.6 s during a
/// scroll, so a burst writes once. Disconnection is the stop signal, and the flush
/// before returning is what makes [`ScrollHook::stop`] synchronous.
fn run_worker(
    rx: std::sync::mpsc::Receiver<i64>,
    events: Arc<dyn EventSink>,
    desktop: Arc<dyn Desktop>,
) {
    use std::sync::mpsc::RecvTimeoutError;

    let mut worker = Worker::new(events, desktop);
    loop {
        let received = match worker.due_in() {
            Some(wait) => rx.recv_timeout(wait),
            None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        match received {
            Ok(notches) => worker.tick(notches),
            Err(RecvTimeoutError::Timeout) => worker.flush(),
            Err(RecvTimeoutError::Disconnected) => {
                worker.flush();
                return;
            }
        }
    }
}

fn log_warn(message: &str) {
    eprintln!("scroll transparency: {message}");
}

// ── the hook itself ──────────────────────────────────────────────────────────

/// Owns the hook thread and the worker behind it.
///
/// One per session. Starting while already running with the same modifier is a no-op,
/// exactly as in Python — the settings screen re-syncs after every save, and
/// reinstalling a system-wide hook on each one would be gratuitous.
pub struct ScrollHook {
    events: Arc<dyn EventSink>,
    desktop: Arc<dyn Desktop>,
    running: Mutex<Option<Running>>,
}

struct Running {
    modifier: &'static str,
    /// The hook thread, so `PostThreadMessageW` knows where to send `WM_QUIT`.
    thread_id: u32,
    hook: std::thread::JoinHandle<()>,
    worker: std::thread::JoinHandle<()>,
}

impl ScrollHook {
    pub fn new(events: Arc<dyn EventSink>) -> Self {
        Self::with_desktop(events, Arc::new(RealDesktop))
    }

    pub fn with_desktop(events: Arc<dyn EventSink>, desktop: Arc<dyn Desktop>) -> Self {
        Self {
            events,
            desktop,
            running: Mutex::new(None),
        }
    }

    pub fn is_running(&self) -> bool {
        self.running
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }

    pub fn modifier(&self) -> &'static str {
        self.running
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map_or(DEFAULT_MODIFIER, |r| r.modifier)
    }

    /// Install the hook, and report whether it is running afterwards.
    ///
    /// Restarts cleanly if already running with a different modifier.
    pub fn start(&self, modifier: &str) -> bool {
        let modifier = normalize_modifier(Some(modifier));
        {
            let running = self.running.lock().unwrap_or_else(|e| e.into_inner());
            if running.as_ref().is_some_and(|r| r.modifier == modifier) {
                return true;
            }
        }
        self.stop();

        let (notches_tx, notches_rx) = std::sync::mpsc::channel::<i64>();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<u32, String>>();

        let events = Arc::clone(&self.events);
        let desktop = Arc::clone(&self.desktop);
        let worker = std::thread::spawn(move || run_worker(notches_rx, events, desktop));
        let hook = std::thread::spawn(move || platform::run_hook(modifier, notches_tx, ready_tx));

        // Wait for the install to succeed or fail: `start` promises to report what is
        // actually running, and the UI shows that as a badge.
        match ready_rx.recv() {
            Ok(Ok(thread_id)) => {
                *self.running.lock().unwrap_or_else(|e| e.into_inner()) = Some(Running {
                    modifier,
                    thread_id,
                    hook,
                    worker,
                });
                true
            }
            Ok(Err(e)) => {
                log_warn(&format!("could not install the mouse hook: {e}"));
                let _ = hook.join();
                let _ = worker.join();
                false
            }
            // The hook thread died without reporting. Nothing to unhook.
            Err(_) => {
                log_warn("the hook thread stopped before reporting");
                let _ = hook.join();
                let _ = worker.join();
                false
            }
        }
    }

    /// Remove the hook and flush any pending save. Safe when nothing was started.
    pub fn stop(&self) {
        let running = self
            .running
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        let Some(running) = running else { return };

        platform::post_quit(running.thread_id);
        // Joining is the point: the hook is really gone and the last scroll is really
        // saved by the time this returns.
        let _ = running.hook.join();
        let _ = running.worker.join();
    }

    /// `{running, modifier, available}` — the half of the status that is about the
    /// hook rather than about the configuration.
    pub fn status(&self) -> Value {
        json!({
            "running": self.is_running(),
            "modifier": self.modifier(),
            "available": is_available(),
        })
    }
}

impl Drop for ScrollHook {
    fn drop(&mut self) {
        self.stop();
    }
}

// ── Win32 ────────────────────────────────────────────────────────────────────

#[cfg(windows)]
mod platform {
    use std::cell::RefCell;
    use std::sync::mpsc::Sender;

    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetMessageW, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx,
        HC_ACTION, MSG, MSLLHOOKSTRUCT, WHEEL_DELTA, WH_MOUSE_LL, WM_MOUSEWHEEL, WM_QUIT,
    };

    /// What the callback needs, kept in thread-local storage rather than a static.
    ///
    /// A `WH_MOUSE_LL` callback is dispatched on the thread that installed the hook,
    /// so the state it reads can live on that thread — which means the callback takes
    /// no lock at all. A `Mutex` here would be a real risk: blocking inside the
    /// callback is exactly what the 300 ms timeout punishes.
    struct HookState {
        modifier: &'static str,
        notches: Sender<i64>,
    }

    thread_local! {
        static HOOK_STATE: RefCell<Option<HookState>> = const { RefCell::new(None) };
    }

    /// Whether *modifier* is held right now.
    ///
    /// Polled at the moment of the scroll rather than tracked: the wheel event says
    /// nothing about which keys are down, and polling cannot miss a key-up that
    /// happened while the app was unfocused.
    fn modifier_is_down(modifier: &str) -> bool {
        super::modifier_keys(modifier)
            .iter()
            .any(|vk| (unsafe { GetAsyncKeyState(i32::from(*vk)) } as u16 & 0x8000) != 0)
    }

    /// Wheel notches in a `WM_MOUSEWHEEL` hook message, signed, 0 if it is not one.
    ///
    /// The delta is the high word of `mouseData`, as a *signed* short, and comes in
    /// multiples of `WHEEL_DELTA`.
    unsafe fn wheel_notches(lparam: LPARAM) -> i64 {
        if lparam.0 == 0 {
            return 0;
        }
        let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);
        let delta = ((info.mouseData >> 16) as u16) as i16;
        i64::from(delta) / i64::from(WHEEL_DELTA)
    }

    /// **Must not panic.** A Rust panic unwinding into a Win32 callback aborts the
    /// process, so every step here is fallible-by-return rather than by panic:
    /// `try_with` for a torn-down TLS, `try_borrow` against re-entry, and `send` on a
    /// closed channel is an ignored `Err`.
    unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code == HC_ACTION as i32 && wparam.0 as u32 == WM_MOUSEWHEEL {
            let notches = wheel_notches(lparam);
            if notches != 0 {
                let _ = HOOK_STATE.try_with(|cell| {
                    if let Ok(state) = cell.try_borrow() {
                        if let Some(state) = state.as_ref() {
                            if modifier_is_down(state.modifier) {
                                let _ = state.notches.send(notches);
                            }
                        }
                    }
                });
            }
        }
        // Never swallow the event: the wheel must still scroll the window.
        CallNextHookEx(None, code, wparam, lparam)
    }

    /// Install the hook and pump messages until `WM_QUIT`.
    ///
    /// Reports its thread id on success so `stop` can reach it, or the error text on
    /// failure — the caller is blocked on that answer.
    pub fn run_hook(
        modifier: &'static str,
        notches: Sender<i64>,
        ready: Sender<Result<u32, String>>,
    ) {
        HOOK_STATE.with(|cell| {
            *cell.borrow_mut() = Some(HookState { modifier, notches });
        });

        let hook = match unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(hook_proc), None, 0) } {
            Ok(hook) => hook,
            Err(e) => {
                let _ = ready.send(Err(e.message()));
                HOOK_STATE.with(|cell| *cell.borrow_mut() = None);
                return;
            }
        };

        let _ = ready.send(Ok(unsafe { GetCurrentThreadId() }));

        // A low-level hook is only delivered to a thread that pumps messages. There is
        // nothing to dispatch — the hook fires during retrieval — so the loop is bare.
        let mut msg = MSG::default();
        loop {
            let result = unsafe { GetMessageW(&mut msg, None, 0, 0) };
            // 0 is WM_QUIT and -1 is an error; either way there is nothing left to do.
            if result.0 <= 0 {
                break;
            }
        }

        let _ = unsafe { UnhookWindowsHookEx(hook) };
        // Dropping the sender is what tells the worker to flush and stop.
        HOOK_STATE.with(|cell| *cell.borrow_mut() = None);
    }

    pub fn post_quit(thread_id: u32) {
        let _ = unsafe { PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
    }
}

#[cfg(not(windows))]
mod platform {
    use std::sync::mpsc::Sender;

    pub fn run_hook(
        _modifier: &'static str,
        _notches: Sender<i64>,
        ready: Sender<Result<u32, String>>,
    ) {
        let _ = ready.send(Err("mouse hooks are Windows-only".to_string()));
    }

    pub fn post_quit(_thread_id: u32) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Recorder;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A desktop that is always focused on one window owned by one process.
    #[derive(Default)]
    struct FakeDesktop {
        hwnd: isize,
        process: String,
        lookups: AtomicUsize,
        applied: Mutex<Vec<(isize, i64)>>,
    }

    impl FakeDesktop {
        fn on(hwnd: isize, process: &str) -> Arc<Self> {
            Arc::new(Self {
                hwnd,
                process: process.to_string(),
                ..Default::default()
            })
        }
    }

    impl Desktop for FakeDesktop {
        fn foreground_window(&self) -> isize {
            self.hwnd
        }
        fn process_name(&self, _hwnd: isize) -> String {
            self.lookups.fetch_add(1, Ordering::SeqCst);
            self.process.clone()
        }
        fn set_opacity(&self, hwnd: isize, alpha: i64) {
            self.applied
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push((hwnd, alpha));
        }
    }

    fn worker(desktop: Arc<FakeDesktop>, events: Arc<Recorder>) -> Worker {
        Worker::new(events, desktop)
    }

    // ── the pure logic, straight from test_scroll_transparency.py ────────────

    #[test]
    fn normalize_modifier_accepts_the_spellings_users_have() {
        for (raw, expected) in [
            ("alt", "alt"),
            ("ALT", "alt"),
            ("  Ctrl  ", "ctrl"),
            ("control", "ctrl"),
            ("shift", "shift"),
            ("win", "win"),
            ("windows", "win"),
            ("super", "win"),
            ("meta", "win"),
        ] {
            assert_eq!(normalize_modifier(Some(raw)), expected, "for {raw:?}");
        }
    }

    #[test]
    fn normalize_modifier_falls_back_rather_than_failing() {
        // A bad value in settings.toml must not stop the engine from starting.
        for raw in [Some(""), None, Some("nonsense"), Some("ctrl+alt")] {
            assert_eq!(normalize_modifier(raw), DEFAULT_MODIFIER, "for {raw:?}");
        }
    }

    #[test]
    fn every_supported_modifier_normalizes_to_itself() {
        for name in SUPPORTED_MODIFIERS {
            assert_eq!(normalize_modifier(Some(name)), *name);
        }
    }

    #[test]
    fn scrolling_up_makes_the_window_more_opaque() {
        assert_eq!(next_alpha(200, 1), 200 + STEP);
    }

    #[test]
    fn scrolling_down_makes_the_window_more_transparent() {
        assert_eq!(next_alpha(200, -1), 200 - STEP);
    }

    #[test]
    fn alpha_is_clamped_to_the_usable_range() {
        assert_eq!(next_alpha(MAX_ALPHA, 5), MAX_ALPHA);
        assert_eq!(next_alpha(MIN_ALPHA, -5), MIN_ALPHA);
    }

    #[test]
    fn a_window_can_never_be_scrolled_past_invisible() {
        // The floor matches the slider's minimum, so anything reached by scrolling
        // can still be dragged back by hand.
        assert_eq!(next_alpha(0, -100), MIN_ALPHA);
        assert!(
            next_alpha(0, -100) > 0,
            "MIN_ALPHA must leave the window visible"
        );
    }

    #[test]
    fn each_modifier_maps_to_its_virtual_keys() {
        assert_eq!(modifier_keys("alt"), &[0x12]);
        assert_eq!(modifier_keys("ctrl"), &[0x11]);
        assert_eq!(modifier_keys("shift"), &[0x10]);
        // Windows has two of these and either counts.
        assert_eq!(modifier_keys("win"), &[0x5B, 0x5C]);
    }

    // ── what a tick actually does ────────────────────────────────────────────

    #[test]
    fn a_scroll_fades_the_focused_window_and_announces_it() {
        let sandbox = crate::testing::Sandbox::new("scroll-tick");
        let _ = &sandbox;
        let desktop = FakeDesktop::on(42, "code.exe");
        let events = Arc::new(Recorder::default());
        let mut w = worker(Arc::clone(&desktop), Arc::clone(&events));

        w.tick(-1);

        // Unsaved means opaque, so one notch down lands at 255 - STEP.
        let applied = desktop.applied.lock().unwrap();
        assert_eq!(*applied, vec![(42, MAX_ALPHA - STEP)]);
        let announced = events.of("transparency_changed");
        assert_eq!(announced.len(), 1);
        assert_eq!(announced[0]["process"], "code.exe");
        assert_eq!(announced[0]["hwnd"], 42);
        assert_eq!(announced[0]["alpha"], MAX_ALPHA - STEP);
    }

    #[test]
    fn scrolling_at_the_floor_changes_nothing() {
        let sandbox = crate::testing::Sandbox::new("scroll-floor");
        let _ = &sandbox;
        let desktop = FakeDesktop::on(7, "code.exe");
        let events = Arc::new(Recorder::default());
        let mut w = worker(Arc::clone(&desktop), Arc::clone(&events));
        w.pending.insert("code.exe".to_string(), MIN_ALPHA);

        w.tick(-1);

        assert!(
            desktop.applied.lock().unwrap().is_empty(),
            "a scroll that cannot move the alpha must not touch the window"
        );
        assert!(
            events.of("transparency_changed").is_empty(),
            "and must not announce a change that did not happen"
        );
        assert!(w.due_in().is_none(), "nor owe a save");
    }

    #[test]
    fn the_process_name_is_resolved_once_per_window() {
        let sandbox = crate::testing::Sandbox::new("scroll-cache");
        let _ = &sandbox;
        let desktop = FakeDesktop::on(9, "code.exe");
        let events = Arc::new(Recorder::default());
        let mut w = worker(Arc::clone(&desktop), events);

        for _ in 0..5 {
            w.tick(-1);
        }
        assert_eq!(
            desktop.lookups.load(Ordering::SeqCst),
            1,
            "resolving a process opens a handle; a fast scroll must not do it per notch"
        );
    }

    #[test]
    fn a_window_with_no_nameable_process_is_left_alone() {
        let sandbox = crate::testing::Sandbox::new("scroll-nameless");
        let _ = &sandbox;
        let desktop = FakeDesktop::on(3, "");
        let events = Arc::new(Recorder::default());
        let mut w = worker(Arc::clone(&desktop), Arc::clone(&events));

        w.tick(-1);
        assert!(desktop.applied.lock().unwrap().is_empty());
        assert!(events.of("transparency_changed").is_empty());
    }

    // ── the file ─────────────────────────────────────────────────────────────

    #[test]
    fn flushing_writes_our_edits_without_disturbing_the_rest() {
        let sandbox = crate::testing::Sandbox::new("scroll-merge");
        let _ = &sandbox;
        let mut seed = Map::new();
        seed.insert("other.exe".to_string(), json!(120));
        transparency::store_settings(&seed).unwrap();

        let desktop = FakeDesktop::on(11, "code.exe");
        let events = Arc::new(Recorder::default());
        let mut w = worker(desktop, events);
        w.tick(-1);
        w.flush();

        let saved = transparency::load_settings();
        assert_eq!(saved["code.exe"], MAX_ALPHA - STEP);
        assert_eq!(saved["other.exe"], 120, "an untouched process must survive");
    }

    /// The bug phase 6 had to work around: Python held a snapshot from `start()` and
    /// wrote the whole thing back, so a fade saved from the window disappeared on the
    /// next scroll. The flush re-reads, so it cannot happen here.
    #[test]
    fn a_setting_saved_elsewhere_survives_our_flush() {
        let sandbox = crate::testing::Sandbox::new("scroll-clobber");
        let _ = &sandbox;
        let desktop = FakeDesktop::on(11, "code.exe");
        let events = Arc::new(Recorder::default());
        let mut w = worker(desktop, events);

        // The worker starts with an empty view of the file...
        w.tick(-1);

        // ...and only then does the window save something of its own.
        let mut from_the_window = Map::new();
        from_the_window.insert("notepad.exe".to_string(), json!(90));
        transparency::store_settings(&from_the_window).unwrap();

        w.flush();

        let saved = transparency::load_settings();
        assert_eq!(saved["notepad.exe"], 90, "the window's save was clobbered");
        assert_eq!(saved["code.exe"], MAX_ALPHA - STEP);
    }

    #[test]
    fn nothing_is_written_when_nothing_changed() {
        let sandbox = crate::testing::Sandbox::new("scroll-clean");
        let _ = &sandbox;
        let desktop = FakeDesktop::on(11, "code.exe");
        let events = Arc::new(Recorder::default());
        let mut w = worker(desktop, events);

        w.flush();
        assert!(
            !crate::config::transparency_file().exists(),
            "an idle hook must not create the file"
        );
    }

    #[test]
    fn the_save_is_debounced_from_the_last_tick() {
        let sandbox = crate::testing::Sandbox::new("scroll-debounce");
        let _ = &sandbox;
        let desktop = FakeDesktop::on(11, "code.exe");
        let events = Arc::new(Recorder::default());
        let mut w = worker(desktop, events);

        assert!(w.due_in().is_none(), "nothing owed before the first tick");
        w.tick(-1);
        let first = w.due_in().expect("a tick owes a save");
        assert!(first <= SAVE_DEBOUNCE);

        std::thread::sleep(Duration::from_millis(50));
        w.tick(-1);
        let after = w.due_in().expect("still owed");
        assert!(
            after > first.saturating_sub(Duration::from_millis(50)),
            "the second tick must restart the debounce, not inherit the first's deadline"
        );

        w.flush();
        assert!(w.due_in().is_none(), "flushing clears the debt");
    }

    // ── lifecycle ────────────────────────────────────────────────────────────

    #[test]
    fn stopping_is_safe_when_nothing_was_started() {
        let hook = ScrollHook::with_desktop(
            Arc::new(Recorder::default()),
            Arc::new(FakeDesktop::default()),
        );
        hook.stop();
        hook.stop();
        assert!(!hook.is_running());
    }

    /// Installs a **real** system-wide hook and takes it down again.
    ///
    /// Safe to do in a test because the desktop is faked: `foreground_window` is 0, so
    /// even a wheel event arriving mid-test resolves to no window and touches nothing.
    /// This is the only thing that exercises `SetWindowsHookExW`, the `GetMessageW`
    /// loop and `PostThreadMessageW` together — every other test stops short of Win32,
    /// and a hook that silently fails to install looks exactly like one that works.
    #[test]
    #[cfg(windows)]
    fn the_hook_really_installs_and_really_comes_back_out() {
        let hook = ScrollHook::with_desktop(
            Arc::new(Recorder::default()),
            Arc::new(FakeDesktop::default()),
        );

        assert!(hook.start("ctrl"), "the hook did not install");
        assert!(hook.is_running());
        assert_eq!(hook.modifier(), "ctrl");

        // Starting again with the same modifier must not reinstall it.
        assert!(hook.start("ctrl"));
        assert_eq!(hook.modifier(), "ctrl");

        // A different modifier restarts cleanly.
        assert!(hook.start("shift"));
        assert_eq!(hook.modifier(), "shift");

        hook.stop();
        assert!(!hook.is_running(), "the hook thread outlived stop()");
    }

    #[test]
    fn status_reports_what_is_installed_not_what_was_asked_for() {
        let hook = ScrollHook::with_desktop(
            Arc::new(Recorder::default()),
            Arc::new(FakeDesktop::default()),
        );
        let status = hook.status();
        assert_eq!(status["running"], false);
        assert_eq!(status["available"], is_available());
        assert_eq!(status["modifier"], DEFAULT_MODIFIER);
    }
}
