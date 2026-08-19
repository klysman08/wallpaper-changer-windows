## 0. Delete the dead weight (1-2 days)

Highest value/effort ratio in the plan. 2,441 lines, 37% of the Python, all superseded by `hotkeys.rs` / `tray.rs` / the React UI. No behaviour change.

- [x] 0.1 Delete `src/wallpaper_changer/gui.py` (1,716 lines), `transparency_gui.py` (329), `hotkeys.py` (396).
- [x] 0.2 Delete `tests/test_hotkeys.py`, `wallpaper_changer.spec`, and `main.py`.
- [x] 0.3 Drop `ttkbootstrap` and `pystray` from `pyproject.toml`, and the `wallpaper-changer-gui = gui:run` script entry.
- [x] 0.4 Fix `startup.py`'s non-frozen path, which currently registers `-m wallpaper_changer.gui` — a module that no longer exists. Now raises rather than writing a registry entry that launches nothing; the frozen path (the one that ships) is unchanged.
- [x] 0.5 Verify: `uv run pytest` green (176 passed, down from 187 — the 11 lost are `test_hotkeys.py`); engine smoke-tested over the real stdio protocol (`ping`, `get_capabilities`, `get_config`, `shutdown` all `ok: true`); `desktop/` untouched so the Rust build is unaffected.

**Outcome:** 2,655 lines deleted, 13 inserted. Ruff errors in `src/` fell from 205 to 99 — all 106 removed were in the deleted files; none were introduced.

## 1. Scaffold the seam and the conformance corpus (2.5-3.5 days)

- [x] 1.1 Convert `desktop/src-tauri/` into a Cargo workspace; add `crates/wallpaper-core` (no `tauri` dependency) and `crates/wallpaper-core-cli`.
- [x] 1.2 Define `Core`, `Dispatch::{Handled, NotPorted}`, `EventSink`, and `CoreError` with a `kind()` matching the Python error `type` vocabulary (`error`, `busy`, `invalid`, `no_history`, `not_configured`, `not_found`, `no_monitors`, `no_mpv`, `io`, `unknown_method`, `bad_params`, `parse`, `internal`).
- [x] 1.3 Change the release profile to `panic = "unwind"` and add `wallpaper_core::guard`, which turns a panic into `{"type": "internal"}`. `debug = "line-tables-only"` keeps a PDB; on MSVC that is a separate file, so the shipped `.exe` is unaffected.
- [x] 1.4 Wire the seam into `Engine::call` with every method returning `NotPorted`; sidecar logic moved behind a `Sidecar` struct in an `Option` field. `Engine::spawn`/`call`/`shutdown` kept their signatures, so `lib.rs`, `tray.rs` and `hotkeys.rs` needed no changes.
- [x] 1.5 Implement the Tauri `EventSink` (`WebviewSink`) emitting `ENGINE_EVENT` as `json!({"event": name, "data": data})`, byte-compatible with `dispatch_line`.
- [x] 1.6 Build `wallpaper-core-cli` speaking the identical newline-JSON stdio protocol, including the BOM strip and non-object-`params` rejection.
- [x] 1.7 Corpus at `tests/conformance/*.json` — 35 cases across 7 areas — with a Rust runner at `crates/wallpaper-core-cli/tests/conformance.rs` that drives any binary speaking the protocol. Each run gets its own `WALLPAPER_CHANGER_CONFIG_DIR`/`_DATA_DIR`, mirroring `conftest.py`.
- [x] 1.8 Verify: **35/35 pass against the Python sidecar under `CONFORMANCE_STRICT=1`**; against the Rust core 3 pass and 32 report as not-yet-ported. A Rust test asserts all 45 method names return `NotPorted`.

**Outcome:** 21 Rust tests green (9 pre-existing + 11 core + 1 conformance), `cargo check --workspace` clean with no warnings.

**Corpus scope, deliberately.** Cases are envelope-only: read-only calls, error paths, and no-op toggles. Nothing that composites, applies a wallpaper, starts video, or changes a real window's opacity — those change the user's desktop, and the golden-image harness in phase 4 is where composition gets pinned.

