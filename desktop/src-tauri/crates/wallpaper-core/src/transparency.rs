//! Making other applications' windows see-through.
//!
//! Ports `transparency.py` and the seven RPC methods around it. Nothing here touches
//! the wallpaper — it is a separate feature that happens to live in the same engine,
//! and its state is one file (`transparency.json`) keyed by **process name**.
//!
//! Keying by process rather than by window handle is the whole design: a handle dies
//! with the window, so a setting keyed by one would be forgotten the moment the user
//! closed the app they had just faded. Keyed by `code.exe`, it survives a restart and
//! is reapplied to whatever window that process opens next.
//!
//! ## Two things about alpha 255
//!
//! `SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA)` is *not* how a window is
//! returned to normal. Fully opaque is expressed by **removing `WS_EX_LAYERED`**, so
//! the window goes back to its ordinary rendering path rather than staying on the
//! layered one at full strength. `set_opacity` reproduces that exactly.
//!
//! And 255 is the default when a process has no saved setting, which is what makes
//! [`toggle`] work as a toggle: unsaved means opaque, so the first press fades.
//!
//! ## One deliberate improvement over the Python
//!
//! `transparency.py` binds `SetWindowLongW`/`GetWindowLongW` with `c_long`, which is
//! the 32-bit entry point. It happens to work because `GWL_EXSTYLE` fits in 32 bits,
//! but it is the wrong function on a 64-bit process. This uses
//! `SetWindowLongPtrW`/`GetWindowLongPtrW`.
//!
//! The process lookup is also better: `QueryFullProcessImageNameW` needs only
//! `PROCESS_QUERY_LIMITED_INFORMATION`, where pywin32's `GetModuleFileNameEx` path
//! demanded `PROCESS_QUERY_INFORMATION | PROCESS_VM_READ` — rights an elevated or
//! protected process will refuse. Windows that used to vanish from the list because
//! the query failed now appear.

use serde_json::{json, Map, Value};

use crate::{config, CoreError};

/// What the transparency hotkey fades to, and what it comes back to.
pub const HALF_OPACITY: i64 = 128;
pub const FULLY_OPAQUE: i64 = 255;

/// Titles that are never worth offering: the desktop itself, the IME host, and two
/// shell surfaces that are always present and never what the user meant.
const IGNORED_TITLES: &[&str] = &[
    "Program Manager",
    "Windows Input Experience",
    "Settings",
    "MSCTFIME UI",
];

/// A window the user could plausibly want to fade.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibleWindow {
    pub hwnd: isize,
    pub title: String,
    pub process: String,
}

impl VisibleWindow {
    fn to_json(&self) -> Value {
        json!({
            "hwnd": self.hwnd as i64,
            "title": self.title,
            "process": self.process,
        })
    }
}

/// Whether a title should be offered in the window list.
///
/// Split out from the enumeration so it can be tested without a desktop: the
/// enumeration itself is untestable, this rule is not.
fn is_listable(title: &str) -> bool {
    !title.is_empty() && !IGNORED_TITLES.contains(&title)
}

/// Clamp to what `SetLayeredWindowAttributes` accepts.
fn clamp_alpha(alpha: i64) -> u8 {
    alpha.clamp(0, 255) as u8
}

/// The next alpha for a process, given what is saved for it.
///
/// Unsaved means opaque, so the first press fades rather than doing nothing.
fn toggled_alpha(current: i64) -> i64 {
    if current < FULLY_OPAQUE {
        FULLY_OPAQUE
    } else {
        HALF_OPACITY
    }
}

#[cfg(windows)]
mod platform {
    use super::{is_listable, VisibleWindow};

