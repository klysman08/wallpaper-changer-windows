//! Putting a composed picture on the desktop.
//!
//! Ports the applying half of `wallpaper.py`: `set_wallpaper_win`,
//! `apply_single_wallpaper`, `apply_desktop_image` and `_apply_collage`. The
//! composing half already lives in [`crate::collage`]; this module is only about the
//! last step — write a BMP, hand it to Windows.
//!
//! Everything here goes through [`WallpaperSetter`] rather than calling
//! `SystemParametersInfoW` directly. The Python tests patched `ctypes` to assert the
//! call happened; a trait says the same thing better, and it is what lets the history
//! ring, the apply lock and the rotation timer be driven headlessly in tests without
//! a real desktop underneath them.

use std::path::{Path, PathBuf};

use image::RgbImage;
use serde_json::Value;

use crate::collage::{compose_collage, fit_image, paste};
use crate::monitor::{virtual_desktop, Monitor};
use crate::{effects, CoreError};

/// Whatever actually makes a file the desktop wallpaper.
///
/// Implemented once for real Windows and once, in tests, by something that just
/// records what it was asked to show.
pub trait WallpaperSetter: Send + Sync {
    fn set(&self, path: &Path) -> Result<(), CoreError>;
}

/// The real thing: `SystemParametersInfoW` plus the span-style registry values.
pub struct WindowsSetter;

impl WallpaperSetter for WindowsSetter {
    fn set(&self, path: &Path) -> Result<(), CoreError> {
        // `Path.resolve()` in Python yields a plain absolute path, never the `\\?\`
        // extended form — and the extended form is not what SystemParametersInfoW
        // should be writing into the registry. `dunce` resolves the same way.
        let absolute = dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        platform::set_wallpaper(&absolute)
    }
}

#[cfg(windows)]
mod platform {
    use std::path::Path;

    use windows::core::PCWSTR;
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPIF_UPDATEINIFILE, SPI_SETDESKWALLPAPER,
    };

    use crate::CoreError;

    const DESKTOP_KEY: &str = r"Control Panel\Desktop";

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Tell Windows to lay one picture across every screen.
    ///
    /// `WallpaperStyle = 22` is "span". The composite already *is* the whole virtual
    /// desktop, so any other style would letterbox it or repeat it per screen.
    fn set_style_span() -> Result<(), CoreError> {
        let key = wide(DESKTOP_KEY);
        for (name, value) in [("WallpaperStyle", "22"), ("TileWallpaper", "0")] {
            let name = wide(name);
            let value = wide(value);
            let status = unsafe {
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    PCWSTR(key.as_ptr()),
                    PCWSTR(name.as_ptr()),
                    REG_SZ.0,
                    Some(value.as_ptr() as *const _),
                    std::mem::size_of_val(value.as_slice()) as u32,
                )
            };
            if status != ERROR_SUCCESS {
                return Err(CoreError::io(format!(
                    "Could not set the wallpaper style (error {})",
                    status.0
                )));
            }
        }
        Ok(())
    }

    pub fn set_wallpaper(path: &Path) -> Result<(), CoreError> {
        set_style_span()?;
        let mut wide_path = wide(&path.to_string_lossy());
        // SPIF_SENDWININICHANGE is omitted deliberately, exactly as wallpaper.py
        // notes: broadcasting WM_SETTINGCHANGE makes Explorer run its own animated
        // crossfade over WorkerW, so a system fade shows even when the user asked
        // for none. SPIF_UPDATEINIFILE persists the path, and the call itself
        // applies the picture immediately.
        unsafe {
            SystemParametersInfoW(
                SPI_SETDESKWALLPAPER,
                0,
                Some(wide_path.as_mut_ptr() as *mut _),
                SPIF_UPDATEINIFILE,
            )
        }
        .map_err(|e| {
            CoreError::io(format!(
                "SystemParametersInfoW could not apply the wallpaper: {e}"
            ))
        })
    }
}

#[cfg(not(windows))]
mod platform {
    use std::path::Path;

    use crate::CoreError;

