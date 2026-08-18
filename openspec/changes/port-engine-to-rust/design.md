## Context

`src/wallpaper_changer/` is 6,641 lines of Python across 19 modules, frozen by PyInstaller and shipped as a Tauri bundle resource. `desktop/src-tauri/src/engine.rs` spawns it with piped stdio and `CREATE_NO_WINDOW`, correlates requests by `u64` id through a `HashMap<u64, oneshot::Sender>`, and enforces a 120s `CALL_TIMEOUT`. A single Tauri command, `engine_call(method, params)`, is the webview's only route in; method names are deliberately *not* validated in Rust because `Engine._METHODS` in `rpc.py` is meant to be the single gate.

This design covers replacing that engine with a native Rust core, in-process.

## Goals / Non-Goals

**Goals:**
- One process, one toolchain, ~15 MB installer.
- The app builds and runs at every commit of the migration.
- Byte-identical collage geometry and on-disk state formats; existing user files keep working untouched.
- A test safety net that outlives the Python it was derived from.

**Non-Goals:**
- New features, frontend changes, or altering the RPC contract.
- Bit-exact reproduction of Pillow's LANCZOS resampler.
- Eliminating the base64 image round-trip (a follow-on, not this change).

## Target Architecture

```
desktop/src-tauri/
├── Cargo.toml                  # workspace root
├── src/                        # lib.rs, engine.rs, hotkeys.rs, tray.rs — unchanged shape
└── crates/
    ├── wallpaper-core/         # the port target: config, imaging, win32, video
    └── wallpaper-core-cli/     # ~80-line stdio binary for conformance tests + manual A/B
```

`wallpaper-core` takes **no `tauri` dependency**, so it stays testable headless. `Engine::call` dispatches into it directly; the sidecar remains reachable only for methods not yet ported, and disappears at phase 8.

Two release-profile changes in `desktop/src-tauri/Cargo.toml` are prerequisites, not polish:

- **`panic = "abort"` must become `unwind`**, and `Core::dispatch` must wrap in `std::panic::catch_unwind` mapping to `{"type": "internal"}`. This reproduces `_error_payload`'s catch-all in `rpc.py`, which is load-bearing: a corrupt image today returns an error envelope, it does not kill the engine.
- **`strip = true`** makes any dxgi crash dump unreadable. Keep a separate PDB before phase 7.

## The Strangler Seam

The seam belongs in **`Engine::call`**, not `lib.rs`. `Engine::call` is already the sole choke point — `lib.rs:91` (hotkey config), `lib.rs:184` (startup visibility), `tray.rs:97`, `hotkeys.rs:188`, and `engine_call` from the webview all funnel through it. Cut there and every caller migrates for free.

```rust
// crates/wallpaper-core/src/lib.rs
pub enum Dispatch {
    Handled(Result<Value, CoreError>),
    NotPorted,                       // fall through to the sidecar
}

pub trait EventSink: Send + Sync {
    fn emit(&self, event: &str, data: Value);
}

impl Core {
    pub async fn dispatch(&self, method: &str, params: &Value) -> Dispatch { /* match */ }
}
```

```rust
// engine.rs
pub async fn call(&self, method: &str, params: Value) -> Result<Value, String> {
    match self.core.dispatch(method, &params).await {
        Dispatch::Handled(r) => r.map_err(|e| format!("{}: {}", e.kind(), e)),
        Dispatch::NotPorted => match &self.sidecar {
            Some(s) => s.call_sidecar(method, params).await,
            None    => Err(format!("unknown_method: Unknown method: {method}")),
        },
    }
}
```

Three details that make or break this:

1. **`NotPorted` must be its own variant, never an `Err`.** If "not ported" is encoded as an error string, a genuine Rust-side failure silently re-executes in Python — and for `apply_wallpaper` that means composing and setting the wallpaper twice.
2. **Error strings must match `dispatch_line` byte-for-byte**: `format!("{kind}: {message}")`. `desktop/src/lib/engine.ts` parses these.
3. **Events keep the same envelope.** The Tauri `EventSink` impl emits `ENGINE_EVENT` as `json!({"event": name, "data": data})` — identical to what `dispatch_line` forwards today, so the webview cannot tell which side produced an event.