    use windows::core::PWSTR;
    use windows::Win32::Foundation::{CloseHandle, COLORREF, HWND, LPARAM};
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetForegroundWindow, GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW,
        GetWindowThreadProcessId, IsWindowVisible, SetLayeredWindowAttributes, SetWindowLongPtrW,
        GWL_EXSTYLE, LWA_ALPHA, WS_EX_LAYERED,
    };

    pub fn foreground_window() -> isize {
        unsafe { GetForegroundWindow() }.0 as isize
    }

    /// A UWP window that is present but hidden — an app suspended in the background
    /// keeps a window that would otherwise show up in the list as if it were open.
    fn is_cloaked(hwnd: HWND) -> bool {
        let mut cloaked: i32 = 0;
        let ok = unsafe {
            DwmGetWindowAttribute(
                hwnd,
                DWMWA_CLOAKED,
                &mut cloaked as *mut i32 as *mut _,
                std::mem::size_of::<i32>() as u32,
            )
        };
        // A failure means DWM has no opinion, which is not the same as "cloaked".
        ok.is_ok() && cloaked != 0
    }

    fn window_title(hwnd: HWND) -> String {
        let length = unsafe { GetWindowTextLengthW(hwnd) };
        if length <= 0 {
            return String::new();
        }
        let mut buffer = vec![0u16; length as usize + 1];
        let written = unsafe { GetWindowTextW(hwnd, &mut buffer) };
        if written <= 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buffer[..written as usize])
    }

    /// The executable name owning a window, or empty if it cannot be determined.
    pub fn process_name(hwnd: isize) -> String {
        let hwnd = HWND(hwnd as *mut _);
        let mut pid: u32 = 0;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if pid == 0 {
            return String::new();
        }

        // LIMITED_INFORMATION is enough for the image name and is granted for
        // processes that would refuse a full query.
        let Ok(process) = (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) })
        else {
            return String::new();
        };

        let mut buffer = vec![0u16; 512];
        let mut size = buffer.len() as u32;
        let queried = unsafe {
            QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                PWSTR(buffer.as_mut_ptr()),
                &mut size,
            )
        };
        let _ = unsafe { CloseHandle(process) };

        if queried.is_err() {
            return String::new();
        }
        let full = String::from_utf16_lossy(&buffer[..size as usize]);
        // Only the file name is stored: a full path would key the settings to an
        // install location and break on an update that moves the executable.
        full.rsplit(['\\', '/']).next().unwrap_or("").to_string()
    }

    /// Collected by the enumeration callback. Passed as the `LPARAM`.
    struct Collector {
        windows: Vec<VisibleWindow>,
    }

    /// # Safety
    /// `lparam` must be the `Collector` pointer `visible_windows` passed in.
    ///
    /// This runs across an FFI boundary, where a Rust panic aborts the process. It
    /// must therefore contain nothing that can panic — no indexing, no `unwrap`.
    unsafe extern "system" fn enumerate(hwnd: HWND, lparam: LPARAM) -> windows::core::BOOL {
        let collector = unsafe { &mut *(lparam.0 as *mut Collector) };

        if !unsafe { IsWindowVisible(hwnd) }.as_bool() || is_cloaked(hwnd) {
            return true.into();
        }
        let title = window_title(hwnd);
        if !is_listable(&title) {
            return true.into();
        }
        // A window whose owner cannot be identified is dropped: the settings are
        // keyed by process name, so one without a name could never be saved.
        let process = process_name(hwnd.0 as isize);
        if process.is_empty() {
            return true.into();
        }

        collector.windows.push(VisibleWindow {
            hwnd: hwnd.0 as isize,
            title,
            process,
        });
        true.into()
    }

    pub fn visible_windows() -> Vec<VisibleWindow> {
        let mut collector = Collector {
            windows: Vec::new(),
        };
        let _ = unsafe {
            EnumWindows(
                Some(enumerate),
                LPARAM(&mut collector as *mut Collector as isize),
            )
        };
        let mut windows = collector.windows;
        windows.sort_by_key(|w| w.title.to_lowercase());
        windows
    }

    /// Apply an opacity, or take the window off the layered path entirely.
    pub fn set_opacity(hwnd: isize, alpha: u8) {
        let hwnd = HWND(hwnd as *mut _);
        let style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
        let layered = WS_EX_LAYERED.0 as isize;

        if alpha < 255 {
            if style & layered == 0 {
                unsafe { SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | layered) };
            }
            let _ = unsafe { SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA) };
        } else if style & layered != 0 {
            // Fully opaque means *not layered*, not "layered at 255" — the window
            // goes back to its ordinary rendering path.
            unsafe { SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style & !layered) };
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::VisibleWindow;

    pub fn foreground_window() -> isize {
        0
    }
    pub fn process_name(_hwnd: isize) -> String {
        String::new()
    }
    pub fn visible_windows() -> Vec<VisibleWindow> {
        Vec::new()
    }
    pub fn set_opacity(_hwnd: isize, _alpha: u8) {}
}

/// The handle of the window with focus, or 0 if there is none.
pub fn foreground_window() -> isize {
    platform::foreground_window()
}

/// The executable name owning a window, or empty if it cannot be determined.
pub fn process_name(hwnd: isize) -> String {
    platform::process_name(hwnd)
}

/// Every visible, titled window whose owning process can be named, by title.
pub fn visible_windows() -> Vec<VisibleWindow> {
    platform::visible_windows()
}

/// Set a window's opacity. Out-of-range values are clamped, never rejected.
pub fn set_opacity(hwnd: isize, alpha: i64) {
    platform::set_opacity(hwnd, clamp_alpha(alpha));
}

// ── persistence ──────────────────────────────────────────────────────────────