    pub fn set_wallpaper(_path: &Path) -> Result<(), CoreError> {
        Err(CoreError::io(
            "Setting the wallpaper is only implemented on Windows.",
        ))
    }
}

/// Persist the canvas and hand it to the setter.
///
/// Always BMP: `SystemParametersInfoW` is the consumer, and BMP is what it reliably
/// accepts. This function is all that `transition.py` amounted to — the visual
/// transition is Windows' own fade, and there has never been anything to animate.
fn write_and_set(
    canvas: &RgbImage,
    out: &Path,
    setter: &dyn WallpaperSetter,
) -> Result<(), CoreError> {
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| CoreError::io(format!("Could not create {}: {e}", parent.display())))?;
    }
    canvas
        .save_with_format(out, image::ImageFormat::Bmp)
        .map_err(|e| CoreError::io(format!("Could not write {}: {e}", out.display())))?;
    setter.set(out)
}

/// The collage path: compose from the configuration, then apply.
pub fn apply_collage(
    cfg: &Value,
    monitors: &[Monitor],
    output_dir: &Path,
    preset_images: Option<&[String]>,
    setter: &dyn WallpaperSetter,
) -> Result<(PathBuf, Vec<String>), CoreError> {
    if monitors.is_empty() {
        return Err(CoreError::invalid("No monitors detected."));
    }
    // `None` for the state file: this is the real rotation, so the selection it
    // makes is exactly what should be journalled.
    let (canvas, used) = compose_collage(cfg, monitors, preset_images, None)?;
    let out = output_dir.join("wallpaper_collage.bmp");
    write_and_set(&canvas, &out, setter)?;
    Ok((out, used))
}

/// One picture, repeated at every screen's own size.
pub fn apply_single(
    image_path: &Path,
    monitors: &[Monitor],
    output_dir: &Path,
    fit_mode: &str,
    effect: &str,
    setter: &dyn WallpaperSetter,
) -> Result<PathBuf, CoreError> {
    let source = open_rgb(image_path)?;
    let (min_x, min_y, total_w, total_h) = virtual_desktop(monitors)?;
    let mut canvas = RgbImage::new(total_w.max(1) as u32, total_h.max(1) as u32);
    for monitor in monitors {
        let fitted = fit_image(&source, monitor.width, monitor.height, fit_mode);
        paste(&mut canvas, &fitted, monitor.x - min_x, monitor.y - min_y);
    }
    let canvas = effects::apply_effect(&canvas, effect)?;
    let out = output_dir.join("wallpaper_default.bmp");
    write_and_set(&canvas, &out, setter)?;
    Ok(out)
}

/// One picture laid across the *whole* virtual desktop.
///
/// Distinct from [`apply_single`], which repeats it per screen. A saved desktop-wide
/// collage is already arranged for the full desktop, so repeating it would put the
/// entire thing, shrunk, on every monitor.
///
/// No effect is applied: the file already carries whichever one was active when it
/// was composed, and running that a second time is not the same picture.
pub fn apply_desktop(
    image_path: &Path,
    monitors: &[Monitor],
    output_dir: &Path,
    fit_mode: &str,
    setter: &dyn WallpaperSetter,
) -> Result<PathBuf, CoreError> {
    let source = open_rgb(image_path)?;
    let (_, _, total_w, total_h) = virtual_desktop(monitors)?;
    let canvas = fit_image(&source, total_w, total_h, fit_mode);
    let out = output_dir.join("wallpaper_saved.bmp");
    write_and_set(&canvas, &out, setter)?;
    Ok(out)
}