Security is unchanged. `engine_call`'s comment says method validation lives in `Engine._METHODS` so there is one gate; in-process, the gate becomes the `match` arm list in `Core::dispatch` — still one place, and now exhaustive by construction.

### Migrate in state-ownership units

`Engine`'s state is shared across methods and cannot straddle two processes:

| Unit | Methods | Shared state |
|---|---|---|
| Stateless leaves | ping, get_translations, list_folder_images, get_thumbnails, get_image_preview, scan_videos, suggest_collage_path, list_saved_collages, forget_saved_collage | none |
| Config | get_config, save_config, get_capabilities, get/set_startup_enabled, notify | `_cfg` cache |
| Apply/history | apply_*, set_effect, preview, save_collage, watch_* | `_apply_lock`, `_history`, `_watch_timer` |
| Transparency | list_windows, set_window_opacity, get_foreground_window, toggle_foreground_opacity, get/save/reapply_opacity_settings | `transparency.json` |
| Scroll | sync_scroll_transparency, scroll_transparency_status | hook + debounce |
| Video | video_*, restore_session, shutdown | `VideoWallpaperPlayer` |

Two cross-unit couplings to plan around:

- **Config cache staleness.** Once Rust owns `save_config`, Python's `Engine._config()` cache goes stale for still-Python methods. Add a `_reload_config` method to `Engine._METHODS` (~5 lines in `rpc.py`) that clears `self._cfg`; Rust calls it after each write. Legitimate — we own the Python during migration.
- **`save_config` folds in live session state.** `rpc.py:135` overwrites `general.rotation_active` from `watch_status()` and `video.enabled` from `_video.is_running()`, so the config unit depends on the apply and video units. While those are still Python, Rust's `save_config` must round-trip `watch_status` and `video_status` to the sidecar before writing. Two extra calls, fails safe, drops out at phase 7.

Before Rust methods that need config exist, a ported method may simply call `self.call_sidecar("get_config", ...)`. One round trip, correctness preserved, deleted later.

## Risks

### DPI coordinate space — check this in phase 1, before anything is built on it

The PyInstaller spec has **no DPI manifest** (verified), so `screeninfo` receives *virtualized* coordinates. The Tauri process is per-monitor-DPI-v2 aware and receives *physical* ones. On a 4K display at 150% scaling that is 2560x1440 versus 3840x2160 — **the composite resolution can change without a single line of layout code changing.**

Test on a fractionally-scaled display, and confirm `Monitor.index` ordering matches `screeninfo`'s on a 3-monitor setup; a reshuffle silently moves which pictures land on which screen.

### Two silent data-loss traps

- **`std::fs::canonicalize` returns `\\?\C:\...`** where Python's `Path.resolve()` returns `C:\...`. `image_utils.py:84` builds the selection-history key as `str(Path(folder).resolve()) + ":random_history"`. Using the Rust default **silently resets every user's rotation history**. Use `dunce::canonicalize` and assert the produced key against a hardcoded literal.
- **`image`'s `grayscale` is Rec. 709** (0.2126/0.7152/0.0722) where Pillow's `ImageOps.grayscale` is `convert("L")` = **ITU-R 601-2** (`R*299 + G*587 + B*114`, /1000, truncating). Using it makes `bw` and `vintage` visibly wrong. Hand-roll 601-2.

### Behaviours that must survive literally

- **Non-blocking locks.** `rpc.py:258` uses `acquire(blocking=False)` — a second concurrent apply returns error type `busy`, it does not queue. Use `tokio::sync::Mutex::try_lock()`, never `.lock().await`. Composite inside `spawn_blocking` while holding the guard.
- **The rotation timer re-arms after the tick completes** (`rpc.py:635`), so the effective period is interval + work time, not a fixed rate. `tokio::time::interval` changes this.
- **`SPIF_UPDATEINIFILE` only.** `SPIF_SENDWININICHANGE` is omitted on purpose (`wallpaper.py:52`) to suppress Explorer's own crossfade.
- **Collage grid arithmetic is integer-truncating**: the `{1:1, 2:2, 3:2, 4:2, 5:3, 6:3, 7:4, 8:4, 9:3}` column table with a `ceil(sqrt(n))` fallback, `h // rows` leaving a black strip at the bottom, and a centred short last row. Port literally.
- **History**: 50 cap with pop-from-front, `del self._history[idx+1:]` truncation on push, `no_history` when `idx <= 0`.

