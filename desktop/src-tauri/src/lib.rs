mod engine;
mod hotkeys;
mod tray;

use std::sync::Mutex;
use std::time::Duration;

use engine::Engine;
use serde_json::{json, Value};
use tauri::webview::PageLoadEvent;
use tauri::{AppHandle, Manager, RunEvent, State, WindowEvent};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_log::{Target, TargetKind};
use tauri_plugin_opener::OpenerExt;

/// Passed to the app when *Windows* launches it, so a boot-time start goes straight
/// to the tray while a launch from the Start menu still opens the window.
const AUTOSTART_ARG: &str = "--minimized";

/// How long to wait for the webview before showing the window regardless.
///
/// WebView2 can fail to attach for reasons outside the app — a runtime mid-update
/// is one — and `on_page_load` then never fires. Without this the app would sit
/// there running, invisible, reachable only from the tray.
const WEBVIEW_GRACE: Duration = Duration::from_secs(10);

/// Whether the window should appear on this launch, and whether it can yet.
///
/// The two halves race: the webview finishes loading on its own schedule, and the
/// answer lives in the engine's config, which takes a round trip to read. Whichever
/// arrives second does the showing.
#[derive(Default)]
struct Startup {
    loaded: bool,
    /// `None` until the config has been consulted.
    show: Option<bool>,
}

impl Startup {
    /// Record a half and report whether the window should be shown now.
    fn resolve(state: &Mutex<Startup>, update: impl FnOnce(&mut Startup)) -> bool {
        let mut startup = match state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        update(&mut startup);
        startup.loaded && startup.show == Some(true)
    }

    /// Whether this launch wanted a window and the webview never delivered one.
    ///
    /// Deliberately leaves `loaded` alone: showing the window twice costs nothing,
    /// and a late-arriving webview should still take the normal path.
    fn stalled(state: &Mutex<Startup>) -> bool {
        let startup = match state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        !startup.loaded && startup.show == Some(true)
    }
}

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

/// Bring the Windows autostart entry in line with what this build expects.
///
/// Two jobs, and the marker file is what tells them apart:
///
///  - **first run** — turn autostart on. Running in the background is what the app is
///    for, so it is on by default; the Startup screen turns it off, and the marker
///    means that choice is never overridden on a later launch.
///  - **every run after** — the registry entry keeps whatever arguments it was
///    written with, so an install that enabled autostart before `--minimized` existed
///    would still be launched without it. Rewriting the entry brings it up to date.
///
/// The marker is written whatever the outcome: an environment that refuses the
/// registry write would otherwise be asked again on every single launch.
fn sync_autostart(app: &AppHandle) {
    let autostart = app.autolaunch();
    let marker = app
        .path()
        .app_config_dir()
        .map(|dir| dir.join("autostart-initialized"));

    // Never in a dev build: the entry would point at `target/debug`, and consuming
    // the marker here would rob the installed build of its own first run — the two
    // share a config directory because they share an identifier.
    let first_run = if cfg!(debug_assertions) {
        log::info!("dev build: leaving the autostart default alone");
        false
    } else {
        match &marker {
            Ok(path) => !path.exists(),
            // Without a writable config directory there is no way to remember the
            // decision, and repeatedly re-enabling would fight the user's own choice.
            Err(e) => {
                log::warn!("no config directory for the autostart marker: {e}");
                false
            }
        }
    };

    if first_run {
        if let Err(e) = autostart.enable() {
            log::warn!("could not enable autostart on first run: {e}");
        } else {
            log::info!("autostart enabled by default on first run");
        }
        if let Ok(path) = &marker {
            let written = path
                .parent()
                .map(std::fs::create_dir_all)
                .transpose()
                .and_then(|_| std::fs::write(path, ""));
            if let Err(e) = written {
                log::warn!("could not write the autostart marker: {e}");
            }
        }
        return;
    }

    if autostart.is_enabled().unwrap_or(false) {
        if let Err(e) = autostart.disable().and_then(|_| autostart.enable()) {
            log::warn!("could not refresh the autostart entry: {e}");
        }
    }
}

/// Work out whether this launch should open the window, and show it if the webview
/// is already waiting.
///
/// Hidden when Windows started us (the `--minimized` argument on the autostart
/// entry) or when the user asked for it with `general.start_minimized`. A failure to
/// read the config shows the window: an invisible app with no way back is far worse
/// than an unwanted one.
async fn decide_visibility(app: &AppHandle) {
    let from_autostart = std::env::args().any(|arg| arg == AUTOSTART_ARG);

    let start_minimized = if from_autostart {
        true
    } else {
        match app.try_state::<Engine>() {
            Some(engine) => engine
                .call("get_config", json!({}))
                .await
                .ok()
                .and_then(|c| {
                    c.get("config")?
                        .get("general")?
                        .get("start_minimized")?
                        .as_bool()
                })
                .unwrap_or(false),
            None => false,
        }
    };

    if start_minimized {
        log::info!("starting hidden in the tray");
    }
    let Some(state) = app.try_state::<Mutex<Startup>>() else {
        return;
    };
    if Startup::resolve(&state, |s| s.show = Some(!start_minimized)) {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
        }
    }
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
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .arg(AUTOSTART_ARG)
                .build(),
        )
        .plugin(tauri_plugin_notification::init())
        // The update check and the download both run here rather than in the
        // webview: the CSP allows no remote origin, and the installer has to land
        // on disk outside the sandbox anyway.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::LogDir { file_name: None }),
                    // The webview target emits `log://log` for the debug panel to
                    // mirror. That emit is an `eval` in the webview, and when the
                    // webview is gone — Ctrl-C, or any teardown — the eval fails and
                    // tauri_runtime_wry logs the failure, which comes straight back
                    // here and emits again: an unbounded feedback loop that fills the
                    // console with the same error until the process is killed. Cutting
                    // its own target out of this one destination breaks the cycle
                    // without hiding anything: stdout and the log file still get it.
                    Target::new(TargetKind::Webview)
                        .filter(|m| m.target() != "tauri_runtime_wry"),
                ])
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(external_navigation_plugin())
        .invoke_handler(tauri::generate_handler![engine_call, reload_hotkeys])
        .setup(|app| {
            app.manage(Mutex::new(Startup::default()));

            sync_autostart(app.handle());

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
                decide_visibility(&handle).await;
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

            // The window is shown when the webview reports in, so a webview that
            // never loads leaves the app running and invisible — the tray its only
            // door. That is the same trade `decide_visibility` already calls: an
            // unwanted window beats an app the user cannot reach. A blank window at
            // least says the app is alive and where the problem is.
            let watchdog = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(WEBVIEW_GRACE).await;
                let Some(state) = watchdog.try_state::<Mutex<Startup>>() else {
                    return;
                };
                if Startup::stalled(&state) {
                    log::warn!(
                        "the webview did not load within {}s — showing the window anyway",
                        WEBVIEW_GRACE.as_secs()
                    );
                    if let Some(window) = watchdog.get_webview_window("main") {
                        let _ = window.show();
                    }
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
                // Without the state there is nothing coordinating the two halves,
                // so show the window rather than risk hiding it forever.
                let show = match webview.try_state::<Mutex<Startup>>() {
                    Some(state) => Startup::resolve(&state, |s| s.loaded = true),
                    None => true,
                };
                if show {
                    let _ = webview.window().show();
                }
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
