//! Video wallpaper: libmpv rendering into WORKERW-embedded host windows.
//!
//! Ports `video_wallpaper.py` and the ten RPC methods around it. For each monitor a
//! borderless `static` child window is created inside the desktop layer that
//! [`crate::workerw`] found, and one libmpv instance is bound to it through mpv's
//! `wid` option. mpv then runs its own decode and render threads, so there is no
//! playback loop here — only setup and teardown.
//!
//! ## Everything runs on one thread, and that is the fix
//!
//! `DestroyWindow` may only be called by the thread that created the window. Python
//! created the host windows on whichever thread happened to call `video_start` — at
//! start-up that was the `restore-session` daemon thread — and destroyed them from the
//! main thread, where the call quietly failed and `_destroy_window` swallowed it. The
//! visible symptom is host windows stranded on the desktop layer until Explorer
//! restarts. Here a single dedicated thread owns every window and every mpv handle for
//! the life of the process, and the public API is a channel to it.
//!
//! ## Three orderings that are not negotiable
//!
//! 1. **mpv is terminated strictly before its window is destroyed.** mpv renders from
//!    its own threads; destroying the HWND first leaves it painting into freed memory,
//!    which is a native crash, not an exception.
//! 2. **The event pump is stopped before `mpv_terminate_destroy`.** A handle must not
//!    be destroyed while another thread sits in `mpv_wait_event`, so the pump is woken
//!    with `mpv_wakeup` and joined first.
//! 3. **The desktop is repainted after teardown**, or the last video frame stays
//!    frozen on the wallpaper.
//!
//! ## The mpv options are load-bearing
//!
//! [`SAFE_VIDEO_OPTIONS`] is carried over verbatim. Crash reports from the packaged
//! app showed access violations inside `dxgi.dll` during playback — below Python, and
//! now below Rust, so uncatchable either way. The D3D11 presentation path stays
//! because it is the only bundled mpv backend that paints reliably into Explorer's
//! WORKERW children, but D3D11VA hardware decoding and its shared DXGI surfaces are
//! off, and a fixed 8-bit output stops the swap chain renegotiating when a video or
//! display reports HDR. **Do not tidy these up.**
//!
//! Note the spelling: python-mpv turned `gpu_api` into `gpu-api` on the way through.
//! Talking to the C API directly means writing the hyphens.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::{json, Value};

use crate::monitor::Monitor;
use crate::{config, monitor, workerw, CoreError, EventSink};

/// Carried verbatim from `_MPV_SAFE_VIDEO_OPTIONS`. See the module docs before
/// touching any of it.
const SAFE_VIDEO_OPTIONS: &[(&str, &str)] = &[
    ("vo", "gpu"),
    ("gpu-api", "d3d11"),
    ("gpu-context", "d3d11"),
    ("hwdec", "no"),
    ("d3d11-output-format", "rgba8"),
    ("profile", "fast"),
];

/// Every video file in *folder*, sorted. Empty when it is not a directory.
///
/// The scan itself is [`crate::images::list_videos`], which `scan_videos` has used
/// since the stateless-leaves phase — the playlist and the folder listing the settings
/// screen shows must not be able to disagree about order.
pub fn scan_video_folder(folder: &str) -> Vec<PathBuf> {
    crate::images::list_videos(folder)
}

/// Whether the video wallpaper can be offered.
///
/// **Deliberately does not load the library.** `get_capabilities` is called by the UI
/// on every launch, and `video_status` on every poll; resolving the DLL to answer them
/// mapped **112 MB** into the process at start-up whether or not the user ever played
/// a video. Under Python that cost at least landed in the sidecar — in-process it is
/// the app's own footprint.
///
/// So this asks the question the callers actually mean: is there a libmpv to load? If
/// one has already been loaded, that answer is authoritative; otherwise the file's
/// presence stands in for it. The gap between the two is a DLL that exists but will not
/// load — corrupt, or the wrong architecture — where the UI offers video and
/// `video_start` then reports `no_mpv`. That is a worse error message in a rare case,
/// traded against a large allocation in every ordinary one.
pub fn has_mpv() -> bool {
    if let Some(loaded) = LIBRARY.get() {
        return loaded.is_some();
    }
    library_candidates()
        .iter()
        .any(|dir| LIBRARY_NAMES.iter().any(|name| dir.join(name).is_file()))
}

/// Load libmpv for real, which only playback needs.
pub fn library_is_loadable() -> bool {
    library().is_some()
}