**Two design notes for later phases:**
- The runner counts `unknown_method` as *skipped*, not passed, so the corpus turns green progressively as methods land and doubles as a progress meter. `CONFORMANCE_STRICT=1` forbids skips and is how the Python side is held to the full corpus.
- Unknown method names return `NotPorted` rather than an error, so the Python allowlist stays the single gate while it exists. When the sidecar goes, the `None` arm in `Engine::call` already answers `unknown_method` — the fallthrough becomes the gate with no extra code.

## 2. Stateless leaves (2-3 days)

Lands: `ping`, `get_translations`, `list_folder_images`, `get_thumbnails`, `get_image_preview`, `scan_videos`, `suggest_collage_path`, `list_saved_collages`, `forget_saved_collage`.

- [x] 2.1 **The DPI check — and it is a non-issue.** `screeninfo` calls `SetProcessDpiAwareness(2)` inside its enumerator, so the Python engine reads *physical* coordinates too; measured awareness goes `0 → 0 → 2` across start / import / `get_monitors()`. Both sides are per-monitor aware when they read `rcMonitor`, so they agree at any scale factor. See design.md for the full finding.
- [x] 2.2 Port `monitor.py` to `EnumDisplayMonitors` + `GetMonitorInfoW`, `Monitor {index, x, y, width, height}` field-identical. **`get_monitors` output is byte-for-byte identical to Python's**, ordering and a negative-offset screen included.
- [ ] 2.3 ~~i18n~~ — **moved to phase 3.** `get_translations` reports `current`, which is the language from `general.language`; without the config port it cannot be answered without diverging.
- [ ] 2.4 ~~gallery~~ — **moved to phase 3.** `list_saved_collages` and `suggest_collage_path` resolve the library folder through `resolve_saved_dir(cfg)`. Splitting the unit would leave the index half-owned.
- [x] 2.5 Port `list_images` / `SUPPORTED` and `scan_video_folder` / `VIDEO_EXTENSIONS`. The asymmetry is deliberate and copied: images are **unsorted** and do not check `is_file()`, videos are **sorted** and do.
- [x] 2.6 `get_thumbnails` (clamp 32–512, JPEG q75, unreadable files dropped silently) and `get_image_preview` (clamp 64–4096, downscale only, q85), including Pillow's `round_aspect` thumbnail sizing and half-to-even rounding.
- [x] 2.7 Resampler drift measured — see below.
- [x] 2.8 Verify: 42 Rust tests green, `cargo check --workspace --all-targets` clean; corpus against the Rust core went 3 → **13 passing**; Python still **35/35 strict**.

**Methods landed (6):** `ping`, `get_monitors`, `list_folder_images`, `scan_videos`, `get_thumbnails`, `get_image_preview`.

**Resampler drift, `image` Lanczos3 vs Pillow LANCZOS** (900×600 source with a gradient, a 3px checkerboard and a fine sinusoid — deliberately hostile):

| Operation | max Δ | mean Δ | % channels > 1 |
|---|---|---|---|
| 900×600 → 450×300 (downscale) | **1** | 0.004 | 0.00% |
| 900×600 → 160×107 (downscale) | **1** | 0.002 | 0.00% |
| 900×600 → 1800×1200 (upscale) | **16** | 0.739 | 7.42% |

**This tightens phase 4's plan.** Downscaling is effectively identical, so the ≤3 tolerance the design assumed can be **≤1** for the common case — a wallpaper larger than its cell, and every thumbnail and preview. Upscaling is materially different and needs its own looser bound; it only occurs when a source image is smaller than the cell it fills. Split the `fit_image` comparison along that line rather than using one tolerance for both.

**Verified against real data:** `list_folder_images` over a 4948-image folder returned an identical set **and identical order** to Python, confirming that leaving the listing unsorted (as `list_images` does) reproduces `Path.iterdir()`.

## 3. Config, startup, notifications, i18n, gallery (3-4 days)

Lands: `get_config`, `save_config`, `get_capabilities`, `get_startup_enabled`, `set_startup_enabled`, `notify`, plus `get_translations` and the gallery reads (`suggest_collage_path`, `list_saved_collages`, `forget_saved_collage`) — both deferred from phase 2, because they resolve the library folder and the current language through the config and would diverge if split off from it.