fn open_rgb(path: &Path) -> Result<RgbImage, CoreError> {
    Ok(image::open(path)
        .map_err(|e| CoreError::invalid(format!("Could not read {}: {e}", path.display())))?
        .to_rgb8())
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use std::sync::Mutex;

    /// Records what it was asked to show instead of touching the desktop.
    #[derive(Default)]
    pub struct FakeSetter {
        pub applied: Mutex<Vec<PathBuf>>,
        /// When set, every call fails with this message — for the paths that have to
        /// survive a failing apply, a rotation tick most of all.
        pub fail_with: Mutex<Option<String>>,
    }

    impl FakeSetter {
        pub fn count(&self) -> usize {
            self.applied.lock().unwrap().len()
        }
    }

    impl WallpaperSetter for FakeSetter {
        fn set(&self, path: &Path) -> Result<(), CoreError> {
            if let Some(message) = self.fail_with.lock().unwrap().clone() {
                return Err(CoreError::io(message));
            }
            self.applied.lock().unwrap().push(path.to_path_buf());
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::FakeSetter;
    use super::*;
    use std::sync::Mutex;

    fn monitors() -> Vec<Monitor> {
        vec![
            Monitor { index: 0, x: 0, y: 0, width: 80, height: 60, name: "A".into() },
            Monitor { index: 1, x: 80, y: -10, width: 60, height: 40, name: "B".into() },
        ]
    }

    fn a_picture(dir: &Path, name: &str) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(name);
        RgbImage::from_fn(30, 20, |x, y| image::Rgb([x as u8 * 8, y as u8 * 12, 40]))
            .save(&path)
            .unwrap();
        path
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("wc-apply-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The composite is the virtual desktop, so a picture repeated per screen must
    /// still produce one canvas the size of the whole thing.
    #[test]
    fn a_single_picture_covers_the_virtual_desktop() {
        let dir = scratch("single");
        let picture = a_picture(&dir, "src.png");
        let setter = FakeSetter::default();

        let out = apply_single(&picture, &monitors(), &dir, "fill", "normal", &setter).unwrap();

        assert_eq!(out.file_name().unwrap(), "wallpaper_default.bmp");
        assert_eq!(setter.applied.lock().unwrap().as_slice(), &[out.clone()]);
        let written = image::open(&out).unwrap().to_rgb8();
        assert_eq!(written.dimensions(), (140, 70)); // x 0..140, y -10..60
    }

    /// A desktop-wide save is fitted once to the whole canvas, not per screen.
    #[test]
    fn a_desktop_picture_is_fitted_once() {
        let dir = scratch("desktop");
        let picture = a_picture(&dir, "src.png");
        let setter = FakeSetter::default();

        let out = apply_desktop(&picture, &monitors(), &dir, "fill", &setter).unwrap();

        assert_eq!(out.file_name().unwrap(), "wallpaper_saved.bmp");
        let written = image::open(&out).unwrap().to_rgb8();
        assert_eq!(written.dimensions(), (140, 70));
    }

    /// The file must exist before the setter is asked to show it — the other order
    /// would hand Windows a path to nothing.
    #[test]
    fn the_file_is_written_before_it_is_applied() {
        struct Checking(Mutex<bool>);
        impl WallpaperSetter for Checking {
            fn set(&self, path: &Path) -> Result<(), CoreError> {
                *self.0.lock().unwrap() = path.is_file();
                Ok(())
            }
        }

        let dir = scratch("order");
        let picture = a_picture(&dir, "src.png");
        let setter = Checking(Mutex::new(false));
        apply_desktop(&picture, &monitors(), &dir, "fill", &setter).unwrap();
        assert!(*setter.0.lock().unwrap(), "the setter saw no file on disk");
    }

    #[test]
    fn an_unreadable_source_is_an_invalid_error_not_a_panic() {
        let dir = scratch("badsrc");
        std::fs::write(dir.join("broken.png"), b"not a png").unwrap();
        let setter = FakeSetter::default();
        let err =
            apply_desktop(&dir.join("broken.png"), &monitors(), &dir, "fill", &setter).unwrap_err();
        assert_eq!(err.kind(), crate::ErrorKind::Invalid);
        assert_eq!(setter.count(), 0);
    }

    #[test]
    fn applying_a_collage_without_monitors_is_rejected() {
        let dir = scratch("nomon");
        let setter = FakeSetter::default();
        let err = apply_collage(&serde_json::json!({}), &[], &dir, None, &setter).unwrap_err();
        assert_eq!(err.kind(), crate::ErrorKind::Invalid);
    }
}
