//! The webview's route into the engine, and the seam the Rust port grows through.
//!
//! [`Engine::call`] asks [`wallpaper_core::Core`] first. Methods the core has taken
//! over are answered in-process; everything else returns [`Dispatch::NotPorted`] and
//! is forwarded to the Python sidecar (`wallpaper_changer.rpc`), which still owns the
//! rest of the Win32 surface — composition, the WORKERW video layer, transparency.
//!
//! This is the single choke point: `lib.rs`, `tray.rs`, `hotkeys.rs` and the
//! `engine_call` command all funnel through `call`, so a method migrates for every
//! caller at once. When the last method lands, the `sidecar` field goes away and the
//! `NotPorted` arm becomes the allowlist gate.
//!
//! The child is spawned from Rust rather than through `tauri-plugin-shell` on
//! purpose: the shell plugin exists so the *front end* can spawn processes, which is
//! attack surface we do not want. Here the webview can only reach the engine through
//! the `engine_call` command, and only with a method name the engine allowlists.
//!
//! Ids correlate requests to responses. Lines carrying `event` instead of `id` are
//! unsolicited engine events and are re-emitted to the webview.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::oneshot;
use wallpaper_core::{Core, Dispatch, EventSink};

/// Event name the webview listens on for unsolicited engine events.
pub const ENGINE_EVENT: &str = "engine-event";

/// A single call may take a while — compositing a 5760x2160 collage is not instant —
/// but it must never hang the UI forever if the sidecar wedges.
const CALL_TIMEOUT: Duration = Duration::from_secs(120);

/// How long the engine gets to tear down its WORKERW child windows before we kill it.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>;

/// Forwards core events to the webview in the sidecar's envelope.
///
/// `dispatch_line` re-emits a sidecar event object verbatim, so building the same
/// `{"event": ..., "data": ...}` shape here is what makes the two sides
/// indistinguishable to the front end.
struct WebviewSink {
    app: AppHandle,
}

impl EventSink for WebviewSink {
    fn emit(&self, event: &str, data: Value) {
        let _ = self
            .app
            .emit(ENGINE_EVENT, json!({ "event": event, "data": data }));
    }
}

/// Lets the core reach back into the methods the sidecar still owns.
///
/// `save_config` folds the live session flags into what it writes, and one of them —
/// whether a video wallpaper is playing — belongs to a unit that is still Python. It
/// also has to tell the sidecar to drop its cached configuration, since Python no
/// longer sees its own writes. Both disappear when the sidecar does.
struct SidecarBridge {
    sidecar: Arc<Sidecar>,
}

impl wallpaper_core::Bridge for SidecarBridge {
    fn call(&self, method: &str, params: Value) -> wallpaper_core::BridgeFuture {
        let sidecar = Arc::clone(&self.sidecar);
        let method = method.to_string();
        Box::pin(async move { sidecar.call(&method, params).await })
    }
}

/// The Python sidecar process and its protocol plumbing.
struct Sidecar {
    stdin: Mutex<Option<ChildStdin>>,
    child: Mutex<Option<Child>>,
    pending: Pending,
    next_id: AtomicU64,
}

pub struct Engine {
    core: Arc<Core>,
    /// `None` once the port is complete. Until then, the fallback for every method
    /// the core has not taken over.
    sidecar: Option<Arc<Sidecar>>,
}

impl Engine {
    /// Build the core and start the sidecar that still backs the unported methods.
    pub fn spawn(app: &AppHandle) -> Result<Self, String> {
        let core = Arc::new(Core::new(Arc::new(WebviewSink { app: app.clone() })));
        // While nothing is ported, a sidecar that will not start means no working
        // engine at all, so the failure is still fatal. As methods migrate this can
        // soften to a warning: the ported half would keep working without it.
        let sidecar = Arc::new(Sidecar::spawn(app)?);
        core.set_bridge(Arc::new(SidecarBridge {
            sidecar: Arc::clone(&sidecar),
        }));

        // The rotation timer belongs to the core now, so the sidecar's own
        // `restore_session` no longer brings it back — it would start a second,
        // invisible timer in the wrong process. Its video half still runs over there.
        let restoring = Arc::clone(&core);
        tauri::async_runtime::spawn(async move {
            if restoring.session().restore_rotation().await {
                log::info!("rotation restored from the saved session");
            }
        });

        Ok(Self {
            core,
            sidecar: Some(sidecar),
        })
    }

