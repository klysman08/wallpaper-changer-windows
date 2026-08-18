//! Drives the protocol conformance corpus against any binary speaking the engine's
//! stdio protocol.
//!
//! The corpus at `tests/conformance/*.json` is language-neutral on purpose. Roughly
//! forty of the assertions in `tests/test_rpc.py` are about the envelope — the method
//! exists, the error `type` is right, the result carries these fields — and those are
//! the ones that must keep holding as the engine moves to Rust. `test_rpc.py` itself
//! cannot be reused: it patches `wallpaper_changer.rpc` internals, a seam that ceases
//! to exist. This runner replaces that half of it and outlives the Python.
//!
//! Two ways to run it:
//!
//! ```text
//! cargo test -p wallpaper-core-cli                     # against the Rust core
//! CONFORMANCE_ENGINE_CMD='["uv","run","--directory","<repo>","wallpaper-changer-rpc"]' \
//!   CONFORMANCE_STRICT=1 cargo test -p wallpaper-core-cli
//! ```
//!
//! Against the Rust core, a method that is not ported yet answers `unknown_method`
//! and the case is counted as skipped rather than failed, so the corpus goes green
//! progressively as the phases land. `CONFORMANCE_STRICT=1` forbids skips, which is
//! how the Python sidecar is held to the full corpus today.
//!
//! Every run gets its own `WALLPAPER_CHANGER_CONFIG_DIR` and `..._DATA_DIR`, so a case
//! that writes — `watch_start` persists `rotation_active` — cannot touch real user
//! files. This mirrors what `tests/conftest.py` does for the Python suite.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;

use serde_json::{json, Value};

/// Generous: a cold `uv run` has to resolve the environment before it answers.
const READY_TIMEOUT: Duration = Duration::from_secs(60);
/// Any single call in this corpus is metadata-only; none of them composite.
const CALL_TIMEOUT: Duration = Duration::from_secs(30);

// ── corpus model ─────────────────────────────────────────────────────────────

struct Case {
    area: String,
    name: String,
    steps: Vec<Step>,
}

struct Step {
    /// A method call, or `None` when `raw` carries a hand-written line instead.
    method: Option<String>,
    params: Value,
    /// A literal line to write, for malformed input and BOM handling.
    raw: Option<String>,
    expect: Value,
}

fn corpus_dir() -> PathBuf {
    // crates/wallpaper-core-cli -> crates -> src-tauri -> desktop -> repo root
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for _ in 0..4 {
        dir = dir.parent().expect("walked above the repo root").to_path_buf();
    }
    dir.join("tests").join("conformance")
}

fn load_corpus() -> Vec<Case> {
    let dir = corpus_dir();
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read corpus at {}: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort(); // the NNN- prefix orders the areas

    let mut cases = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file).expect("read corpus file");
        let doc: Value = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("{} is not valid JSON: {e}", file.display()));
        let area = doc["area"].as_str().unwrap_or("?").to_string();
        for case in doc["cases"].as_array().expect("cases must be an array") {
            let steps = case["steps"]
                .as_array()
                .expect("steps must be an array")
                .iter()
                .map(|s| Step {
                    method: s["method"].as_str().map(str::to_string),
                    params: s.get("params").cloned().unwrap_or(json!({})),
                    raw: s["raw"].as_str().map(str::to_string),
                    expect: s.get("expect").cloned().unwrap_or(json!({})),
                })
                .collect();
            cases.push(Case {
                area: area.clone(),
                name: case["name"].as_str().expect("case needs a name").to_string(),
                steps,
            });
        }
    }
    assert!(!cases.is_empty(), "corpus at {} is empty", dir.display());
    cases
}

// ── the engine under test ────────────────────────────────────────────────────

struct Engine {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    _config_dir: tempdir::TempDir,
    _data_dir: tempdir::TempDir,
}

