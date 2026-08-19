//! Finding the desktop layer to render live wallpaper into.
//!
//! Ports `workerw.py`. WORKERW sits **behind** the desktop icons (`SHELLDLL_DefView`)
//! but **above** the wallpaper bitmap, which makes it the only surface where a video
//! can play without covering the icons or being covered by them.
//!
//! ## Three shapes of desktop, and why all three are tried
//!
//! Explorer does not expose WORKERW the same way across Windows versions, and the
//! difference is not something a version check can be trusted to predict:
//!
//! 1. **Windows 11** — WorkerW is a direct child of Progman.
//! 2. **Windows 10** — WorkerW is a *top-level* window, the sibling immediately
//!    following the one that hosts `SHELLDLL_DefView`.
//! 3. **Some Windows 11 builds** paint the wallpaper straight from Progman and never
//!    make a WorkerW at all, so [`desktop_parent`] falls back to Progman itself.
//!
//! Getting `None` from [`desktop_parent`] therefore means something much narrower than
//! "unsupported Windows": it means Explorer is not running.
//!
//! ## The undocumented message
//!
//! `0x052C` asks Progman to spawn the WorkerW layer. It is undocumented, it is what
//! every live-wallpaper tool uses, and it is idempotent — sending it when the layer
//! already exists does nothing. The 100 ms sleep afterwards is not superstition: the
//! shell creates the window asynchronously, and looking for it immediately finds
//! nothing on a machine that was about to succeed.

/// The handle of the WORKERW desktop layer, or `None` if there is not one.
pub fn workerw() -> Option<isize> {
    platform::workerw()
}

/// A window suitable as the parent for desktop-layer embedding.
///
/// Prefers WORKERW and falls back to Progman. `None` only when the desktop shell is
/// not running at all.
pub fn desktop_parent() -> Option<isize> {
    platform::desktop_parent()
}

/// The screen-space top-left of a window, for mapping monitor coordinates into it.
pub fn window_origin(hwnd: isize) -> (i32, i32) {
    platform::window_origin(hwnd)
}

/// Force the desktop layer to repaint, so no last video frame stays frozen on it.
pub fn refresh(hwnd: isize) {
    platform::refresh(hwnd)
}

/// The direct children of a window.
///
/// For checking that the video host windows really went away. `DestroyWindow` from the
/// wrong thread fails silently, so "we asked it to close" and "it closed" are different
/// claims, and this is how the second one gets made.
pub fn children(parent: isize) -> Vec<isize> {
    platform::children(parent)
}

#[cfg(windows)]
mod platform {
    use std::sync::atomic::{AtomicIsize, Ordering};
    use std::sync::Mutex;
    use std::time::Duration;