- [x] 3.1 Path resolution ported: `user_config_dir`, `user_data_dir`, `resolve_output_dir`, `resolve_saved_dir`, `resolve_path`, and the `WALLPAPER_CHANGER_CONFIG_DIR` / `WALLPAPER_CHANGER_DATA_DIR` overrides.
- [x] 3.2 `migrate_legacy_files` ported — copy, never move; never overwrite.
- [x] 3.3 Read and write with `toml_edit`, reproducing the `startswith("_")` skip and the atomic write (sibling tempfile -> `sync_all` -> `rename`).
- [ ] 3.4 ~~`_reload_config` in `rpc.py`~~ — **not needed.** `Core::config()` reads the file fresh on every call instead of caching, so nothing can go stale while Python still writes it. The cache returns with `save_config`.
- [ ] 3.5 ~~round-trip the session flags to the sidecar~~ — **deferred with `save_config`,** see below.
- [x] 3.6 `startup.py` ported to the registry API; `notify` deferred, see below.
- [x] 3.7 Path and round-trip tests translated to Rust, including a load/save/load **fixpoint** property test.
- [x] 3.8 Writer validated against the **real shipped `settings.toml`**, embedded as a fixture: values identical across a save, all 16 comments retained. For contrast, Python's writer drops all 16 and shrinks the file from 1970 to 781 bytes.
- [x] 3.9 i18n exported to `assets/translations.json` (267 keys x 3 languages) and embedded with `include_str!`; `get_translations` serves it with `current` read from `general.language`.
- [x] 3.10 `gallery.py` ported: `entries()` with on-read disk reconciliation, `find`, `record`, `forget`, `suggest_name`, `library_dir`, and `normcase`-equivalent identity.

**Methods landed (7):** `get_config`, `get_translations`, `get_startup_enabled`, `set_startup_enabled`, `suggest_collage_path`, `list_saved_collages`, `forget_saved_collage`. Running total: **13 of 45**.

**Verified against Python, same seeded config directory, 7/7 byte-identical** — including `get_config` (the full nested structure), `get_translations` (40 KB across three languages) and `list_saved_collages` (with a vanished file pruned by both sides). 74 Rust tests green; corpus against the Rust core 13 -> **19**; Python still **35/35 strict** and **176 pytest**.

### Deferred out of phase 3, with reasons

- **`save_config` -> phase 5.** `rpc.py:135` folds `general.rotation_active` and `video.enabled` in from the live rotation timer and video player before writing. Those belong to units Python still owns, and the plan's workaround — round-tripping `watch_status`/`video_status` to the sidecar — needs a call path from `Core` back into the sidecar that does not exist. Cheaper and safer to land `save_config` with rotation in phase 5. Until then Python owns every write, and `Core::config()` stays uncached so it always sees them.
- **`get_capabilities` -> phase 8.** It aggregates three units at once: `startup.is_startup_enabled()` (ported), `has_mpv()` (phase 8) and `scroll_transparency.is_available()` (phase 7).
- **`notify` -> phase 9.** The plan's choice of `tauri-plugin-notification` cannot be called from `wallpaper-core`, which deliberately has no `tauri` dependency. It needs a `Notifier` trait alongside `EventSink`, which belongs with the shell wiring.

### Inherited bug found, not fixed

`startup.py` reads and writes the `Run` value **`WallpaperChanger`** (no space). The value the app actually uses is **`Wallpaper Changer`** (with a space), written by `tauri-plugin-autostart` and pointing at `tauri-native.exe --minimized`. So `get_startup_enabled` reports `false` on a machine where autostart is on, and `get_capabilities` carries that wrong answer; `set_startup_enabled` would add a *second* entry rather than change the real one. Nothing calls either method today — `engine.ts` declares them but the Settings screen uses the plugin. **Ported faithfully rather than fixed**, since changing it would add or remove a real autostart entry, which is beyond a parity port. Decide in phase 9 whether to point them at the plugin's value or drop them.

## 4. Composition (5-8 days — the risk concentrate)

Lands: `preview`, `save_collage`.