### Pillow effect semantics

`image` has no equivalent of `ImageEnhance`'s lerp-against-a-degenerate-image. Hand-roll, ~60 lines against an exact spec:

- `Color(f)` = `blend(gray_as_rgb, im, f)`
- `Contrast(f)` = flat image of `int(mean_of_L_histogram + 0.5)`, then `blend(flat, im, f)`
- `Sharpness(f)` = `blend(im.filter(SMOOTH), im, f)`, SMOOTH = `[1,1,1,1,5,1,1,1,1] / 13`
- `colorize(gray, black, white)` = per-channel linear ramp LUT
- `DETAIL` = `[0,-1,0,-1,10,-1,0,-1,0] / 6`, offset 0

**`Image.filter` with a 3x3 kernel does not process the 1-pixel border** — edge pixels are copied through unchanged. Convolving them lights up all four edges in the differential harness.

### Video layer — thread affinity bug to fix on the way across

`DestroyWindow` may only be called by the thread that created the window. Today the host windows are created on the `restore-session` daemon thread (`rpc.py:933`) and destroyed from the main thread, with `_destroy_window` swallowing the failure. **Own all host windows on one dedicated thread.**

Preserve verbatim: `_MPV_SAFE_VIDEO_OPTIONS` (`vo=gpu`, `gpu_api=d3d11`, `gpu_context=d3d11`, `hwdec=no`, `d3d11_output_format=rgba8`, `profile=fast` — these exist to dodge dxgi.dll access violations, do not "clean up"); **mpv terminate strictly before `DestroyWindow`**; the 100 ms settle after the undocumented `0x052C` to Progman; the Win10/Win11 dual WORKERW discovery with the Progman fallback; `InvalidateRect` + `UpdateWindow` on the parent after teardown; audio on the first instance only; `playlist_pos` navigation with wrap.

**Pre-approved bail-out, no re-planning needed:** extract a ~400-line `wallpaper-video-host.exe` that receives monitor rects and a playlist on stdin, owns WORKERW discovery and libmpv, and exits on EOF, supervised with auto-restart. Keeps direct dispatch for the other 43 methods and keeps the 145 MB deleted.

## Crate Selection

| Need | Choice | Note |
|---|---|---|
| TOML | `toml_edit` | Preserves comments and unknown keys — a deliberate behaviour change, see below |
| Images | `image` 0.25 | Decode jpg/png/bmp/webp, encode BMP, `imageops::resize` Lanczos3 |
| Effects | hand-rolled | `image` has no `ImageEnhance` equivalent; ~60 lines |
| Win32 | `windows`, pinned to Tauri's resolved 0.61.3 | Confirm with `cargo tree -i windows`. **Do not add `windows-sys` alongside** — two incompatible `HWND` newtypes means casting at every boundary |
| Monitors | `EnumDisplayMonitors` + `GetMonitorInfoW` | Not Tauri's `available_monitors`: it drags `tauri` into the core, needs an `AppHandle`, and its coordinate space is not guaranteed to be the raw virtual-screen space the layout depends on |
| libmpv | `libloading` + hand-rolled FFI | See below |
| Toasts | `tauri-plugin-notification` | Already a dependency at `lib.rs:242`. Drops the `powershell.exe -EncodedCommand` subprocess-per-toast |
| Misc | `rand`, `base64` 0.22, `dunce`, `tokio-util`, `chrono` | `dunce` is load-bearing — see the canonicalize trap above |

**`toml_edit` changes observable `save_config` behaviour**, in three ways, all improvements: comments survive (today the hand-rolled writer destroys them), unknown and scalar top-level keys survive (today `if not isinstance(values, dict): continue` silently drops them), and any test asserting exact file bytes needs updating. Nothing depends on "config is normalized on save" — take the win. Must reproduce: the `startswith("_")` skip so `_config_path` is never written, and the atomic write (`mkstemp` -> `fsync` -> `os.replace` becomes tempfile -> `File::sync_all` -> `fs::rename` on the same volume).