impl Engine {
    fn start() -> Self {
        let (program, args) = target_command();
        let config_dir = tempdir::TempDir::new("wc-conf-cfg");
        let data_dir = tempdir::TempDir::new("wc-conf-data");

        let mut child = Command::new(&program)
            .args(&args)
            // Isolation: a case that persists settings must never reach %APPDATA%.
            .env("WALLPAPER_CHANGER_CONFIG_DIR", config_dir.path())
            .env("WALLPAPER_CHANGER_DATA_DIR", data_dir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| panic!("cannot start {program}: {e}"));

        let stdin = child.stdin.take().expect("stdin");
        let stdout = child.stdout.take().expect("stdout");
        let stderr = child.stderr.take().expect("stderr");

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if !line.trim().is_empty() && tx.send(line).is_err() {
                    break;
                }
            }
        });
        // Drain stderr so a chatty engine cannot fill the pipe and block.
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if !line.trim().is_empty() {
                    eprintln!("[engine] {line}");
                }
            }
        });

        Self { child, stdin, lines: rx, _config_dir: config_dir, _data_dir: data_dir }
    }

    fn next_line(&self, timeout: Duration) -> Result<Value, String> {
        match self.lines.recv_timeout(timeout) {
            Ok(line) => serde_json::from_str(&line)
                .map_err(|e| format!("engine emitted a non-JSON line ({e}): {line}")),
            Err(RecvTimeoutError::Timeout) => Err("engine went quiet".to_string()),
            Err(RecvTimeoutError::Disconnected) => Err("engine exited".to_string()),
        }
    }

    /// Read past unsolicited events to the next response carrying `id`.
    fn next_response(&self) -> Result<Value, String> {
        let deadline = std::time::Instant::now() + CALL_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err("no response before the deadline".to_string());
            }
            let value = self.next_line(remaining)?;
            if value.get("event").is_none() {
                return Ok(value);
            }
        }
    }

    fn write(&mut self, line: &str) -> Result<(), String> {
        writeln!(self.stdin, "{line}").map_err(|e| e.to_string())?;
        self.stdin.flush().map_err(|e| e.to_string())
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = writeln!(self.stdin, "{}", json!({ "id": 0, "method": "shutdown" }));
        let _ = self.stdin.flush();
        // Do not wait indefinitely on an engine that ignored it.
        for _ in 0..40 {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The binary to exercise: `CONFORMANCE_ENGINE_CMD` as a JSON array, else this
/// crate's own CLI.
fn target_command() -> (String, Vec<String>) {
    match std::env::var("CONFORMANCE_ENGINE_CMD") {
        Ok(raw) => {
            let parts: Vec<String> = serde_json::from_str(raw.trim())
                .expect("CONFORMANCE_ENGINE_CMD must be a JSON array of strings");
            let mut it = parts.into_iter();
            let program = it.next().expect("CONFORMANCE_ENGINE_CMD is empty");
            (program, it.collect())
        }
        Err(_) => (env!("CARGO_BIN_EXE_wallpaper-core-cli").to_string(), Vec::new()),
    }
}

fn strict() -> bool {
    std::env::var("CONFORMANCE_STRICT").is_ok_and(|v| v == "1")
}

// ── assertions ───────────────────────────────────────────────────────────────

/// Does `actual` contain everything `expected` specifies? Extra fields are fine —
/// the corpus pins the contract, not the whole payload.
fn contains(actual: &Value, expected: &Value, path: &str, problems: &mut Vec<String>) {
    match expected {
        Value::Object(fields) => {
            for (key, want) in fields {
                let child = format!("{path}.{key}");
                match actual.get(key) {
                    Some(got) => contains(got, want, &child, problems),
                    None => problems.push(format!("{child} is missing")),
                }
            }
        }
        _ if actual == expected => {}
        _ => problems.push(format!("{path}: expected {expected}, got {actual}")),
    }
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// `None` means the step passed; `Some(reason)` explains the failure.
fn check(response: &Value, expect: &Value) -> Option<String> {
    let mut problems = Vec::new();
    let ok = response.get("ok").and_then(Value::as_bool).unwrap_or(false);

    if let Some(want_ok) = expect.get("ok").and_then(Value::as_bool) {
        if ok != want_ok {
            let detail = if ok {
                format!("succeeded with {}", response.get("result").unwrap_or(&Value::Null))
            } else {
                format!("failed with {}", response.get("error").unwrap_or(&Value::Null))
            };
            problems.push(format!("expected ok={want_ok}, but it {detail}"));
        }
    }

    if let Some(want_type) = expect.get("error_type").and_then(Value::as_str) {
        let got = response.pointer("/error/type").and_then(Value::as_str);
        if got != Some(want_type) {
            problems.push(format!("expected error type {want_type:?}, got {got:?}"));
        }
    }

    let result = response.get("result").unwrap_or(&Value::Null);

    if let Some(want) = expect.get("result") {
        contains(result, want, "result", &mut problems);
    }

    if let Some(keys) = expect.get("result_keys").and_then(Value::as_array) {
        for key in keys.iter().filter_map(Value::as_str) {
            if result.get(key).is_none() {
                problems.push(format!("result.{key} is missing"));
            }
        }
    }

    if let Some(types) = expect.get("result_types").and_then(Value::as_object) {
        for (key, want) in types {
            let want = want.as_str().unwrap_or("?");
            match result.get(key) {
                Some(got) if type_name(got) == want => {}
                Some(got) => problems.push(format!(
                    "result.{key}: expected a {want}, got a {} ({got})",
                    type_name(got)
                )),
                None => problems.push(format!("result.{key} is missing")),
            }
        }
    }

    (!problems.is_empty()).then(|| problems.join("; "))
}

/// A method the target has not taken over yet. Only ever a skip, never a pass.
fn not_ported(response: &Value, expect: &Value) -> bool {
    let expected_unknown =
        expect.get("error_type").and_then(Value::as_str) == Some("unknown_method");
    !expected_unknown
        && response.pointer("/error/type").and_then(Value::as_str) == Some("unknown_method")
}

// ── the test ─────────────────────────────────────────────────────────────────

#[test]
fn corpus_holds_against_the_target_engine() {
    let cases = load_corpus();
    let mut engine = Engine::start();

    // `rpc.py` emits `ready` before it answers anything, so the shell can tell a slow
    // start from a dead one. It must be the first line on the channel.
    let first = engine.next_line(READY_TIMEOUT).expect("engine never became ready");
    assert_eq!(
        first.get("event").and_then(Value::as_str),
        Some("ready"),
        "the first line must be the ready event, got {first}"
    );

    let mut passed = 0usize;
    let mut skipped: Vec<String> = Vec::new();
    let mut failed: Vec<String> = Vec::new();
    let mut by_area: BTreeMap<String, (usize, usize)> = BTreeMap::new();

    let mut id = 1u64;
    'cases: for case in &cases {
        let label = format!("[{}] {}", case.area, case.name);
        for (index, step) in case.steps.iter().enumerate() {
            let line = match (&step.raw, &step.method) {
                (Some(raw), _) => raw.clone(),
                (None, Some(method)) => {
                    json!({ "id": id, "method": method, "params": step.params }).to_string()
                }
                (None, None) => panic!("{label}: step {index} has neither method nor raw"),
            };
            id += 1;

            if let Err(e) = engine.write(&line) {
                failed.push(format!("{label}: could not send step {index}: {e}"));
                continue 'cases;
            }

            let response = match engine.next_response() {
                Ok(v) => v,
                Err(e) => {
                    failed.push(format!("{label}: step {index}: {e}"));
                    continue 'cases;
                }
            };

            if not_ported(&response, &step.expect) {
                skipped.push(label.clone());
                continue 'cases;
            }

            if let Some(reason) = check(&response, &step.expect) {
                failed.push(format!("{label}: step {index}: {reason}"));
                continue 'cases;
            }
        }
        passed += 1;
        by_area.entry(case.area.clone()).or_default().0 += 1;
        by_area.entry(case.area.clone()).or_default().1 += 1;
    }

    let (program, _) = target_command();
    println!("\nconformance target: {program}");
    println!("  {passed} passed, {} skipped, {} failed", skipped.len(), failed.len());
    for (area, (count, _)) in &by_area {
        println!("    {area}: {count}");
    }
    if !skipped.is_empty() {
        println!("  not ported yet:");
        for name in &skipped {
            println!("    - {name}");
        }
    }

    assert!(failed.is_empty(), "\n{}", failed.join("\n"));

    if strict() {
        assert!(
            skipped.is_empty(),
            "CONFORMANCE_STRICT=1 forbids skips, but {} case(s) answered unknown_method:\n{}",
            skipped.len(),
            skipped.join("\n")
        );
    }
}

// ── a tiny temp-dir helper ───────────────────────────────────────────────────
//
// Not worth a dependency: the runner needs a unique directory that is removed on
// drop, and nothing else.
mod tempdir {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    pub struct TempDir(PathBuf);

    impl TempDir {
        pub fn new(prefix: &str) -> Self {
            let unique = format!(
                "{prefix}-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            );
            let path = std::env::temp_dir().join(unique);
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("create temp dir");
            Self(path)
        }

        pub fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