- [ ] 4.1 Port `_compute_grid_layout` and `plan_collage` literally: the `{1:1, 2:2, 3:2, 4:2, 5:3, 6:3, 7:4, 8:4, 9:3}` table with `ceil(sqrt(n))` fallback, `h // rows`, and the centred short last row.
- [ ] 4.2 Port `fit_image`'s five modes with integer-truncating arithmetic exactly (`int(src_w * ratio)`, `// 2` offsets); `span` aliases `fill`.
- [ ] 4.3 Port `pick_images`. **Use `dunce::canonicalize`, not `std::fs::canonicalize`** — the latter yields `\\?\C:\...` and silently resets every user's rotation history. Assert the produced state key against a hardcoded literal, and add a test loading a real `state.json`.
- [ ] 4.4 Hand-roll the effects: ITU-R 601-2 grayscale, `ImageEnhance` lerp semantics for Color/Contrast/Sharpness, `colorize` LUT, and the DETAIL/SMOOTH kernels. **Leave the 1-pixel border unprocessed**, as Pillow does.
- [ ] 4.5 Port `compose_collage` including the fitted-image cache and the modulo wrap for short image lists, and `crop_to_monitor`.
- [ ] 4.6 Implement `preview` (returning `cells` in pre-downscale composite coordinates) and `save_collage` (full-res, not the preview PNG).
- [ ] 4.7 Build the differential harness as three separate comparisons: `plan_collage` JSON byte-identical, `apply_effect` <=1 delta, `fit_image` 1-3 delta.
- [ ] 4.8 Run the harness once, **freeze the Python outputs as golden PNGs committed to the repo**, and keep them as the permanent Rust regression suite.

## 5. Apply, rotation, history (3-4 days)

Lands: `apply_wallpaper`, `apply_default_wallpaper`, `apply_previous_wallpaper`, `apply_saved_collage`, `set_effect`, `watch_start/stop/status/toggle`.

- [ ] 5.1 Port `set_wallpaper_win`: `SystemParametersInfoW` with **`SPIF_UPDATEINIFILE` only** (never `SPIF_SENDWININICHANGE`), plus the `WallpaperStyle = "22"` / `TileWallpaper = "0"` registry writes.
- [ ] 5.2 Port `apply_single_wallpaper` and `apply_desktop_image`, preserving that a saved desktop-wide export spans all screens while a single-screen crop repeats per screen, and that neither re-applies a baked-in effect.
- [ ] 5.3 Implement the apply lock with `tokio::sync::Mutex::try_lock()` returning `busy` — **never `.lock().await`**. Composite inside `spawn_blocking` while holding the guard.
- [ ] 5.4 Port the history ring: 50 cap with pop-from-front, truncate-forward on push, `no_history` when `idx <= 0`. Keep `apply_saved_collage` out of history.
- [ ] 5.5 Implement the rotation timer so it **re-arms after the tick completes** (period = interval + work time), not as a fixed-rate `tokio::time::interval`. Persist `general.rotation_active` on every toggle.
- [ ] 5.6 Emit `wallpaper_applied` and the `error` event with `source: "watch"` through `EventSink`.
- [ ] 5.7 Define `trait WallpaperSetter` with a fake, and drive the history / lock / timer logic headlessly in tests.
- [ ] 5.8 Verify: manual apply, rotation, and back-navigation on real multi-monitor hardware.

## 6. Transparency (2-3 days)

Lands: `list_windows`, `set_window_opacity`, `get_foreground_window`, `toggle_foreground_opacity`, `get_opacity_settings`, `save_opacity_settings`, `reapply_opacity_settings`.

- [ ] 6.1 Port `list_visible_windows` with the cloaked filter (`DwmGetWindowAttribute` / `DWMWA_CLOAKED`), the title blocklist, and the lowercased-title sort.
- [ ] 6.2 Resolve process names via `GetWindowThreadProcessId` + `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` + `QueryFullProcessImageNameW` — better than pywin32's `GetModuleFileNameEx`, and no `VM_READ` needed.
- [ ] 6.3 Port `set_window_opacity` using **`SetWindowLongPtrW`** (the Python binds `SetWindowLongW` as `c_long`, wrong on 64-bit). Preserve that alpha 255 **strips** `WS_EX_LAYERED` rather than setting 255.
- [ ] 6.4 Port the process-keyed `transparency.json` persistence and `reapply_saved_settings`.
- [ ] 6.5 Drop the `pywin32` dependency.
- [ ] 6.6 Verify: manual fade/restore across several apps; unit tests for the alpha clamp and the 128/255 toggle.

## 7. Scroll transparency (2-3 days)

Lands: `sync_scroll_transparency`, `scroll_transparency_status`.