// ── libmpv, loaded at runtime ────────────────────────────────────────────────

#[allow(non_camel_case_types)]
type mpv_handle = *mut std::ffi::c_void;

const MPV_FORMAT_FLAG: i32 = 3;
const MPV_FORMAT_INT64: i32 = 4;

const MPV_EVENT_NONE: i32 = 0;
const MPV_EVENT_SHUTDOWN: i32 = 1;
const MPV_EVENT_LOG_MESSAGE: i32 = 2;

#[repr(C)]
struct MpvEvent {
    event_id: i32,
    error: i32,
    reply_userdata: u64,
    data: *mut std::ffi::c_void,
}

#[repr(C)]
struct MpvEventLogMessage {
    prefix: *const std::ffi::c_char,
    level: *const std::ffi::c_char,
    text: *const std::ffi::c_char,
    log_level: i32,
}

/// The ten libmpv entry points this needs, resolved by name at runtime.
///
/// Hand-rolled rather than through `libmpv`/`libmpv2`: both link at build time, and
/// what is needed here is *runtime* loading of a DLL from a path chosen at runtime,
/// degrading gracefully when it is absent. That is the whole reason `_prepare_libmpv`
/// existed to fake it with `%PATH%`.
struct MpvLib {
    _library: libloading::Library,
    create: unsafe extern "C" fn() -> mpv_handle,
    initialize: unsafe extern "C" fn(mpv_handle) -> i32,
    terminate_destroy: unsafe extern "C" fn(mpv_handle),
    set_option_string:
        unsafe extern "C" fn(mpv_handle, *const std::ffi::c_char, *const std::ffi::c_char) -> i32,
    set_property: unsafe extern "C" fn(
        mpv_handle,
        *const std::ffi::c_char,
        i32,
        *mut std::ffi::c_void,
    ) -> i32,
    get_property: unsafe extern "C" fn(
        mpv_handle,
        *const std::ffi::c_char,
        i32,
        *mut std::ffi::c_void,
    ) -> i32,
    command: unsafe extern "C" fn(mpv_handle, *mut *const std::ffi::c_char) -> i32,
    error_string: unsafe extern "C" fn(i32) -> *const std::ffi::c_char,
    request_log_messages: unsafe extern "C" fn(mpv_handle, *const std::ffi::c_char) -> i32,
    wait_event: unsafe extern "C" fn(mpv_handle, f64) -> *mut MpvEvent,
    wakeup: unsafe extern "C" fn(mpv_handle),
}

// The handles are only ever touched from the video thread and each instance's own
// event pump; the library itself is immutable once loaded.
unsafe impl Send for MpvLib {}
unsafe impl Sync for MpvLib {}

/// Filenames a vendored libmpv might use, newest first.
const LIBRARY_NAMES: &[&str] = &["libmpv-2.dll", "mpv-2.dll", "mpv-1.dll"];

static LIBRARY: OnceLock<Option<Arc<MpvLib>>> = OnceLock::new();

/// An extra directory to look in first, set by the shell.
///
/// The core cannot depend on `tauri`, so it cannot ask where a bundled resource
/// landed. In a packaged build that is the only place `libmpv-2.dll` exists, so the
/// shell resolves the resource directory and passes it down before anything can play.
/// Must be set before anything loads the library, which is resolved once and cached.
static SEARCH_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Point the loader at a directory before anything asks whether libmpv exists.
pub fn set_search_dir(dir: PathBuf) {
    let _ = SEARCH_DIR.set(dir);
}

fn library() -> Option<Arc<MpvLib>> {
    LIBRARY.get_or_init(load_library).clone()
}

/// Where `libmpv-2.dll` might be, in the order Python looked.
///
/// `MPV_DLL_DIR` first so a developer can point at a different build, then the
/// vendored `libmpv/` folder, then the project root itself — which is where a frozen
/// build put it beside the executable.
fn library_candidates() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(dir) = SEARCH_DIR.get() {
        dirs.push(dir.clone());
    }
    if let Ok(dir) = std::env::var("MPV_DLL_DIR") {
        if !dir.is_empty() {
            dirs.push(PathBuf::from(dir));
        }
    }
    let root = config::project_root();
    dirs.push(root.join("libmpv"));
    dirs.push(root.clone());
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            dirs.push(parent.join("libmpv"));
            dirs.push(parent.to_path_buf());
        }
    }
    dirs
}