    /// Answer a request from the core, or forward it to the sidecar.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        match self.core.dispatch(method, &params).await {
            // The error string must match `dispatch_line`'s exactly — the front end
            // reads the "{kind}: {message}" prefix, and it must not be able to tell
            // which side produced the failure.
            Dispatch::Handled(result) => result.map_err(|e| format!("{}: {}", e.kind(), e)),
            Dispatch::NotPorted => match &self.sidecar {
                Some(sidecar) => sidecar.call(method, params).await,
                None => Err(format!("unknown_method: Unknown method: {method}")),
            },
        }
    }

    /// Ask the engine to shut down, then make sure the process is gone.
    pub fn shutdown(&self) {
        if let Some(sidecar) = &self.sidecar {
            sidecar.shutdown();
        }
    }
}

impl Sidecar {
    /// Spawn the sidecar and start the reader threads.
    fn spawn(app: &AppHandle) -> Result<Self, String> {
        let mut command = engine_command(app)?;
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = command
            .spawn()
            .map_err(|e| format!("failed to start the wallpaper engine: {e}"))?;

        let stdin = child.stdin.take().ok_or("engine stdin unavailable")?;
        let stdout = child.stdout.take().ok_or("engine stdout unavailable")?;
        let stderr = child.stderr.take().ok_or("engine stderr unavailable")?;

        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));

        // ── stdout: the protocol channel ──────────────────────────────────────
        let reader_pending = Arc::clone(&pending);
        let reader_app = app.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                match line {
                    Ok(line) if !line.trim().is_empty() => {
                        dispatch_line(&line, &reader_pending, &reader_app);
                    }
                    Ok(_) => {}
                    Err(e) => {
                        log::warn!("engine stdout closed: {e}");
                        break;
                    }
                }
            }
            // The engine died or closed the pipe. Nothing will ever answer the
            // in-flight calls, so fail them instead of leaving callers hanging.
            let mut map = reader_pending.lock().unwrap();
            for (_, tx) in map.drain() {
                let _ = tx.send(Err("engine stopped before answering".into()));
            }
            let _ = reader_app.emit(ENGINE_EVENT, json!({ "event": "stopped", "data": {} }));
        });

        // ── stderr: logs only, never protocol ─────────────────────────────────
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if !line.trim().is_empty() {
                    log::info!("[engine] {line}");
                }
            }
        });

        Ok(Self {
            stdin: Mutex::new(Some(stdin)),
            child: Mutex::new(Some(child)),
            pending,
            next_id: AtomicU64::new(1),
        })
    }

    /// Send a request and await its response.
    async fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);

        let request = json!({ "id": id, "method": method, "params": params });
        if let Err(e) = self.write_line(&request.to_string()) {
            self.pending.lock().unwrap().remove(&id);
            return Err(e);
        }

        match tokio::time::timeout(CALL_TIMEOUT, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err("engine dropped the response channel".into()),
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                Err(format!("engine call '{method}' timed out"))
            }
        }
    }

    fn write_line(&self, line: &str) -> Result<(), String> {
        let mut guard = self.stdin.lock().unwrap();
        let stdin = guard.as_mut().ok_or("engine stdin is closed")?;
        writeln!(stdin, "{line}").map_err(|e| format!("failed to write to engine: {e}"))?;
        stdin
            .flush()
            .map_err(|e| format!("failed to flush engine stdin: {e}"))
    }

    /// Ask the engine to shut down, then make sure the process is gone.
    ///
    /// The graceful request matters: the engine parents its video host windows to
    /// WORKERW, and killing it outright strands them on the desktop until Explorer
    /// restarts. Closing stdin is what ends the engine's read loop.
    fn shutdown(&self) {
        let _ = self.write_line(&json!({ "id": 0, "method": "shutdown" }).to_string());
        *self.stdin.lock().unwrap() = None; // drop the pipe -> engine's loop ends

        let mut guard = self.child.lock().unwrap();
        let Some(child) = guard.as_mut() else { return };

        let deadline = std::time::Instant::now() + SHUTDOWN_GRACE;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Ok(None) => {
                    log::warn!("engine ignored shutdown; killing it");
                    let _ = child.kill();
                    let _ = child.wait();
                    break;
                }
                Err(e) => {
                    log::warn!("could not wait on the engine: {e}");
                    let _ = child.kill();
                    break;
                }
            }
        }
        *guard = None;
    }
}

