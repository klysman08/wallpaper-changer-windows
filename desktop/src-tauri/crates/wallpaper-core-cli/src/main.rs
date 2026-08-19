//! Speaks the engine's newline-delimited JSON protocol against [`wallpaper_core`].
//!
//! This exists so the core can be exercised exactly the way the Python sidecar is:
//! the conformance corpus drives this binary and `wallpaper-changer-rpc` through the
//! same runner and asserts the same envelopes. It is also what `WALLPAPER_ENGINE_CMD`
//! can be pointed at for a manual A/B against Python.
//!
//! It is a test and development tool, not something the installer ships.
//!
//! Protocol, mirroring `rpc.py`'s `serve()`:
//!   - one JSON object per line on stdin: `{"id": N, "method": "...", "params": {...}}`
//!   - one JSON object per line on stdout: `{"id": N, "ok": true, "result": {...}}`
//!     or `{"id": N, "ok": false, "error": {"type": "...", "message": "..."}}`
//!   - unsolicited events carry `event`/`data` and no `id`
//!   - stderr is logs only, never protocol
//!
//! While methods remain unported, `Dispatch::NotPorted` is answered as
//! `unknown_method` — this binary has no sidecar to fall through to. That is the
//! same answer the shell will give once the sidecar is deleted for good.

use std::io::{BufRead, Write};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use wallpaper_core::{Core, CoreError, Dispatch, EventSink, PROTOCOL_VERSION};

/// Writes events to stdout as they happen, interleaved with responses.
///
/// stdout is shared with the response writer, so it is behind a mutex: a half-written
/// event line spliced into a response would corrupt the channel for the reader.
struct StdoutSink {
    out: Arc<Mutex<std::io::Stdout>>,
}

impl EventSink for StdoutSink {
    fn emit(&self, event: &str, data: Value) {
        let line = json!({ "event": event, "data": data });
        let mut out = self.out.lock().unwrap();
        let _ = writeln!(out, "{line}");
        let _ = out.flush();
    }
}

// Multi-threaded on purpose. The request loop below blocks on stdin, and the rotation
// timer is a spawned task: on a current-thread runtime it would never be polled while
// the engine sat waiting for the next line, and rotation would silently never fire.
#[tokio::main]
async fn main() {
    let out = Arc::new(Mutex::new(std::io::stdout()));
    let core = Core::new(Arc::new(StdoutSink { out: Arc::clone(&out) }));

    emit(&out, &json!({ "event": "ready", "data": { "protocol": PROTOCOL_VERSION } }));

    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        // `rpc.py` strips a UTF-8 BOM here; a client that writes one on the first
        // line would otherwise fail to parse and look like a protocol bug.
        let line = line.trim().trim_start_matches('\u{feff}');
        if line.is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<Value>(line) {
            Ok(request) => handle(&core, &request).await,
            Err(e) => failure(Value::Null, &CoreError::new(
                wallpaper_core::ErrorKind::Parse,
                format!("Invalid JSON: {e}"),
            )),
        };

        emit(&out, &response);

        if response.get("_shutdown").is_some() {
            break;
        }
    }
}

async fn handle(core: &Core, request: &Value) -> Value {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let Some(method) = request.get("method").and_then(Value::as_str) else {
        return failure(id, &CoreError::bad_params("Request has no method."));
    };
    let params = request.get("params").cloned().unwrap_or(json!({}));

    // `rpc.py` rejects a non-object `params` before dispatching, because it splats
    // them as keyword arguments.
    if !params.is_object() && !params.is_null() {
        return failure(id, &CoreError::bad_params("params must be an object."));
    }

    let mut response = match core.dispatch(method, &params).await {
        Dispatch::Handled(Ok(result)) => success(id, result),
        Dispatch::Handled(Err(e)) => failure(id, &e),
        // Nothing to fall through to here. Once the shell drops the sidecar it
        // answers the same way, so this is the end-state behaviour, early.
        Dispatch::NotPorted => failure(id, &CoreError::unknown_method(method)),
    };

    if method == "shutdown" {
        // A marker the writer strips; it must never reach the wire.
        response["_shutdown"] = Value::Bool(true);
    }
    response
}

fn success(id: Value, result: Value) -> Value {
    json!({ "id": id, "ok": true, "result": result })
}

fn failure(id: Value, error: &CoreError) -> Value {
    json!({ "id": id, "ok": false, "error": error.to_payload() })
}

fn emit(out: &Arc<Mutex<std::io::Stdout>>, value: &Value) {
    let mut value = value.clone();
    value.as_object_mut().map(|o| o.remove("_shutdown"));
    let mut out = out.lock().unwrap();
    let _ = writeln!(out, "{value}");
    let _ = out.flush();
}