fn load_library() -> Option<Arc<MpvLib>> {
    for dir in library_candidates() {
        for name in LIBRARY_NAMES {
            let path = dir.join(name);
            if !path.is_file() {
                continue;
            }
            match unsafe { open_library(&path) } {
                Ok(lib) => {
                    log::info!("libmpv loaded from {}", path.display());
                    return Some(Arc::new(lib));
                }
                Err(e) => log::warn!("could not load {}: {e}", path.display()),
            }
        }
    }
    log::info!("libmpv not found; the video wallpaper stays unavailable");
    None
}

/// Load the DLL and resolve every symbol, failing as a whole if any is missing.
///
/// On Windows this uses `LOAD_WITH_ALTERED_SEARCH_PATH`, so libmpv's own dependent
/// DLLs are looked for beside it rather than on `%PATH%`. That is the clean version of
/// what `_prepare_libmpv` was doing by prepending to `%PATH%` and calling
/// `os.add_dll_directory`.
unsafe fn open_library(path: &Path) -> Result<MpvLib, String> {
    #[cfg(windows)]
    let library = {
        use libloading::os::windows::{Library, LOAD_WITH_ALTERED_SEARCH_PATH};
        libloading::Library::from(
            Library::load_with_flags(path, LOAD_WITH_ALTERED_SEARCH_PATH)
                .map_err(|e| e.to_string())?,
        )
    };
    #[cfg(not(windows))]
    let library = libloading::Library::new(path).map_err(|e| e.to_string())?;

    macro_rules! symbol {
        ($name:literal) => {
            *library
                .get(concat!($name, "\0").as_bytes())
                .map_err(|e| format!("{}: {e}", $name))?
        };
    }

    Ok(MpvLib {
        create: symbol!("mpv_create"),
        initialize: symbol!("mpv_initialize"),
        terminate_destroy: symbol!("mpv_terminate_destroy"),
        set_option_string: symbol!("mpv_set_option_string"),
        set_property: symbol!("mpv_set_property"),
        get_property: symbol!("mpv_get_property"),
        command: symbol!("mpv_command"),
        error_string: symbol!("mpv_error_string"),
        request_log_messages: symbol!("mpv_request_log_messages"),
        wait_event: symbol!("mpv_wait_event"),
        wakeup: symbol!("mpv_wakeup"),
        _library: library,
    })
}

fn cstring(value: &str) -> std::ffi::CString {
    // A NUL inside a path or option value is not representable; truncating at it is
    // better than refusing, and mpv will report the resulting path as not found.
    std::ffi::CString::new(value).unwrap_or_else(|e| {
        let bytes = e.into_vec();
        let upto = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
        std::ffi::CString::new(&bytes[..upto]).unwrap_or_default()
    })
}

impl MpvLib {
    fn error_text(&self, code: i32) -> String {
        let text = unsafe { (self.error_string)(code) };
        if text.is_null() {
            return format!("mpv error {code}");
        }
        unsafe { std::ffi::CStr::from_ptr(text) }
            .to_string_lossy()
            .into_owned()
    }

    fn check(&self, code: i32, what: &str) -> Result<(), String> {
        if code < 0 {
            Err(format!("{what}: {}", self.error_text(code)))
        } else {
            Ok(())
        }
    }
}

// ── one mpv instance bound to one host window ────────────────────────────────

/// An mpv handle, the window it renders into, and the thread draining its events.
struct Instance {
    lib: Arc<MpvLib>,
    handle: mpv_handle,
    hwnd: isize,
    /// Set to end the event pump. Read by the pump between waits.
    pump_stop: Arc<Mutex<bool>>,
    pump: Option<std::thread::JoinHandle<()>>,
}

// The handle crosses to the pump thread, which is the only other toucher, and the
// ordering that makes that safe is enforced in `terminate`.
unsafe impl Send for Instance {}

