//! The Windows "start with the system" registry entry.
//!
//! Ports `startup.py`.
//!
//! ## This is not the entry the app actually uses
//!
//! The shipping app registers autostart through `tauri-plugin-autostart`, which
//! writes the `Run` value **`Wallpaper Changer`** (with a space, from `productName`)
//! pointing at `tauri-native.exe --minimized`. This module reads and writes
//! **`WallpaperChanger`** (no space), which is a *different* value under the same
//! key.
//!
//! Two consequences, both inherited rather than introduced:
//!
//! - [`is_enabled`] reports `false` on a machine where autostart is on, because it
//!   is looking at a name nothing writes. `get_capabilities` carries that answer.
//! - [`set_enabled`] would add a *second* autostart entry rather than changing the
//!   real one.
//!
//! Nothing calls them today: `engine.ts` declares `getStartupEnabled` and
//! `setStartupEnabled`, but the Settings screen uses `@tauri-apps/plugin-autostart`
//! instead. They are ported as they are so the protocol keeps its shape; pointing
//! them at the plugin's value name, or dropping them, is a decision for the phase
//! that owns packaging.

use crate::CoreError;

const APP_VALUE: &str = "WallpaperChanger";
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const STARTUP_FLAG: &str = "--startup";

/// Whether the process was launched by the `Run` entry.
pub fn is_startup_launch() -> bool {
    std::env::args().any(|arg| arg == STARTUP_FLAG)
}

#[cfg(windows)]
mod platform {
    use super::{APP_VALUE, RUN_KEY, STARTUP_FLAG};
    use crate::CoreError;

    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{ERROR_SUCCESS, WIN32_ERROR};
    use windows::Win32::System::Registry::{
        RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ,
    };

    /// A null-terminated UTF-16 buffer, which is what every `W` entry point wants.
    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn is_enabled() -> bool {
        let key = wide(RUN_KEY);
        let name = wide(APP_VALUE);
        let mut size: u32 = 0;

        // First call sizes the buffer; a missing value fails here and reads as off.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                PCWSTR(key.as_ptr()),
                PCWSTR(name.as_ptr()),
                RRF_RT_REG_SZ,
                None,
                None,
                Some(&mut size),
            )
        };
        if status != ERROR_SUCCESS || size == 0 {
            return false;
        }

        let mut buffer = vec![0u8; size as usize];
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                PCWSTR(key.as_ptr()),
                PCWSTR(name.as_ptr()),
                RRF_RT_REG_SZ,
                None,
                Some(buffer.as_mut_ptr() as *mut _),
                Some(&mut size),
            )
        };
        if status != ERROR_SUCCESS {
            return false;
        }

        // `bool(val)` in Python: present but empty counts as disabled.
        let chars: Vec<u16> = buffer
            .chunks_exact(2)
            .map(|c| u16::from_ne_bytes([c[0], c[1]]))
            .take_while(|&c| c != 0)
            .collect();
        !chars.is_empty()
    }

    /// `"<exe>" --startup`, quoted because the install path contains a space.
    fn command() -> Result<String, CoreError> {
        let exe = std::env::current_exe()
            .map_err(|e| CoreError::io(format!("Could not locate the executable: {e}")))?;
        Ok(format!("\"{}\" {STARTUP_FLAG}", exe.display()))
    }

    fn check(status: WIN32_ERROR, what: &str) -> Result<(), CoreError> {
        if status == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(CoreError::io(format!("{what} failed (error {})", status.0)))
        }
    }

    pub fn set_enabled(enabled: bool) -> Result<(), CoreError> {
        let key = wide(RUN_KEY);
        let name = wide(APP_VALUE);

        if !enabled {
            let status = unsafe {
                RegDeleteKeyValueW(
                    HKEY_CURRENT_USER,
                    PCWSTR(key.as_ptr()),
                    PCWSTR(name.as_ptr()),
                )
            };
            // Deleting something that is not there is success, as it is in Python.
            if status == ERROR_SUCCESS || !is_enabled() {
                return Ok(());
            }
            return check(status, "removing the autostart entry");
        }

        let value = wide(&command()?);
        // REG_SZ wants a byte count, and it must include the null terminator that
        // `wide` appended — a short count truncates the last character.
        let byte_len = std::mem::size_of_val(value.as_slice()) as u32;
        let status = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                PCWSTR(key.as_ptr()),
                PCWSTR(name.as_ptr()),
                REG_SZ.0,
                Some(value.as_ptr() as *const _),
                byte_len,
            )
        };
        check(status, "writing the autostart entry")
    }
}

#[cfg(not(windows))]
mod platform {
    use crate::CoreError;

    pub fn is_enabled() -> bool {
        false
    }

    pub fn set_enabled(_enabled: bool) -> Result<(), CoreError> {
        Err(CoreError::io("Autostart is only implemented on Windows."))
    }
}

/// Whether the `WallpaperChanger` `Run` value is present and non-empty.
pub fn is_enabled() -> bool {
    platform::is_enabled()
}

/// Add or remove the `WallpaperChanger` `Run` value.
pub fn set_enabled(enabled: bool) -> Result<(), CoreError> {
    platform::set_enabled(enabled)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Read-only, so it is safe to run against the real registry: it must answer
    /// rather than fail, whatever the machine happens to have.
    #[test]
    fn reading_the_flag_never_panics() {
        let _ = is_enabled();
    }

    /// The flag is read from the command line, so an unrelated argument list is not
    /// mistaken for a startup launch.
    #[test]
    fn startup_launch_is_detected_from_the_arguments() {
        // The test harness is not launched with --startup.
        assert!(!is_startup_launch());
    }
}