    use windows::core::w;
    use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::{InvalidateRect, UpdateWindow};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumChildWindows, EnumWindows, FindWindowExW, FindWindowW, GetWindowRect,
        SendMessageTimeoutW, SMTO_NORMAL,
    };

    /// Undocumented: asks Progman to spawn the WorkerW behind the icons.
    const WM_SPAWN_WORKERW: u32 = 0x052C;

    fn progman() -> Option<HWND> {
        unsafe { FindWindowW(w!("Progman"), None) }
            .ok()
            .filter(|h| !h.is_invalid())
    }

    /// Ask Progman to create the WorkerW layer. Idempotent — safe to repeat.
    ///
    /// The timeout matters: `SendMessageTimeoutW` rather than `SendMessageW` because a
    /// wedged Explorer would otherwise block this thread forever, and a video
    /// wallpaper that will not start is better than an app that will not respond.
    fn spawn_workerw(progman: HWND) {
        let mut result = 0usize;
        unsafe {
            SendMessageTimeoutW(
                progman,
                WM_SPAWN_WORKERW,
                WPARAM(0),
                LPARAM(0),
                SMTO_NORMAL,
                1000,
                Some(&mut result),
            );
        }
    }

    /// Where the `EnumWindows` callback leaves its answer.
    ///
    /// A static rather than the `LPARAM`, because the callback must not panic — a
    /// panic unwinding into a Win32 callback aborts the process — and dereferencing a
    /// pointer smuggled through `LPARAM` is one more thing that could go wrong inside
    /// it. Discovery is only ever done from the video thread, so there is no contention.
    static FOUND: AtomicIsize = AtomicIsize::new(0);

    /// **Must not panic.** Every call here is fallible-by-return.
    unsafe extern "system" fn find_sibling(hwnd: HWND, _lparam: LPARAM) -> windows::core::BOOL {
        // The DefView host is the marker; the WorkerW we want is its next sibling.
        let hosts_defview = FindWindowExW(Some(hwnd), None, w!("SHELLDLL_DefView"), None)
            .is_ok_and(|h| !h.is_invalid());
        if hosts_defview {
            if let Ok(sibling) = FindWindowExW(None, Some(hwnd), w!("WorkerW"), None) {
                if !sibling.is_invalid() {
                    FOUND.store(sibling.0 as isize, Ordering::SeqCst);
                }
            }
        }
        true.into()
    }

    pub fn workerw() -> Option<isize> {
        let progman = progman()?;

        spawn_workerw(progman);
        // The shell materialises the window asynchronously; looking immediately finds
        // nothing on a machine that was about to succeed.
        std::thread::sleep(Duration::from_millis(100));

        // Windows 11: a direct child of Progman.
        if let Ok(child) = unsafe { FindWindowExW(Some(progman), None, w!("WorkerW"), None) } {
            if !child.is_invalid() {
                return Some(child.0 as isize);
            }
        }

        // Windows 10: the top-level sibling of the DefView host.
        FOUND.store(0, Ordering::SeqCst);
        let _ = unsafe { EnumWindows(Some(find_sibling), LPARAM(0)) };
        match FOUND.load(Ordering::SeqCst) {
            0 => None,
            found => Some(found),
        }
    }

    pub fn desktop_parent() -> Option<isize> {
        if let Some(worker) = workerw() {
            return Some(worker);
        }
        progman().map(|h| h.0 as isize)
    }

    pub fn window_origin(hwnd: isize) -> (i32, i32) {
        let mut rect = RECT::default();
        if unsafe { GetWindowRect(HWND(hwnd as *mut _), &mut rect) }.is_ok() {
            (rect.left, rect.top)
        } else {
            (0, 0)
        }
    }

    pub fn refresh(hwnd: isize) {
        let hwnd = HWND(hwnd as *mut _);
        unsafe {
            let _ = InvalidateRect(Some(hwnd), None, true);
            let _ = UpdateWindow(hwnd);
        }
    }

    /// Collected by the same static-and-no-panic rule as [`find_sibling`].
    static CHILDREN: Mutex<Vec<isize>> = Mutex::new(Vec::new());

    /// **Must not panic.**
    unsafe extern "system" fn collect_child(hwnd: HWND, _lparam: LPARAM) -> windows::core::BOOL {
        if let Ok(mut children) = CHILDREN.lock() {
            children.push(hwnd.0 as isize);
        }
        true.into()
    }

    pub fn children(parent: isize) -> Vec<isize> {
        let Ok(mut guard) = CHILDREN.lock() else {
            return Vec::new();
        };
        guard.clear();
        drop(guard);
        let _ = unsafe {
            EnumChildWindows(Some(HWND(parent as *mut _)), Some(collect_child), LPARAM(0))
        };
        CHILDREN.lock().map(|c| c.clone()).unwrap_or_default()
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn workerw() -> Option<isize> {
        None
    }
    pub fn desktop_parent() -> Option<isize> {
        None
    }
    pub fn window_origin(_hwnd: isize) -> (i32, i32) {
        (0, 0)
    }
    pub fn refresh(_hwnd: isize) {}
    pub fn children(_parent: isize) -> Vec<isize> {
        Vec::new()
    }
}