impl Instance {
    /// Create an mpv bound to *hwnd* and load the whole playlist into it.
    fn create(
        lib: &Arc<MpvLib>,
        hwnd: isize,
        videos: &[PathBuf],
        loop_playlist: bool,
        audio: bool,
    ) -> Result<Self, String> {
        let handle = unsafe { (lib.create)() };
        if handle.is_null() {
            return Err("mpv_create returned nothing".to_string());
        }

        // From here on any failure has to destroy the handle, so the work is done in a
        // closure and the handle is cleaned up on the way out.
        let configure = || -> Result<(), String> {
            let mut options: Vec<(String, String)> = SAFE_VIDEO_OPTIONS
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            // `wid` is a *string* option and must be set before mpv_initialize.
            options.push(("wid".to_string(), hwnd.to_string()));
            options.push(("keepaspect".to_string(), "yes".to_string()));
            options.push((
                "loop-playlist".to_string(),
                if loop_playlist { "inf" } else { "no" }.to_string(),
            ));
            options.push((
                "mute".to_string(),
                if audio { "no" } else { "yes" }.to_string(),
            ));
            options.push(("osc".to_string(), "no".to_string()));
            options.push(("input-default-bindings".to_string(), "no".to_string()));
            options.push(("input-vo-keyboard".to_string(), "no".to_string()));

            for (name, value) in &options {
                let code = unsafe {
                    (lib.set_option_string)(handle, cstring(name).as_ptr(), cstring(value).as_ptr())
                };
                // An option mpv does not know is not worth refusing to play over —
                // except `wid`, without which it would open its own window on top of
                // everything.
                if code < 0 {
                    if name == "wid" {
                        return Err(lib.error_text(code));
                    }
                    log::warn!("mpv rejected {name}={value}: {}", lib.error_text(code));
                }
            }

            let level = cstring("warn");
            unsafe { (lib.request_log_messages)(handle, level.as_ptr()) };

            lib.check(unsafe { (lib.initialize)(handle) }, "mpv_initialize")?;

            for (index, video) in videos.iter().enumerate() {
                let mode = if index == 0 { "replace" } else { "append" };
                let path = cstring(&video.to_string_lossy());
                let verb = cstring("loadfile");
                let mode = cstring(mode);
                let mut args = [
                    verb.as_ptr(),
                    path.as_ptr(),
                    mode.as_ptr(),
                    std::ptr::null(),
                ];
                lib.check(
                    unsafe { (lib.command)(handle, args.as_mut_ptr()) },
                    "loadfile",
                )?;
            }
            Ok(())
        };

        if let Err(e) = configure() {
            // Already bound to the window: terminate before anyone destroys it.
            unsafe { (lib.terminate_destroy)(handle) };
            return Err(e);
        }

        let mut instance = Self {
            lib: Arc::clone(lib),
            handle,
            hwnd,
            pump_stop: Arc::new(Mutex::new(false)),
            pump: None,
        };
        instance.start_pump();
        Ok(instance)
    }