**libmpv: hand-roll the FFI.** The `libmpv` crate is unmaintained and on the 1.x API; `libmpv2` is the maintained fork but is thin, Linux-exercised, and the `wid` embedding path on Windows is not its tested ground. Decisively, **both link at build time**, whereas we need *runtime* loading of the vendored `libmpv/libmpv-2.dll` from a path we choose, with graceful degradation when absent — exactly what `_prepare_libmpv`'s `%PATH%` hack exists to fake. `LoadLibraryW` on an absolute path is cleaner than what is there now. About 10 symbols (`mpv_create`, `mpv_set_option_string`, `mpv_initialize`, `mpv_command`, `mpv_set_property_string`, `mpv_set_property`, `mpv_get_property`, `mpv_terminate_destroy`, plus `mpv_request_log_messages` / `mpv_wait_event` for logging), ~150 lines of safe wrapper — smaller than the risk surface of an unmaintained crate on the one code path that already produces access violations. `wid` is set as a *string* option before `mpv_initialize`.

## Test Strategy

The 166 pytest tests are **not portable and should not be ported wholesale**. `test_rpc.py` (969 lines, 76 tests) mocks at `patch("wallpaper_changer.rpc.apply_wallpaper")` — it tests a seam between two Python modules, and that seam ceases to exist. Keep the suite running against Python, unchanged, for as long as Python is in the build. Build two new things alongside.

**Minimum viable safety net = the conformance corpus + the phase-3 differential harness.** Everything else is optional.

### 1. Protocol conformance corpus (phase 0.5 — the only artifact that outlives the Python)

Roughly 40 of the 76 `test_rpc.py` tests assert only on the JSON envelope: method exists, error `type` for bad params, event emitted, field names present. Extract them into language-neutral `tests/conformance/NNN-name.json` files (`{setup, request, expect}`), driven by a small runner speaking the stdio protocol. Run against **both** the Python sidecar and `wallpaper-core-cli`; both must pass. When Python goes, the corpus stays and becomes the Rust regression suite. Highest-leverage test investment in the plan.

### 2. Differential / golden-image harness (phase 3 — transitional tool, permanent output)

~20 fixture images x 5 fit modes x 4 effects x {1,2,3}-monitor layouts x collage counts 1-9, with `preset_images` fixed so selection is deterministic. **Decompose into three comparisons, not one** — conflating them means you cannot tell which you are looking at:

| Comparison | Tolerance |
|---|---|
| `plan_collage` JSON | **byte-identical.** Pure integer arithmetic; any diff is a bug. Covers the grid table and fit-mode off-by-ones on its own |
| `apply_effect` alone | **<=1.** No resampling involved; larger means wrong luma coefficients or border handling |
| `fit_image` alone | 1-3 expected. >8 means an off-by-one in the crop |

Run once, **freeze the Python outputs as golden PNGs committed to the repo**, then delete the Python. The harness is disposable; the goldens are permanent Rust regression tests.

### 3. Port test intent to Rust units (~1 day per phase)

Worth it only for hand-written integer arithmetic and pure logic: `_compute_grid_layout`, `fit_image` offsets, `pick_images` no-repeat cycling, history push/back-index, `next_alpha`, `normalize_modifier`, and all 19 of `test_config_paths.py` (they translate near line-for-line).

**Do not** port the tests that mock ctypes to assert a Win32 call happened. Express those as traits (`WallpaperSetter`, `DesktopLayer`, `OpacityStore`) with fakes — a better test than the Python one.

## Migration Sequence

Ordered by risk and dependency, lowest first. Each phase is a commit or small stack on `feat/rust-engine-port`; the app builds and runs at every one.

| Phase | Scope | Days |
|---|---|---|
| 0 | Delete dead Python (`gui.py`, `transparency_gui.py`, `hotkeys.py`) | 1-2 |
| 0.5 | Scaffold crates + seam + conformance corpus | 2.5-3.5 |
| 1 | Stateless leaves, i18n, monitors — **includes the DPI check** | 2-3 |
| 2 | Config, startup, notifications | 2-3 |
| 3 | Composition + differential harness | 5-8 |
| 4 | Apply, rotation, history | 3-4 |
| 5 | Transparency | 2-3 |
| 6 | Scroll hook | 2-3 |
| 7 | Video / mpv / WORKERW | 5-8 |
| 8 | Delete sidecar, port CLI to clap, packaging | 3-5 |
| | **Total** | **27.5-42.5** |
