//! Supervises the Python engine sidecar (`wallpaper_changer.rpc`).
//!
//! The sidecar owns everything that touches Win32: the wallpaper composition, the
//! WORKERW video layer, and window transparency. This module owns its lifetime and
//! speaks its newline-delimited JSON protocol.
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

pub struct Engine {
    stdin: Mutex<Option<ChildStdin>>,
    child: Mutex<Option<Child>>,
    pending: Pending,
    next_id: AtomicU64,
}

impl Engine {
    /// Spawn the sidecar and start the reader threads.
    pub fn spawn(app: &AppHandle) -> Result<Self, String> {
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
    pub async fn call(&self, method: &str, params: Value) -> Result<Value, String> {
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
    pub fn shutdown(&self) {
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
        let mut parts = raw.split_whitespace();
        let program = parts.next().ok_or("WALLPAPER_ENGINE_CMD is empty")?;
        let mut cmd = Command::new(program);
        cmd.args(parts);
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
