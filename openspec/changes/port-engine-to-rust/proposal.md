## Why

The app ships as two processes speaking newline-delimited JSON over stdio: a Tauri v2 shell (1,082 lines of Rust) and a Python 3.11 engine (6,641 lines) frozen by PyInstaller into a **145 MB** one-dir bundle shipped as a Tauri `resources` entry.

That split no longer earns its cost:

- **The 145 MB bundle is ~90% of the download.** A Rust core brings the installer to roughly 15 MB.
- **The process isolation is nominal.** `engine.rs` has no restart logic, so a sidecar crash leaves a live window whose every call fails `"engine stopped before answering"` *and* orphaned WORKERW child windows, because the graceful teardown never ran. The UI surviving is not a working app; it is a shell that can only be closed.
- **`SHUTDOWN_GRACE` is a cost of the sidecar, not a reason for it.** In-process, teardown runs in `RunEvent::Exit` on the process that owns the HWNDs — no pipe to close, no 5s poll, no `child.kill()` path that strands windows on the desktop.
- **Roughly 2,441 lines of the Python are already dead** (`gui.py`, `transparency_gui.py`, `hotkeys.py`, all superseded by `hotkeys.rs` / `tray.rs` / the React UI), and another 1,006 (`i18n.py`) are pure string data. Actual engine logic to port is **~3,590 lines**.
- **One toolchain.** No `uv`, no PyInstaller, no `build_engine.ps1`, no frozen-binary smoke probe in the release path.

## What Changes

- **New `wallpaper-core` crate** under `desktop/src-tauri/crates/`, with no `tauri` dependency so it stays testable headless. It becomes the owner of config, image composition, the Win32 surface, and the video layer.
- **Strangler seam in `Engine::call`** (`desktop/src-tauri/src/engine.rs`): a `Dispatch::Handled | Dispatch::NotPorted` enum routes ported methods into Rust and falls through to the Python sidecar for the rest. The app builds and runs at every commit; the frontend never changes.
- **Migration in state-ownership units, not per method** — `Engine`'s state (`_apply_lock`, `_history`, `_watch_timer`, the config cache, the video player) is shared across methods and cannot straddle two processes.
- **Delete all Python**, including the legacy ttkbootstrap GUIs and `cli.py`. The CLI returns as a `clap`-based mode of the Tauri binary.
- **Delete the sidecar plumbing**: `engine_command`'s three resolution branches, `parse_engine_override`, the `resources` bundle entry, both PyInstaller specs, and `scripts/build_engine.ps1`.
- **New test artifacts** replacing the 166 pytest tests: a language-neutral protocol conformance corpus and a set of frozen golden images.

## Non-Goals

- **No new features.** This is parity-only. The RPC method set, error envelopes, event names, and on-disk file formats stay as they are.
- **No frontend changes.** `desktop/src/` is untouched; the webview cannot tell which side answered a call.
- **No change to user file locations.** `%APPDATA%\WallpaperChanger` and `%LOCALAPPDATA%\WallpaperChanger` keep their layout, and existing `state.json` / `gallery.json` / `transparency.json` must keep working unmodified.
- **Not eliminating the base64 image round-trip.** A `wallpaper://` asset protocol is a natural follow-on once the core is Rust, but it is out of scope here.

## Decisions Taken

| Question | Decision |
|---|---|
| Architecture | In-process `wallpaper-core` crate. No sidecar. |
| Deletion scope | All Python, including `cli.py`. Repo becomes Rust + TypeScript. |
| Video | Attempt the full mpv/WORKERW port in Rust; pre-approved bail-out to a ~20 MB video-only sidecar. |
| Pixel parity | Geometry byte-identical, effects ≤1 channel delta, resampling drift accepted. |

**Accepted tradeoff:** a native access violation inside libmpv/D3D11 will now take down the tray, hotkeys and window rather than just the engine. That is the one thing the sidecar genuinely provided, and the phase 7 fallback exists to buy it back if needed.

## Capabilities

### New Capabilities
- `rust-engine`: A native Rust core owning configuration, image composition, the Windows API surface, and the video wallpaper layer, replacing the Python sidecar in-process.

### Removed Capabilities
- The Python engine (`src/wallpaper_changer/`), its stdio RPC transport, and the PyInstaller build pipeline.

## Effort

**27.5–42.5 focused days (~6–9 calendar weeks).** Phases 3 (composition) and 7 (video) carry nearly all the variance; phases 0–2 are close to their lower bounds. Assumes one developer comfortable with Rust but not a Win32 expert, and a multi-monitor Windows 11 machine with at least one fractionally-scaled display. Add 30% to phases 4–7 if Win32 is genuinely unfamiliar.

**Natural stopping points to bank value early:** after phase 2 the two largest Python files are gone (`gui.py` + `i18n.py`, 2,722 lines) for very little risk. After phase 6 roughly 90% of the Python is gone and a ~20 MB video-only sidecar could be kept indefinitely.
