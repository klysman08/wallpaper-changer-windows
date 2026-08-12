//! System tray icon and menu, replacing the old pystray integration.
//!
//! The tray is what makes closing the window mean "keep running in the background"
//! rather than "quit" — the rotation timer and video wallpaper are supposed to
//! outlive the window.

use serde_json::json;
use tauri::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};

use crate::engine::Engine;

pub const TRAY_ID: &str = "main";

pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show window", true, None::<&str>)?;
    let apply = MenuItem::with_id(app, "apply", "Apply wallpaper now", true, None::<&str>)?;
    let next = MenuItem::with_id(app, "next", "Next wallpaper", true, None::<&str>)?;
    let prev = MenuItem::with_id(app, "prev", "Previous wallpaper", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &show,
            &PredefinedMenuItem::separator(app)?,
            &apply,
            &next,
            &prev,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    // A missing or mispackaged icon must not take the whole app down on startup: the
    // tray is still usable without one, and the window is the thing people need.
    let icon = app.default_window_icon().cloned();
    if icon.is_none() {
        log::warn!("no default window icon; the tray will fall back to the platform default");
    }

    let mut builder = TrayIconBuilder::with_id(TRAY_ID);
    if let Some(icon) = icon {
        builder = builder.icon(icon);
    }

    builder
        .tooltip("Wallpaper Changer")
        .menu(&menu)
        // Left click should open the window; without this the icon only responds
        // to the menu, which people reliably find confusing.
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_window(tray.app_handle());
            }
        })
        .on_menu_event(on_menu_event)
        .build(app)?;
    Ok(())
}

fn on_menu_event<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    match event.id().as_ref() {
        "show" => show_window(app),
        "quit" => {
            // Let the engine tear down its WORKERW video windows before we go.
            if let Some(engine) = app.try_state::<Engine>() {
                engine.shutdown();
            }
            app.exit(0);
        }
        "apply" | "next" => call_engine(app, "apply_wallpaper"),
        "prev" => call_engine(app, "apply_previous_wallpaper"),
        _ => {}
    }
}

fn call_engine<R: Runtime>(app: &AppHandle<R>, method: &'static str) {
    // Resolve the engine inside the task: `Engine` is not cloneable, and the handle
    // is what we can legitimately move across threads.
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(engine) = app.try_state::<Engine>() else {
            return;
        };
        if let Err(e) = engine.call(method, json!({})).await {
            log::info!("tray action {method} did not run: {e}");
        }
    });
}

fn show_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