    /// Drain mpv's event queue, forwarding its warnings to the application log.
    ///
    /// Not optional bookkeeping: an unread queue grows, and this is also where
    /// `_handle_mpv_log` went.
    fn start_pump(&mut self) {
        let lib = Arc::clone(&self.lib);
        let handle = self.handle as usize;
        let stop = Arc::clone(&self.pump_stop);
        self.pump = Some(std::thread::spawn(move || {
            let handle = handle as mpv_handle;
            loop {
                if *stop.lock().unwrap_or_else(|e| e.into_inner()) {
                    return;
                }
                let event = unsafe { (lib.wait_event)(handle, 1.0) };
                if event.is_null() {
                    continue;
                }
                let event = unsafe { &*event };
                match event.event_id {
                    MPV_EVENT_NONE => continue,
                    MPV_EVENT_SHUTDOWN => return,
                    MPV_EVENT_LOG_MESSAGE => {
                        if !event.data.is_null() {
                            let message = unsafe { &*(event.data as *const MpvEventLogMessage) };
                            let text = unsafe { cstr(message.text) };
                            let text = text.trim();
                            if !text.is_empty() {
                                log::warn!(
                                    "mpv [{}] {}: {text}",
                                    unsafe { cstr(message.level) },
                                    unsafe { cstr(message.prefix) },
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }
        }));
    }

    fn set_flag(&self, name: &str, value: bool) {
        let mut flag: i32 = i32::from(value);
        let code = unsafe {
            (self.lib.set_property)(
                self.handle,
                cstring(name).as_ptr(),
                MPV_FORMAT_FLAG,
                &mut flag as *mut i32 as *mut _,
            )
        };
        if code < 0 {
            log::debug!("mpv could not set {name}: {}", self.lib.error_text(code));
        }
    }

    fn playlist_pos(&self) -> Option<i64> {
        let mut position: i64 = 0;
        let code = unsafe {
            (self.lib.get_property)(
                self.handle,
                cstring("playlist-pos").as_ptr(),
                MPV_FORMAT_INT64,
                &mut position as *mut i64 as *mut _,
            )
        };
        (code >= 0 && position >= 0).then_some(position)
    }

    fn set_playlist_pos(&self, position: i64) {
        let mut position = position;
        let code = unsafe {
            (self.lib.set_property)(
                self.handle,
                cstring("playlist-pos").as_ptr(),
                MPV_FORMAT_INT64,
                &mut position as *mut i64 as *mut _,
            )
        };
        if code < 0 {
            log::debug!(
                "mpv could not seek the playlist: {}",
                self.lib.error_text(code)
            );
        }
    }

    /// Stop the pump, then mpv, then the window — in that order, always.
    fn terminate(mut self) {
        *self.pump_stop.lock().unwrap_or_else(|e| e.into_inner()) = true;
        // The pump is parked inside `mpv_wait_event`; destroying the handle underneath
        // it is a use-after-free, so wake it and wait for it to leave.
        unsafe { (self.lib.wakeup)(self.handle) };
        if let Some(pump) = self.pump.take() {
            let _ = pump.join();
        }
        unsafe { (self.lib.terminate_destroy)(self.handle) };
        destroy_window(self.hwnd);
    }
}

unsafe fn cstr(pointer: *const std::ffi::c_char) -> String {
    if pointer.is_null() {
        return String::new();
    }
    std::ffi::CStr::from_ptr(pointer)
        .to_string_lossy()
        .into_owned()
}

// ── host windows ─────────────────────────────────────────────────────────────

#[cfg(windows)]
fn create_host_window(parent: isize, monitor: &Monitor, origin: (i32, i32)) -> Option<isize> {
    use windows::core::w;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, WINDOW_EX_STYLE, WS_CHILD, WS_EX_NOACTIVATE, WS_VISIBLE,
    };

    // Coordinates are relative to the parent's client area, so the monitor's screen
    // position is offset by the parent's top-left. On a layout with a screen above the
    // primary that origin is negative, and dropping it puts every video off-screen.
    let x = monitor.x - origin.0;
    let y = monitor.y - origin.1;

    // A predefined `static` control, so there is no window class to register and no
    // WndProc to write — mpv paints the whole client area anyway.
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(WS_EX_NOACTIVATE.0),
            w!("static"),
            None,
            WS_CHILD | WS_VISIBLE,
            x,
            y,
            monitor.width,
            monitor.height,
            Some(HWND(parent as *mut _)),
            None,
            None,
            None,
        )
    };
    match hwnd {
        Ok(hwnd) if !hwnd.is_invalid() => Some(hwnd.0 as isize),
        _ => {
            log::warn!(
                "CreateWindowExW failed for monitor {}: {}",
                monitor.index,
                windows::core::Error::from_win32().message()
            );
            None
        }
    }
}

#[cfg(not(windows))]
fn create_host_window(_parent: isize, _monitor: &Monitor, _origin: (i32, i32)) -> Option<isize> {
    None
}

/// Destroy a host window. Safe with a stale handle.
///
/// Explorer can recreate the WORKERW layer — a display change or a shell restart —
/// which destroys our children and leaves the handles behind. No `IsWindow` guard:
/// `DestroyWindow` reports a bad handle rather than misbehaving, and a guard would
/// only add a window where the handle could go stale between check and use.
#[cfg(windows)]
fn destroy_window(hwnd: isize) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::DestroyWindow;
    if hwnd != 0 {
        let _ = unsafe { DestroyWindow(HWND(hwnd as *mut _)) };
    }
}

#[cfg(not(windows))]
fn destroy_window(_hwnd: isize) {}

// ── the video thread ─────────────────────────────────────────────────────────

/// What a caller asks the video thread to do. Every variant carries its reply channel.
enum Command {
    Start {
        videos: Vec<PathBuf>,
        loop_playlist: bool,
        sound: bool,
        monitors: Vec<Monitor>,
        reply: Sender<Result<(), String>>,
    },
    Stop {
        reply: Sender<()>,
    },
    Step {
        direction: i64,
        reply: Sender<String>,
    },
    SetSound {
        enabled: bool,
        reply: Sender<()>,
    },
    Status {
        reply: Sender<(bool, String)>,
    },
}

/// The video thread's own state. No locks — it is the only thread that sees this.
#[derive(Default)]
struct Playing {
    instances: Vec<Instance>,
    videos: Vec<PathBuf>,
    parent: Option<isize>,
}

impl Playing {
    fn running(&self) -> bool {
        !self.instances.is_empty()
    }

