//! Display enumeration — the replacement for `monitor.py`'s `screeninfo` wrapper.
//!
//! Everything downstream is arithmetic on these rectangles: the virtual desktop the
//! collage is composed onto, which grid cell lands on which screen, and where the
//! video host windows are positioned inside WORKERW. The numbers must match what the
//! Python engine produced, or pictures move between screens without a line of layout
//! code changing.
//!
//! ## Coordinates are physical, and that is not an accident
//!
//! A DPI-unaware process is handed *virtualized* coordinates: a 3840x2160 display at
//! 150% is reported as 2560x1440. Composing at that size and handing it to
//! `SystemParametersInfoW` makes Windows stretch the result back up, which is
//! visibly soft.
//!
//! `screeninfo` avoids this by calling `SetProcessDpiAwareness(2)` inside its
//! enumerator, so the Python engine has been reading physical coordinates all along
//! — it just acquires that awareness lazily, on the first `get_monitors()` call,
//! rather than at startup. This module does the same thing eagerly and idempotently,
//! which also means `wallpaper-core-cli` (a plain console binary, DPI-unaware by
//! default) reports exactly what the Tauri process reports.

use crate::{CoreError, ErrorKind};

/// One display, field-identical to `monitor.py`'s dataclass.
///
/// `name` is carried for diagnostics only and never crosses the wire — the RPC shape
/// is `{index, x, y, width, height}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Monitor {
    pub index: usize,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub name: String,
}

impl Monitor {
    /// The `{index, x, y, width, height}` object the protocol carries.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "index": self.index,
            "x": self.x,
            "y": self.y,
            "width": self.width,
            "height": self.height,
        })
    }
}

/// The bounding box of every display: `(min_x, min_y, total_width, total_height)`.
///
/// Ports `wallpaper.py::get_virtual_desktop`. The composite is exactly this
/// rectangle, so a monitor at a negative offset — a screen above or left of the
/// primary, which is the common multi-head layout — shifts the origin rather than
/// growing the canvas.
pub fn virtual_desktop(monitors: &[Monitor]) -> Result<(i32, i32, i32, i32), CoreError> {
    if monitors.is_empty() {
        return Err(CoreError::no_monitors("No monitors detected."));
    }
    let min_x = monitors.iter().map(|m| m.x).min().unwrap();
    let min_y = monitors.iter().map(|m| m.y).min().unwrap();
    let max_x = monitors.iter().map(|m| m.x + m.width).max().unwrap();
    let max_y = monitors.iter().map(|m| m.y + m.height).max().unwrap();
    Ok((min_x, min_y, max_x - min_x, max_y - min_y))
}

#[cfg(windows)]
mod platform {
    use super::Monitor;
    use crate::CoreError;

    use windows::core::BOOL;
    use windows::Win32::Foundation::{LPARAM, RECT, TRUE};
    use windows::Win32::Graphics::Gdi::{
        EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
    };
    use windows::Win32::UI::HiDpi::{
        SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    };

    /// Ask for per-monitor DPI awareness, once per process.
    ///
    /// Best-effort by design. Inside the Tauri app tao has already set it and this
    /// call fails harmlessly; inside `wallpaper-core-cli` it is what makes the CLI
    /// agree with the app. Failure only means we are back to the virtualized
    /// coordinates a DPI-unaware process would have seen anyway.
    fn ensure_dpi_aware() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        });
    }

    unsafe extern "system" fn collect(
        handle: HMONITOR,
        _hdc: HDC,
        _clip: *mut RECT,
        lparam: LPARAM,
    ) -> BOOL {
        let found = &mut *(lparam.0 as *mut Vec<Monitor>);

        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;

        // GetMonitorInfoW takes the MONITORINFO prefix; MONITORINFOEXW extends it
        // with szDevice, which cbSize above is what opts us into.
        if GetMonitorInfoW(handle, &mut info as *mut _ as *mut MONITORINFO).as_bool() {
            let r = info.monitorInfo.rcMonitor;
            let name = String::from_utf16_lossy(&info.szDevice)
                .trim_end_matches('\0')
                .to_string();
            found.push(Monitor {
                index: found.len(),
                x: r.left,
                y: r.top,
                width: r.right - r.left,
                height: r.bottom - r.top,
                name,
            });
        }
        TRUE // keep enumerating
    }

    pub fn enumerate() -> Result<Vec<Monitor>, CoreError> {
        ensure_dpi_aware();

        let mut found: Vec<Monitor> = Vec::new();
        // NULL device context and NULL clip rectangle: every display, in the order
        // Windows reports them. `screeninfo` passes a desktop DC instead, because it
        // also wants per-monitor DCs for physical size; that changes nothing about
        // which monitors come back or in what order.
        let ok = unsafe {
            EnumDisplayMonitors(
                None,
                None,
                Some(collect),
                LPARAM(&mut found as *mut _ as isize),
            )
        };
        if !ok.as_bool() {
            return Err(CoreError::no_monitors("EnumDisplayMonitors failed."));
        }
        Ok(found)
    }
}