/// The saved `{process name: alpha}` map.
///
/// Anything unreadable reads as empty rather than failing the call: this file is a
/// convenience, and a corrupt one should cost the user their saved fades, not the
/// ability to open the screen that would let them set new ones.
///
/// Faithful to `load_opacity_settings`, a **single** unusable entry empties the whole
/// map — the Python builds it with one dict comprehension, so one bad value raises
/// out of the lot.
pub fn load_settings() -> Map<String, Value> {
    let path = config::transparency_file();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Map::new();
    };
    let Ok(Value::Object(raw)) = serde_json::from_str::<Value>(&text) else {
        return Map::new();
    };

    let mut settings = Map::new();
    for (process, value) in raw {
        let Some(alpha) = as_alpha(&value) else {
            return Map::new();
        };
        settings.insert(process, json!(alpha));
    }
    settings
}

/// `int(v)` in Python: a number truncates toward zero, a numeric string parses, and
/// anything else is an error.
fn as_alpha(value: &Value) -> Option<i64> {
    match value {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f.trunc() as i64)),
        Value::String(s) => s.trim().parse::<i64>().ok(),
        _ => None,
    }
}

/// Persist the `{process name: alpha}` map.
///
/// A write failure is logged into the returned error rather than being swallowed as
/// `save_opacity_settings` does in Python — the caller acknowledged a save, and
/// telling it the write worked when it did not is worse than failing.
pub fn store_settings(settings: &Map<String, Value>) -> Result<(), CoreError> {
    let path = config::transparency_file();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| CoreError::io(format!("Could not create {}: {e}", parent.display())))?;
    }
    let text = serde_json::to_string_pretty(&Value::Object(settings.clone()))
        .map_err(|e| CoreError::io(format!("Could not encode the opacity settings: {e}")))?;
    std::fs::write(&path, text)
        .map_err(|e| CoreError::io(format!("Could not write {}: {e}", path.display())))
}

/// Re-apply every saved opacity to whatever windows are open now.
///
/// Matching is by process name, so the setting lands on the window that process has
/// open *today*, not the one it had when the setting was made. Returns how many
/// windows were touched.
pub fn reapply() -> usize {
    let settings = load_settings();
    if settings.is_empty() {
        return 0;
    }
    let mut applied = 0;
    for window in visible_windows() {
        if let Some(alpha) = settings.get(&window.process).and_then(as_alpha) {
            set_opacity(window.hwnd, alpha);
            applied += 1;
        }
    }
    applied
}

// ── RPC results ──────────────────────────────────────────────────────────────

pub fn list_windows_result() -> Value {
    json!({
        "windows": visible_windows().iter().map(VisibleWindow::to_json).collect::<Vec<_>>(),
    })
}

pub fn set_window_opacity_result(hwnd: i64, alpha: i64) -> Value {
    set_opacity(hwnd as isize, alpha);
    // The *requested* alpha is echoed, not the clamped one — that is what `rpc.py`
    // returns, and the front end uses it to confirm its own slider position.
    json!({ "hwnd": hwnd, "alpha": alpha })
}

pub fn get_foreground_window_result() -> Value {
    json!({ "hwnd": foreground_window() as i64 })
}

/// Fade the focused window, or bring it back.
///
/// Keyed by process name so the choice survives the window being closed and
/// reopened, and persisted straight away — a shortcut that forgot its own effect
/// would be useless.
pub fn toggle_foreground_opacity_result() -> Result<Value, CoreError> {
    let hwnd = foreground_window();
    if hwnd == 0 {
        return Err(CoreError::not_found("No focused window."));
    }
    let process = process_name(hwnd);
    if process.is_empty() {
        return Err(CoreError::not_found(
            "Could not identify the focused window.",
        ));
    }

    let mut settings = load_settings();
    let current = settings
        .get(&process)
        .and_then(as_alpha)
        .unwrap_or(FULLY_OPAQUE);
    let alpha = toggled_alpha(current);

    set_opacity(hwnd, alpha);
    settings.insert(process.clone(), json!(alpha));
    store_settings(&settings)?;

    Ok(json!({ "hwnd": hwnd as i64, "process": process, "alpha": alpha }))
}

pub fn get_opacity_settings_result() -> Value {
    json!({ "settings": Value::Object(load_settings()) })
}

pub fn save_opacity_settings_result(settings: &Value) -> Result<Value, CoreError> {
    let Some(incoming) = settings.as_object() else {
        return Err(CoreError::invalid("Opacity settings must be an object."));
    };
    let mut normalised = Map::new();
    for (process, value) in incoming {
        let alpha = as_alpha(value).ok_or_else(|| {
            CoreError::invalid(format!("Opacity for {process} must be a whole number."))
        })?;
        normalised.insert(process.clone(), json!(alpha));
    }
    store_settings(&normalised)?;
    Ok(json!({ "saved": true }))
}