    fn current_name(&self) -> String {
        if self.videos.is_empty() {
            return String::new();
        }
        let position = self
            .instances
            .first()
            .and_then(Instance::playlist_pos)
            .filter(|p| (*p as usize) < self.videos.len())
            .unwrap_or(0) as usize;
        file_name(&self.videos[position])
    }

    fn stop(&mut self) {
        // mpv first, every time: `Instance::terminate` destroys the window only after
        // its player is gone.
        for instance in self.instances.drain(..) {
            instance.terminate();
        }
        if let Some(parent) = self.parent.take() {
            workerw::refresh(parent);
        }
    }

    fn start(
        &mut self,
        lib: &Arc<MpvLib>,
        videos: Vec<PathBuf>,
        loop_playlist: bool,
        sound: bool,
        monitors: &[Monitor],
    ) -> Result<(), String> {
        self.stop();

        // `start()` in Python refuses both of these before it touches the desktop, and
        // the order matters: an empty playlist would otherwise create host windows and
        // an mpv per screen with nothing to play, which looks like a black wallpaper
        // rather than an error.
        if videos.is_empty() {
            return Err("No videos configured.".to_string());
        }
        if monitors.is_empty() {
            return Err("No monitors detected.".to_string());
        }

        let parent = workerw::desktop_parent().ok_or_else(|| {
            "Could not find the desktop layer (WORKERW/Progman). Is Explorer running?".to_string()
        })?;
        self.parent = Some(parent);
        self.videos = videos;
        let origin = workerw::window_origin(parent);

        // Audio on the first instance only, or every monitor plays the same soundtrack
        // a few milliseconds apart and it sounds like an echo.
        let mut first = true;
        for monitor in monitors {
            let Some(hwnd) = create_host_window(parent, monitor, origin) else {
                continue;
            };
            match Instance::create(lib, hwnd, &self.videos, loop_playlist, sound && first) {
                Ok(instance) => {
                    self.instances.push(instance);
                    first = false;
                }
                Err(e) => {
                    log::warn!("mpv would not start on monitor {}: {e}", monitor.index);
                    destroy_window(hwnd);
                }
            }
        }

        if self.instances.is_empty() {
            // Never leave a half-built desktop layer behind.
            self.stop();
            return Err("No video host windows could be created on the desktop layer.".to_string());
        }
        Ok(())
    }

    /// Move every monitor to the same playlist index, wrapping.
    ///
    /// Driven through `playlist-pos` rather than mpv's own next/prev so the screens
    /// re-sync onto one index and the resulting name is known exactly. Wrapping works
    /// whether or not looping is on, which is what an explicit next/previous should do.
    fn step(&mut self, direction: i64) -> String {
        if self.videos.is_empty() {
            return String::new();
        }
        let count = self.videos.len() as i64;
        if self.instances.is_empty() {
            return file_name(&self.videos[0]);
        }
        let current = self.instances[0].playlist_pos().unwrap_or(0);
        let target = (current + direction).rem_euclid(count);
        for instance in &self.instances {
            instance.set_playlist_pos(target);
        }
        file_name(&self.videos[target as usize])
    }

