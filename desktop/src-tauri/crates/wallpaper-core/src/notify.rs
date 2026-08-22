//! Telling the user something, outside the window.
//!
//! Ports `notifications.py`, the last method the Python sidecar answered.
//!
//! Behind a trait for the same reason [`crate::WallpaperSetter`] is: sending a toast
//! needs the Tauri application handle, and this crate deliberately has no `tauri`
//! dependency so it stays testable headless. The shell supplies the real one, the
//! stdio CLI and the tests supply something that only writes to the log.
//!
//! ## What this replaces
//!
//! `send_windows_notification` built a PowerShell script, base64-encoded it, and
//! spawned `powershell.exe -EncodedCommand` **per toast**, on a thread, ignoring the
//! result. `tauri-plugin-notification` was already a dependency of the shell, so the
//! subprocess goes away entirely.
//!
//! One deliberate difference: Python could not report a failure, because the work
//! happened on a detached thread after the method had already returned `{"sent": true}`.
//! Here the send is synchronous and a failure is reported. Nothing in the interface
//! calls `notify` today, so there is no behaviour anyone is relying on to change.

use crate::CoreError;

/// Somewhere a short message can be shown to the user.
pub trait Notifier: Send + Sync {
    fn notify(&self, title: &str, message: &str) -> Result<(), CoreError>;
}

/// Writes the notification to the application log instead of showing it.
///
/// For the stdio CLI and for tests: both want the call to succeed and neither has a
/// notification centre to show anything in.
pub struct LoggingNotifier;

impl Notifier for LoggingNotifier {
    fn notify(&self, title: &str, message: &str) -> Result<(), CoreError> {
        log::info!("notification: {title} — {message}");
        Ok(())
    }
}