pub fn reapply_opacity_settings_result() -> Value {
    json!({ "applied": reapply() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Sandbox;

    #[test]
    fn the_shell_surfaces_that_are_never_offered() {
        assert!(is_listable("Visual Studio Code"));
        assert!(!is_listable(""), "an untitled window is not offerable");
        assert!(
            !is_listable("Program Manager"),
            "that is the desktop itself"
        );
        assert!(!is_listable("Windows Input Experience"));
        assert!(!is_listable("Settings"));
        assert!(!is_listable("MSCTFIME UI"));
        // The filter is exact, not a prefix — a real window must not be caught by it.
        assert!(is_listable("Settings — Project"));
    }

    #[test]
    fn alpha_is_clamped_rather_than_rejected() {
        assert_eq!(clamp_alpha(-40), 0);
        assert_eq!(clamp_alpha(0), 0);
        assert_eq!(clamp_alpha(128), 128);
        assert_eq!(clamp_alpha(255), 255);
        assert_eq!(clamp_alpha(9000), 255);
    }

    /// The toggle has to start from "opaque" for a process it has never seen, or the
    /// first press of the hotkey would appear to do nothing.
    #[test]
    fn toggling_starts_by_fading_and_then_restores() {
        assert_eq!(toggled_alpha(FULLY_OPAQUE), HALF_OPACITY);
        assert_eq!(toggled_alpha(HALF_OPACITY), FULLY_OPAQUE);
        assert_eq!(toggled_alpha(0), FULLY_OPAQUE, "anything faded restores");
        assert_eq!(toggled_alpha(254), FULLY_OPAQUE);
    }

    #[test]
    fn settings_round_trip_through_the_file() {
        let _sandbox = Sandbox::new("opacity");
        assert!(load_settings().is_empty(), "no file reads as no settings");

        let mut settings = Map::new();
        settings.insert("code.exe".into(), json!(180));
        settings.insert("explorer.exe".into(), json!(255));
        store_settings(&settings).unwrap();

        let read_back = load_settings();
        assert_eq!(read_back["code.exe"], 180);
        assert_eq!(read_back["explorer.exe"], 255);
    }

    /// A file the user (or a crash) mangled must not stop the screen from opening.
    #[test]
    fn an_unreadable_file_reads_as_empty() {
        let _sandbox = Sandbox::new("badopacity");
        let path = config::transparency_file();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();

        std::fs::write(&path, "{ not json").unwrap();
        assert!(load_settings().is_empty());

        // A JSON array is valid JSON and still not a settings map.
        std::fs::write(&path, "[1, 2, 3]").unwrap();
        assert!(load_settings().is_empty());

        // One unusable value empties the lot, as the Python dict comprehension does.
        std::fs::write(&path, r#"{"code.exe": 180, "bad.exe": "not a number"}"#).unwrap();
        assert!(load_settings().is_empty());
    }

    #[test]
    fn numeric_strings_and_floats_are_accepted_the_way_int_accepts_them() {
        assert_eq!(as_alpha(&json!(200)), Some(200));
        assert_eq!(as_alpha(&json!(200.9)), Some(200), "int() truncates");
        assert_eq!(as_alpha(&json!("128")), Some(128));
        assert_eq!(as_alpha(&json!(true)), None);
        assert_eq!(as_alpha(&json!(null)), None);
        assert_eq!(as_alpha(&json!("half")), None);
    }

    #[test]
    fn saving_rejects_a_value_that_is_not_a_number() {
        let _sandbox = Sandbox::new("savebad");
        let err = save_opacity_settings_result(&json!({ "code.exe": "opaque" })).unwrap_err();
        assert_eq!(err.kind(), crate::ErrorKind::Invalid);

        let err = save_opacity_settings_result(&json!([])).unwrap_err();
        assert_eq!(err.kind(), crate::ErrorKind::Invalid);
    }

    #[test]
    fn saving_an_empty_map_is_a_valid_way_to_forget_everything() {
        let _sandbox = Sandbox::new("saveempty");
        let mut settings = Map::new();
        settings.insert("code.exe".into(), json!(100));
        store_settings(&settings).unwrap();

        save_opacity_settings_result(&json!({})).unwrap();
        assert!(load_settings().is_empty());
    }

    /// Reading the desktop is safe; changing it is not. This only asserts the call
    /// answers on whatever machine runs the suite.
    #[test]
    fn enumerating_windows_never_panics() {
        for window in visible_windows() {
            assert!(!window.title.is_empty());
            assert!(!window.process.is_empty());
            assert_ne!(window.hwnd, 0);
        }
        let _ = foreground_window();
    }

    /// With nothing saved there is nothing to reapply, so this cannot touch a real
    /// window — which is what makes it safe to run.
    #[test]
    fn reapplying_with_no_settings_touches_nothing() {
        let _sandbox = Sandbox::new("reapply");
        assert_eq!(reapply(), 0);
    }
}