- [ ] 7.1 Implement the hook on a dedicated thread: `SetWindowsHookExW(WH_MOUSE_LL)` + `GetMessageW` loop, stopped via `PostThreadMessageW(tid, WM_QUIT)`.
- [ ] 7.2 Keep the callback minimal — filter `WM_MOUSEWHEEL`, `GetAsyncKeyState` the modifier, push to a channel. **It must return well under `LowLevelHooksTimeout` (300 ms) or Windows silently unhooks us.**
- [ ] 7.3 Move process-name lookup, `SetLayeredWindowAttributes`, and the 0.6s debounced save to a worker thread (an improvement over the Python, which does the lookup inline on the hook thread).
- [ ] 7.4 Port `next_alpha`, `normalize_modifier`, `STEP = 5`, `MIN_ALPHA = 20`, and the `alt|ctrl|shift|win` modifier set; emit `transparency_changed`.
- [ ] 7.5 Drop the `pynput` dependency.
- [ ] 7.6 Verify: manual; translate the pure-logic tests from `test_scroll_transparency.py`.

## 8. Video (5-8 days — highest variance)

Lands: `video_start/stop/next/prev/set_sound/toggle/toggle_sound/status`, `restore_session`, `shutdown`.

- [ ] 8.1 Port `workerw.py`: the undocumented `0x052C` to Progman via `SendMessageTimeoutW`, the 100 ms settle, and the Win11 (`FindWindowExW` on Progman) / Win10 (`SHELLDLL_DefView` sibling walk) dual discovery with Progman fallback.
- [ ] 8.2 Hand-roll the libmpv FFI with `libloading` and `LoadLibraryW` on an explicit absolute path (~10 symbols, ~150 lines), with graceful degradation when the DLL is absent. Set `wid` as a *string* option before `mpv_initialize`.
- [ ] 8.3 Carry `_MPV_SAFE_VIDEO_OPTIONS` verbatim — these exist to dodge dxgi.dll access violations, do not "clean up".
- [ ] 8.4 **Own all host windows on one dedicated thread**, fixing the current cross-thread `DestroyWindow` failure (created on the `restore-session` thread at `rpc.py:933`, destroyed from main).
- [ ] 8.5 Preserve **mpv terminate strictly before `DestroyWindow`**, the `InvalidateRect` + `UpdateWindow` desktop repaint, audio on the first instance only, and `playlist_pos` navigation with wrap.
- [ ] 8.6 Port `restore_session` and the `shutdown` teardown ordering.
- [ ] 8.7 Verify: manual on real multi-monitor hardware; a soak test of 200 start/stop cycles that enumerates WORKERW children afterward and asserts none of ours survive.
- [ ] 8.8 **Bail-out if this fights back** (pre-approved, no re-planning): extract a ~400-line `wallpaper-video-host.exe` taking monitor rects and a playlist on stdin, supervised with auto-restart.

## 9. Delete the sidecar and ship (3-5 days)

- [ ] 9.1 Remove the sidecar plumbing from `engine.rs`: all three branches of `engine_command`, `parse_engine_override`, the reader threads, `CALL_TIMEOUT`, and `SHUTDOWN_GRACE`.
- [ ] 9.2 Remove `"resources": { "engine": "engine" }` from `tauri.conf.json`.
- [ ] 9.3 Delete `src/wallpaper_changer/`, `tests/` (Python), `wallpaper_changer_rpc.spec`, `main_rpc.py`, `pyproject.toml`, `uv.lock`, and `scripts/build_engine.ps1`.
- [ ] 9.4 Remove the `-SkipEngine` switch and the engine-staging assert from `scripts/build_app.ps1`. Keep signing-key validation, `bun install`, `tauri build`, artifact collection, and `latest.json` generation.
- [ ] 9.5 Port `cli.py` to a `clap`-based mode of the Tauri binary (`apply`, `watch`, `video`), preserving the `IntRange(1, 8)` collage-count and effect-choice validation.
- [ ] 9.6 Update `CLAUDE.md` and `AGENTS.md` — the two-process architecture description is the first thing both documents explain.
- [ ] 9.7 Verify: `cargo test --workspace`; full signed installer build (~15 MB) with `latest.json`; install over an existing 5.4.0 install and confirm the updater upgrades in place rather than adding a second program entry.
- [ ] 9.8 Verify the scaled-display case end-to-end: a monitor at 150% must produce the same composite resolution as before the port.