    fn set_sound(&mut self, enabled: bool) {
        for (index, instance) in self.instances.iter().enumerate() {
            instance.set_flag("mute", !(enabled && index == 0));
        }
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The handle the rest of the engine holds. Everything it does is a message.
pub struct VideoPlayer {
    events: Arc<dyn EventSink>,
    commands: Mutex<Option<Sender<Command>>>,
}

impl VideoPlayer {
    pub fn new(events: Arc<dyn EventSink>) -> Self {
        Self {
            events,
            commands: Mutex::new(None),
        }
    }

    /// The channel to the video thread, starting it on first use.
    ///
    /// Lazy because the thread exists to own window handles, and an app that never
    /// plays a video should never create it. Once started it lives for the process:
    /// the windows it owns can only be destroyed by it.
    fn thread(&self) -> Sender<Command> {
        let mut commands = self.commands.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(sender) = commands.as_ref() {
            return sender.clone();
        }
        let (tx, rx) = channel::<Command>();
        let events = Arc::clone(&self.events);
        std::thread::Builder::new()
            .name("video".to_string())
            .spawn(move || run_video_thread(rx, events))
            .expect("could not start the video thread");
        *commands = Some(tx.clone());
        tx
    }

    fn send<T>(&self, make: impl FnOnce(Sender<T>) -> Command) -> Option<T> {
        let (tx, rx) = channel::<T>();
        self.thread().send(make(tx)).ok()?;
        rx.recv().ok()
    }

    pub fn start(
        &self,
        videos: Vec<PathBuf>,
        loop_playlist: bool,
        sound: bool,
        monitors: Vec<Monitor>,
    ) -> Result<(), CoreError> {
        let outcome = self.send(|reply| Command::Start {
            videos,
            loop_playlist,
            sound,
            monitors,
            reply,
        });
        match outcome {
            Some(Ok(())) => Ok(()),
            Some(Err(e)) => Err(CoreError::error(e)),
            None => Err(CoreError::internal("The video thread is not answering.")),
        }
    }

    pub fn stop(&self) {
        // Only if the thread was ever started — otherwise there is nothing playing and
        // asking would spawn a thread to say so.
        let started = self
            .commands
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some();
        if started {
            self.send(|reply| Command::Stop { reply });
        }
    }

    pub fn step(&self, direction: i64) -> String {
        self.send(|reply| Command::Step { direction, reply })
            .unwrap_or_default()
    }

    pub fn set_sound(&self, enabled: bool) {
        self.send(|reply| Command::SetSound { enabled, reply });
    }

    /// `(running, current file name)`.
    pub fn status(&self) -> (bool, String) {
        let started = self
            .commands
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some();
        if !started {
            return (false, String::new());
        }
        self.send(|reply| Command::Status { reply })
            .unwrap_or((false, String::new()))
    }

    pub fn is_running(&self) -> bool {
        self.status().0
    }
}

fn run_video_thread(commands: std::sync::mpsc::Receiver<Command>, events: Arc<dyn EventSink>) {
    let mut playing = Playing::default();
    for command in commands {
        match command {
            Command::Start {
                videos,
                loop_playlist,
                sound,
                monitors,
                reply,
            } => {
                let first = videos.first().map(|p| file_name(p));
                let outcome = match library() {
                    Some(lib) => playing.start(&lib, videos, loop_playlist, sound, &monitors),
                    None => Err("libmpv is not available.".to_string()),
                };
                if outcome.is_ok() {
                    if let Some(name) = first {
                        events.emit("video_status", json!({ "current": name }));
                    }
                }
                let _ = reply.send(outcome);
            }
            Command::Stop { reply } => {
                playing.stop();
                let _ = reply.send(());
            }
            Command::Step { direction, reply } => {
                let name = playing.step(direction);
                if !name.is_empty() {
                    events.emit("video_status", json!({ "current": name.clone() }));
                }
                let _ = reply.send(name);
            }
            Command::SetSound { enabled, reply } => {
                playing.set_sound(enabled);
                let _ = reply.send(());
            }
            Command::Status { reply } => {
                let _ = reply.send((playing.running(), playing.current_name()));
            }
        }
    }
    // The sender was dropped, which only happens as the process goes away. Take the
    // host windows with us rather than stranding them on the desktop layer.
    playing.stop();
}

// ── RPC results ──────────────────────────────────────────────────────────────

/// The videos a configuration points at, or the error the caller should see.
pub fn videos_for(config: &Value) -> Result<Vec<PathBuf>, CoreError> {
    let folder = config
        .pointer("/video/folder")
        .and_then(Value::as_str)
        .unwrap_or("");
    let videos = scan_video_folder(folder);
    if videos.is_empty() {
        return Err(CoreError::not_found(
            "No videos found in the configured folder.",
        ));
    }
    Ok(videos)
}

pub fn status_result(player: &VideoPlayer) -> Value {
    let (running, current) = player.status();
    json!({ "running": running, "current": current, "has_mpv": has_mpv() })
}

/// Everything `video_start` needs from the configuration, validated in Python's order.
///
/// The order is the contract: no mpv before no videos before no monitors, because that
/// is which error the user sees when more than one is true.
pub fn start_inputs(config: &Value) -> Result<(Vec<PathBuf>, bool, bool, Vec<Monitor>), CoreError> {
    // The real load, not the cheap [`has_mpv`] probe — this is the moment playback is
    // actually being asked for, so it is the right place to pay for it, and a DLL that
    // is present but unloadable should still report `no_mpv` rather than a bare error.
    if !library_is_loadable() {
        return Err(CoreError::no_mpv("libmpv is not available."));
    }
    let videos = videos_for(config)?;
    let monitors = monitor::get_monitors()?;
    if monitors.is_empty() {
        return Err(CoreError::no_monitors("No monitors detected."));
    }
    let loop_playlist = config
        .pointer("/video/loop")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let sound = config
        .pointer("/video/sound")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Ok((videos, loop_playlist, sound, monitors))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("wc-video-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn scanning_finds_video_files_sorted_and_ignores_the_rest() {
        let dir = folder("scan");
        for name in ["b.mp4", "a.MKV", "c.txt", "d.webm"] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        std::fs::create_dir(dir.join("e.mp4")).unwrap();

        let found: Vec<String> = scan_video_folder(&dir.to_string_lossy())
            .iter()
            .map(|p| file_name(p))
            .collect();

        // Case-insensitive on the extension, sorted, directories are not files.
        assert_eq!(found, vec!["a.MKV", "b.mp4", "d.webm"]);
    }

    #[test]
    fn scanning_a_missing_folder_is_empty_rather_than_an_error() {
        assert!(scan_video_folder("C:/no/such/folder/anywhere").is_empty());
        assert!(scan_video_folder("").is_empty());
    }

    #[test]
    fn a_folder_with_no_videos_is_not_found_rather_than_empty() {
        let dir = folder("empty");
        std::fs::write(dir.join("readme.txt"), b"x").unwrap();
        let config = json!({ "video": { "folder": dir.to_string_lossy() } });
        let error = videos_for(&config).unwrap_err();
        assert_eq!(error.kind(), crate::ErrorKind::NotFound);
    }

    #[test]
    fn the_playlist_wraps_in_both_directions() {
        // `step` is arithmetic over the playlist; the wrap is the part worth pinning,
        // and Python's `%` on a negative left operand is Rust's `rem_euclid`, not `%`.
        let mut playing = Playing {
            videos: vec![
                PathBuf::from("a.mp4"),
                PathBuf::from("b.mp4"),
                PathBuf::from("c.mp4"),
            ],
            ..Default::default()
        };
        // With no instances it reports the first file rather than failing.
        assert_eq!(playing.step(1), "a.mp4");

        let count = playing.videos.len() as i64;
        assert_eq!(
            (0 + -1i64).rem_euclid(count),
            2,
            "stepping back from 0 wraps to the end"
        );
        assert_eq!(
            (2 + 1i64).rem_euclid(count),
            0,
            "stepping past the end wraps to 0"
        );
    }

    #[test]
    fn a_status_call_never_starts_the_video_thread() {
        // `get_capabilities` and `video_status` are asked constantly, including before
        // anything has played. Spawning a thread that owns desktop windows to answer
        // "nothing is playing" would be a poor trade.
        let player = VideoPlayer::new(Arc::new(crate::NullSink));
        assert_eq!(player.status(), (false, String::new()));
        player.stop();
        assert!(player.commands.lock().unwrap().is_none());
    }

    /// Asking whether video is available must not map 112 MB into the process.
    ///
    /// `get_capabilities` runs on every launch and `video_status` on every poll, so a
    /// `has_mpv` that resolves the library makes an app that never plays a video pay
    /// for one anyway. Written to tolerate another test having loaded it first: what is
    /// asserted is that this call does not *cause* the load.
    #[test]
    fn asking_whether_video_is_available_does_not_load_it() {
        let loaded_before = LIBRARY.get().is_some();
        let _ = has_mpv();
        assert_eq!(
            LIBRARY.get().is_some(),
            loaded_before,
            "has_mpv resolved the library; it is supposed to only look for the file"
        );
    }

    #[test]
    fn the_safe_options_are_still_the_ones_that_dodge_dxgi() {
        // These exist because of native access violations in dxgi.dll, and the spelling
        // changed when python-mpv stopped translating underscores. A rename here is a
        // silent regression, so it is pinned.
        let options: Vec<&str> = SAFE_VIDEO_OPTIONS.iter().map(|(k, _)| *k).collect();
        assert_eq!(
            options,
            vec![
                "vo",
                "gpu-api",
                "gpu-context",
                "hwdec",
                "d3d11-output-format",
                "profile"
            ]
        );
        assert!(
            !options.iter().any(|o| o.contains('_')),
            "the C API takes hyphens; python-mpv was doing that translation"
        );
    }
}
