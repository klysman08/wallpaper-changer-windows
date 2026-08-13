mod engine;
mod hotkeys;
mod tray;

use engine::Engine;
use serde_json::{json, Value};
use tauri::webview::PageLoadEvent;
use tauri::{AppHandle, Manager, RunEvent, State, WindowEvent};
use tauri_plugin_log::{Target, TargetKind};
use tauri_plugin_opener::OpenerExt;

/// The webview's single door to the Python engine.
///
/// `method` is not validated here on purpose — the engine keeps its own allowlist
/// (`Engine._METHODS`) and rejects anything else, so there is one place to maintain
/// rather than two that can drift apart.
#[tauri::command]
async fn engine_call(
    engine: State<'_, Engine>,
    method: String,
    params: Option<Value>,
) -> Result<Value, String> {
    engine
        .call(
            &method,
            params.unwrap_or_else(|| Value::Object(Default::default())),
        )
        .await
}

/// Re-read the bindings from the engine and re-register them.
///
/// The UI calls this after saving settings, so an edited shortcut takes effect
/// without a restart. Returns the bindings that could not be registered — usually
/// because another application already owns the combination.
#[tauri::command]
async fn reload_hotkeys(app: AppHandle) -> Result<Vec<String>, String> {
    let config = {
        let engine = app.try_state::<Engine>().ok_or("engine is not running")?;
        engine.call("get_config", json!({})).await?
    };
    let bindings = config
        .get("config")
        .and_then(|c| c.get("hotkeys"))
        .cloned()
        .unwrap_or_else(|| json!({}));
    Ok(hotkeys::register_all(&app, &bindings))
}

fn external_navigation_plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::<R>::new("external-navigation")
        .on_navigation(|webview, url| {
            let is_internal_host = matches!(
                url.host_str(),
                Some("localhost") | Some("127.0.0.1") | Some("tauri.localhost") | Some("::1")
            );

            let is_internal = url.scheme() == "tauri" || is_internal_host;

            if is_internal {
                return true;
            }

            let is_external_link = matches!(url.scheme(), "http" | "https" | "mailto" | "tel");

            if is_external_link {
                log::info!("opening external link in system browser: {}", url);
                let _ = webview.opener().open_url(url.as_str(), None::<&str>);
                return false;
            }

            true
        })
        .build()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::LogDir { file_name: None }),
                    Target::new(TargetKind::Webview),
                ])
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(external_navigation_plugin())
        .invoke_handler(tauri::generate_handler![engine_call, reload_hotkeys])
        .setup(|app| {
            match Engine::spawn(app.handle()) {
                Ok(engine) => {
                    app.manage(engine);
                    log::info!("wallpaper engine started");
                }
                // A missing engine must not block the window: the UI renders and
                // reports the failure instead of the app dying before first paint.
                Err(e) => log::error!("could not start the wallpaper engine: {e}"),
            }

            if let Err(e) = tray::build(app.handle()) {
                log::error!("could not create the tray icon: {e}");
            }

            // Registering hotkeys needs the engine's config, so it has to wait for a
            // round trip; doing it inline would block the window from appearing.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                match reload_hotkeys(handle).await {
                    Ok(failed) if !failed.is_empty() => {
                        log::warn!(
                            "these hotkeys could not be registered: {}",
                            failed.join(", ")
                        )
                    }
                    Ok(_) => log::info!("global hotkeys registered"),
                    Err(e) => log::error!("could not register hotkeys: {e}"),
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing hides to the tray instead of quitting: the rotation timer and
            // the video wallpaper are meant to keep running. Quit is on the tray menu.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .on_page_load(|webview, payload| {
            if webview.label() == "main" && matches!(payload.event(), PageLoadEvent::Finished) {
                log::info!("main webview finished loading");
                let _ = webview.window().show();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app, event| {
            // Tear the engine down on the way out, whichever way we are exiting.
            // Skipping this strands the video host windows on the desktop layer.
            if matches!(event, RunEvent::ExitRequested { .. } | RunEvent::Exit) {
                if let Some(engine) = app.try_state::<Engine>() {
                    engine.shutdown();
                }
            }
        });
}