/// Route one protocol line to its waiting caller, or to the webview as an event.
fn dispatch_line(line: &str, pending: &Pending, app: &AppHandle) {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        log::warn!("engine emitted a non-JSON line: {line}");
        return;
    };

    if value.get("event").is_some() {
        let _ = app.emit(ENGINE_EVENT, &value);
        return;
    }

    let Some(id) = value.get("id").and_then(Value::as_u64) else {
        log::warn!("engine response without an id: {line}");
        return;
    };

    let Some(tx) = pending.lock().unwrap().remove(&id) else {
        // id 0 is the shutdown request, which nobody awaits.
        if id != 0 {
            log::warn!("engine answered unknown request id {id}");
        }
        return;
    };

    let result = if value.get("ok").and_then(Value::as_bool) == Some(true) {
        Ok(value.get("result").cloned().unwrap_or(Value::Null))
    } else {
        let err = value.get("error");
        let kind = err
            .and_then(|e| e.get("type"))
            .and_then(Value::as_str)
            .unwrap_or("error");
        let message = err
            .and_then(|e| e.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("unknown engine error");
        Err(format!("{kind}: {message}"))
    };
    let _ = tx.send(result);
}

/// Build the command that starts the engine.
///
/// Resolution order:
///   1. `WALLPAPER_ENGINE_CMD` — an explicit override, useful for debugging.
///   2. The bundled PyInstaller build, once the app is installed.
///   3. `uv run` against the repo checkout, so `tauri dev` needs no packaging step.
fn engine_command(app: &AppHandle) -> Result<Command, String> {
    if let Ok(raw) = std::env::var("WALLPAPER_ENGINE_CMD") {
        let (program, args) = parse_engine_override(&raw)?;
        let mut cmd = Command::new(program);
        cmd.args(args);
        log::info!("engine: using WALLPAPER_ENGINE_CMD override");
        return Ok(cmd);
    }

    if let Ok(bundled) = app.path().resolve(
        "engine/wallpaper-changer-rpc.exe",
        tauri::path::BaseDirectory::Resource,
    ) {
        if bundled.exists() {
            log::info!("engine: using bundled sidecar at {}", bundled.display());
            return Ok(Command::new(bundled));
        }
    }

    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(PathBuf::from)
        .ok_or("could not locate the repository root")?;
    if !repo_root.join("pyproject.toml").exists() {
        return Err(format!(
            "no bundled engine, and no checkout at {}",
            repo_root.display()
        ));
    }
    log::info!("engine: using `uv run` against {}", repo_root.display());
    let mut cmd = Command::new("uv");
    cmd.arg("run")
        .arg("--directory")
        .arg(&repo_root)
        .arg("wallpaper-changer-rpc");
    Ok(cmd)
}

/// Split `WALLPAPER_ENGINE_CMD` into a program and its arguments.
///
/// Two accepted forms, because splitting on whitespace would mangle the common case
/// of a path under `C:\Program Files\...`:
///
///   - a bare path, taken whole and run with no arguments:
///     `C:\Program Files\WallpaperChanger\engine\wallpaper-changer-rpc.exe`
///   - a JSON array when arguments are needed, which needs no quoting rules of its
///     own: `["uv", "run", "--directory", "C:\\src\\wallpaper", "wallpaper-changer-rpc"]`
fn parse_engine_override(raw: &str) -> Result<(String, Vec<String>), String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("WALLPAPER_ENGINE_CMD is empty".into());
    }

    if trimmed.starts_with('[') {
        let parts: Vec<String> = serde_json::from_str(trimmed)
            .map_err(|e| format!("WALLPAPER_ENGINE_CMD is not a valid JSON array: {e}"))?;
        let mut parts = parts.into_iter();
        let program = parts
            .next()
            .ok_or("WALLPAPER_ENGINE_CMD is an empty JSON array")?;
        return Ok((program, parts.collect()));
    }

    Ok((trimmed.to_string(), Vec::new()))
}

#[cfg(test)]
mod tests {
    use super::parse_engine_override;

    #[test]
    fn keeps_a_path_with_spaces_intact() {
        let raw = r"C:\Program Files\WallpaperChanger\engine\wallpaper-changer-rpc.exe";
        assert_eq!(parse_engine_override(raw).unwrap(), (raw.to_string(), vec![]));
    }

    #[test]
    fn reads_arguments_from_a_json_array() {
        let raw = r#"["uv", "run", "--directory", "C:\\src\\wp", "wallpaper-changer-rpc"]"#;
        let (program, args) = parse_engine_override(raw).unwrap();
        assert_eq!(program, "uv");
        assert_eq!(args, ["run", "--directory", r"C:\src\wp", "wallpaper-changer-rpc"]);
    }

    #[test]
    fn rejects_input_it_cannot_turn_into_a_command() {
        assert!(parse_engine_override("   ").is_err());
        assert!(parse_engine_override("[]").is_err());
        assert!(parse_engine_override("[not json]").is_err());
    }
}