#[cfg(not(windows))]
mod platform {
    use super::Monitor;
    use crate::CoreError;

    pub fn enumerate() -> Result<Vec<Monitor>, CoreError> {
        Err(CoreError::no_monitors(
            "Display enumeration is only implemented on Windows.",
        ))
    }
}

/// Every display attached right now, in Windows' own enumeration order.
///
/// The order is the identity of a monitor as far as the rest of the engine is
/// concerned — `Monitor.index` is what a saved collage records and what the preview
/// hit-targets refer to.
pub fn get_monitors() -> Result<Vec<Monitor>, CoreError> {
    platform::enumerate()
}

/// The `get_monitors` RPC result.
pub fn get_monitors_result() -> Result<serde_json::Value, CoreError> {
    let monitors = get_monitors()?;
    let (_, _, width, height) = match virtual_desktop(&monitors) {
        Ok(v) => v,
        // `rpc.py` answers with zeroes rather than failing, so the UI can render an
        // empty stage instead of an error toast.
        Err(e) if e.kind() == ErrorKind::NoMonitors => (0, 0, 0, 0),
        Err(e) => return Err(e),
    };
    Ok(serde_json::json!({
        "monitors": monitors.iter().map(Monitor::to_json).collect::<Vec<_>>(),
        "virtual_width": width,
        "virtual_height": height,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(index: usize, x: i32, y: i32, width: i32, height: i32) -> Monitor {
        Monitor { index, x, y, width, height, name: format!("\\\\.\\DISPLAY{index}") }
    }

    #[test]
    fn virtual_desktop_of_a_single_screen_is_that_screen() {
        let m = [mon(0, 0, 0, 1920, 1080)];
        assert_eq!(virtual_desktop(&m).unwrap(), (0, 0, 1920, 1080));
    }

    /// The layout on the development machine: a 4K screen sitting up and to the
    /// right of a 1080p primary, so both origins are negative in y.
    #[test]
    fn virtual_desktop_spans_a_screen_at_a_negative_offset() {
        let m = [mon(0, 0, 0, 1920, 1080), mon(1, 1920, -1072, 3840, 2160)];
        assert_eq!(virtual_desktop(&m).unwrap(), (0, -1072, 5760, 2160));
    }

    /// A screen left of the primary moves the origin rather than widening the canvas
    /// past what the displays actually cover.
    #[test]
    fn virtual_desktop_handles_a_screen_to_the_left() {
        let m = [mon(0, 0, 0, 1920, 1080), mon(1, -1920, 0, 1920, 1080)];
        assert_eq!(virtual_desktop(&m).unwrap(), (-1920, 0, 3840, 1080));
    }

    #[test]
    fn virtual_desktop_without_monitors_is_an_error() {
        assert_eq!(virtual_desktop(&[]).unwrap_err().kind(), ErrorKind::NoMonitors);
    }

    #[test]
    fn json_shape_is_the_python_dataclass_without_the_name() {
        let json = mon(2, 10, -20, 800, 600).to_json();
        assert_eq!(json["index"], 2);
        assert_eq!(json["x"], 10);
        assert_eq!(json["y"], -20);
        assert_eq!(json["width"], 800);
        assert_eq!(json["height"], 600);
        assert!(json.get("name").is_none(), "name is diagnostics only");
    }

    /// Real hardware: whatever is attached, the invariants must hold.
    #[cfg(windows)]
    #[test]
    fn enumeration_agrees_with_itself() {
        let monitors = get_monitors().expect("enumeration failed");
        assert!(!monitors.is_empty(), "no displays enumerated");
        for (i, m) in monitors.iter().enumerate() {
            assert_eq!(m.index, i, "indices must be dense and in enumeration order");
            assert!(m.width > 0 && m.height > 0, "empty rectangle for {}", m.name);
        }
        let (_, _, w, h) = virtual_desktop(&monitors).unwrap();
        assert!(w > 0 && h > 0);
    }
}
