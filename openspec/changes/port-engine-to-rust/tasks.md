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

- [x] 4.1 `grid_layout` and `plan_collage` ported literally: the `{1:1, 2:2, 3:2, 4:2, 5:3, 6:3, 7:4, 8:4, 9:3}` table with `ceil(sqrt(n))` fallback, `h / rows` truncation, and the centred short last row.
- [x] 4.2 `fit_image`'s five modes with the integer-truncating arithmetic (`int(src * ratio)`, `// 2` offsets); `span` aliases `fill`; Pillow's clipping `paste` and out-of-bounds `crop` reproduced.
- [x] 4.3 `pick_images` ported with **`dunce::canonicalize`**. A test pins the state key against a literal and asserts it carries no `\?\` prefix.
- [x] 4.4 Effects hand-rolled from measured Pillow behaviour — see below.
- [x] 4.5 `compose_collage` with the fitted-image cache and the modulo wrap for short selections, plus `crop_to_monitor` and `images_on`.
- [x] 4.6 `preview` (cells in pre-downscale composite pixels) and `save_collage` (full resolution, not the preview PNG).
- [x] 4.7 Differential harness at `tests/differential/compare.py`, three separate comparisons.
- [x] 4.8 **109/109 checks pass**, and the Pillow outputs are frozen as 37 golden PNGs with a Rust test (`tests/golden.rs`) holding the core to them.

**Methods landed (2):** `preview`, `save_collage`. Running total: **15 of 45**.

### Differential results

| Stage | Bound | Measured |
|---|---|---|
| `plan_collage` geometry, 4 layouts x counts 1-9 x both sharing modes | **0** | **0 — byte-identical, 72/72** |
| `apply_effect`, 4 effects x 3 image shapes | 1 | **0 — bit-identical, 12/12** |
| `fit_image` downscale | 1 | 0–1 |
| `fit_image` no resample (`center`) | 0 | **0** |
| `fit_image` upscale | 24 | 18–22 |

**Effects came out bit-identical, better than the ≤1 the plan budgeted.** Four Pillow behaviours had to be matched exactly, each measured rather than assumed:

- Greyscale is ITU-R 601-2 in fixed point, `(R*19595 + G*38470 + B*7471 + 32768) >> 16`. The `image` crate's `grayscale` is Rec. 709 and would have made `bw` and `vintage` visibly wrong.
- `Image.blend` **truncates**, and with `alpha > 1` (which `hdr` uses at 1.35 and 1.45) it extrapolates past both endpoints, so the clamp is load-bearing.
- A 3x3 kernel leaves the 1-pixel border untouched.
- The kernel is **pre-divided into `f32`** and accumulated row by row. That is not the same as summing integers and dividing at the end: the rounding error pushes an exact `.5` slightly low, so it rounds *down* where `round_half_up` would round up. Getting this wrong costs a full level on ~7% of pixels for `hdr`.

**The fitting bound follows the real scale direction, not the case name.** Fitting a 400x300 source into 500x100 *upscales* the width, and `fill` and `fit` can disagree about direction for the same target. The first harness run failed 9 checks purely because the labels lied; classifying by the computed ratio fixed it with no change to the port.

**Corpus unchanged at 19.** `preview` and `save_collage` are not in it deliberately: they need a populated wallpapers folder, which would make the protocol corpus depend on image fixtures. Pixels are the golden suite's job; the corpus stays about envelopes.

### Notes for later phases

- **WebP export is not supported.** `_SAVE_FORMATS` in Python accepts `.webp`, but the `image` crate is decode-only for WebP, so `save_collage` rejects it as `invalid`. PNG, JPEG and BMP all work. Either vendor an encoder or drop `.webp` from the UI's format list in phase 9.
- **Debug-mode composition is slow.** The core test suite takes ~175 s because it composes full canvases unoptimised. Consider `opt-level = 1` for `[profile.test]` if iteration gets painful.

## 5. Apply, rotation, history (3-4 days)

Lands: `apply_wallpaper`, `apply_default_wallpaper`, `apply_previous_wallpaper`, `apply_saved_collage`, `set_effect`, `watch_start/stop/status/toggle`, plus `save_config` deferred from phase 3.

- [x] 5.1 Port `set_wallpaper_win`: `SystemParametersInfoW` with **`SPIF_UPDATEINIFILE` only** (never `SPIF_SENDWININICHANGE`), plus the `WallpaperStyle = "22"` / `TileWallpaper = "0"` registry writes.
- [x] 5.2 Port `apply_single_wallpaper` and `apply_desktop_image`, preserving that a saved desktop-wide export spans all screens while a single-screen crop repeats per screen, and that neither re-applies a baked-in effect.
- [x] 5.3 Apply lock via `Session::begin_apply`, a `try_lock` that answers `busy` — **never `.lock().await`**. The composition runs in `spawn_blocking` while the guard is held.
- [x] 5.4 History ring ported: 50 cap with pop-from-front, truncate-forward on push, `no_history` when the cursor is at or before 0. `apply_saved_collage` stays out of it.
- [x] 5.5 Rotation timer **re-arms after the tick completes**, so the period is interval + work time. Deliberately a `sleep` loop rather than `tokio::time::interval`, which would fire immediately and then catch up on missed ticks. `general.rotation_active` is persisted on every toggle.
- [x] 5.6 `wallpaper_applied` and the `error` event with `source: "watch"` go through `EventSink`.
- [x] 5.7 `trait WallpaperSetter` with a `FakeSetter`; the lock, the history, the timer, `set_effect` and `save_config` are all driven headlessly.
- [x] 5.8 `save_config` landed with rotation, as planned — see below for the return path it needed.
- [ ] 5.9 Verify: manual apply, rotation and back-navigation on real hardware. **Not done** — every check here stops short of changing the developer's own desktop; see below.

**Methods landed (10):** `apply_wallpaper`, `apply_previous_wallpaper`, `apply_default_wallpaper`, `apply_saved_collage`, `set_effect`, `watch_start`, `watch_stop`, `watch_status`, `watch_toggle`, `save_config`. Running total: **25 of 45**.

**Verification:** 143 core unit tests, 3 golden, 9 shell, 176 pytest — all green. Corpus against the Rust core 19 → **25**, with the whole `rotation` area now passing; Python still **35/35 strict**. The 10 remaining skips are exactly `get_capabilities` plus the transparency and video areas, i.e. phases 6-8.

### `Bridge` — the return path phase 3 said was missing

`save_config` folds `general.rotation_active` and `video.enabled` into what it writes. Rotation is ours now; the video player is not. Phase 3 deferred this because "it needs a call path from `Core` back into the sidecar that does not exist" — so this phase built it: `trait Bridge` alongside `EventSink`, implemented in `engine.rs` by the sidecar it already owns. Three uses, all transitional:

- `video_status` → the real `video.enabled`. If the sidecar cannot answer, the value **already in the file** is kept rather than asserting "off", which would silently lose the user's video wallpaper on the next launch.
- `sync_scroll_transparency` after a save, because the mouse hook has to follow the new settings immediately.
- `_reload_config`, six new lines in `rpc.py`. This one is not optional: `Engine._config()` over there caches, and Python no longer sees its own settings file being written. Without it the next `_remember` in the sidecar would write a **stale copy back over what the core just saved**, silently undoing a rotation toggle.

### The rotation timer had to move whole, not by method

`rpc.py`'s `serve()` calls `restore_session()` on a thread at startup — it is not an RPC method, so porting `watch_start` alone would have left the sidecar starting *its own* timer on every launch: two rotations running, one of them invisible to the UI and unstoppable from it. So `restore_session` lost its rotation half (the video half stays), and `Engine::spawn` now restores rotation through the core. Two Python tests asserted the old contract and were rewritten to assert the new one.

### The settings are read fresh, with an overlay — not cached

`Engine._config()` caches, and `set_effect` works by *mutating that cache*: the effect goes live without being written. A cache in the core would be actively wrong while the sidecar exists, because Python writes `settings.toml` whenever `video_start` toggles `video.enabled`. So the file stays the single source of truth and the only thing held in memory is `Session::overrides` — the sparse set of values changed but not saved, cleared when `save_config` adopts them. `get_config` and `preview` both read through it, so the live effect is visible exactly where it was before.

### The BMP is not byte-identical to Pillow's, and that is fine

The wallpaper file is the one byte-level surface this phase adds — composition was already pinned by the golden PNGs, but the *file* handed to `SystemParametersInfoW` now comes from a different encoder. Compared across four shapes including a 2x2 and a 40x1:

| | |
|---|---|
| Pixel data | **identical**, every byte |
| File size, `biSizeImage`, `bfOffBits`, bit depth, compression | identical |
| `biXPelsPerMeter` / `biYPelsPerMeter` | **0 (Rust) vs 3780 (Pillow)** |

The only difference is the declared pixel density, 96 DPI versus unspecified. `WallpaperStyle` is what decides how the desktop lays the picture out, not the BMP's density fields, so this changes nothing about what appears on screen.

### Not verified, deliberately

Task 5.9 is open. Everything above is checked against fakes, fixtures and the Python engine; nothing in this session applied a real wallpaper, started a real rotation tick that composed, or exercised the tray and hotkey paths end to end. That check changes the desktop of whoever runs it, so it is the developer's to make: `bun run tauri dev`, then apply a collage, step back, toggle rotation, and confirm `settings.toml` picks up `rotation_active`.

Worth watching for in that pass:

- The apply goes through `spawn_blocking` now. A long composite must leave the window responsive rather than freezing it, which is the visible half of the lock behaviour.
- Rotation restored at startup comes from the core, not the sidecar — confirm exactly one timer runs by toggling it off from the tray and checking it stays off across a restart.

## 6. Transparency (2-3 days)

Lands: `list_windows`, `set_window_opacity`, `get_foreground_window`, `toggle_foreground_opacity`, `get_opacity_settings`, `save_opacity_settings`, `reapply_opacity_settings`.

- [x] 6.1 `list_visible_windows` ported with the cloaked filter (`DwmGetWindowAttribute` / `DWMWA_CLOAKED`), the four-title blocklist, and the lowercased-title sort.
- [x] 6.2 Process names via `GetWindowThreadProcessId` + `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` + `QueryFullProcessImageNameW`.
- [x] 6.3 `set_window_opacity` ported using **`SetWindowLongPtrW`** (the Python binds `SetWindowLongW` as `c_long`, wrong on 64-bit). Alpha 255 **strips** `WS_EX_LAYERED` rather than setting 255.
- [x] 6.4 Process-keyed `transparency.json` persistence and `reapply_saved_settings` ported.
- [ ] 6.5 ~~Drop `pywin32`~~ — **moved to phase 7.** `scroll_transparency.py` calls `transparency._get_process_name_for_hwnd`, so the dependency belongs to the *scroll* unit, not this one. It drops when the hook moves.
- [x] 6.6 Unit tests for the blocklist, the alpha clamp, the 128/255 toggle and the settings file, plus two differential checks against Python. Manual fade/restore is still open — see below.

**Methods landed (7):** `list_windows`, `set_window_opacity`, `get_foreground_window`, `toggle_foreground_opacity`, `get_opacity_settings`, `save_opacity_settings`, `reapply_opacity_settings`. Running total: **32 of 45**.

**Verification:** 153 core unit tests, 3 golden, 9 shell, 176 pytest — all green. Corpus against the Rust core 25 → **29**; Python still **35/35 strict**. The 6 remaining skips are `get_capabilities`, the two scroll-hook cases (phase 7) and the three video cases (phase 8).

### Differential results

**Window enumeration agreed exactly** — same handles, titles, process names and sort order, with no field differing. **Small sample: 3 windows on the machine that ran it**, so this confirms the shape and the ordering rule rather than the breadth of the filter.

**`transparency.json` round-trips both ways**, which matters because the file outlives the port and a user upgrading mid-migration keeps what they saved:

| Check | Result |
|---|---|
| Rust reads a file Python wrote | values identical |
| Python reads a file Rust wrote | values identical |
| Non-ASCII process names | written literally in UTF-8, matching `ensure_ascii=False` |

One cosmetic difference: `serde_json`'s `Map` is a `BTreeMap`, so Rust writes the keys **sorted** where Python preserved insertion order. Both sides read either file identically; nothing keys off the order.

### The scroll hook caches this file, and would have clobbered it

`ScrollTransparency._settings` is loaded once at `start()` and never reread, and its debounced `_flush` writes the whole snapshot back. With the core now owning every other write, a fade saved from the window would be silently reverted by the user's next modifier-scroll. Same hazard as the config cache in phase 5, so the same fix: `reload_settings()` on the hook plus a `_reload_opacity_settings` RPC, called through the `Bridge` after `save_opacity_settings` and `toggle_foreground_opacity`. Both go away in phase 7 with the hook.

### Two inherited bugs fixed rather than ported

Phase 5 ported `startup.py`'s wrong registry value name faithfully, because changing it would have added or removed a real autostart entry. These two are different — they are strictly wrong, and fixing them cannot surprise anyone:

- **`SetWindowLongW` bound as `c_long` is the 32-bit entry point.** It works today only because `GWL_EXSTYLE` happens to fit in 32 bits. The port uses `SetWindowLongPtrW`/`GetWindowLongPtrW`.
- **The process query asked for far more rights than it needed.** pywin32's `GetModuleFileNameEx` path opens with `PROCESS_QUERY_INFORMATION | PROCESS_VM_READ`, which an elevated or protected process refuses — and a window whose process cannot be named is *dropped from the list entirely*. `QueryFullProcessImageNameW` needs only `PROCESS_QUERY_LIMITED_INFORMATION`. Windows that used to be silently missing should now appear.

The second is a visible behaviour change: the window list can get **longer**. The differential run above showed no difference, but it enumerated only 3 windows and no elevated ones, so that is not evidence either way — it is the case to look at during manual verification.

### Not verified, deliberately

Nothing in this session faded a real window. `list_windows`, `get_foreground_window` and the settings file are exercised for real; `set_window_opacity`, `toggle_foreground_opacity` and `reapply_opacity_settings` are covered only where they are no-ops, because the alternative is changing how the developer's own screen looks. Worth checking by hand:

- Fade a window to 128 and back to 255, and confirm the window returns to its **ordinary** rendering path — that is the `WS_EX_LAYERED` strip, and the visible symptom of getting it wrong is subtle (a window that looks right but renders through the layered path).
- Fade something running elevated, which is the case the old rights bug hid.
- Fade from the window, then modifier-scroll on another app, and confirm the first setting is still there — that is the cache fix above.

### A build break the workspace check could not see

`bun run tauri dev` failed after this phase with `could not find select in tokio`, on code
`cargo check --workspace --all-targets` had just accepted. `wallpaper-core` asked tokio for
`sync`/`time`/`rt` but not `macros`; `wallpaper-core-cli` asks for `macros`, and a workspace
check unifies features across every crate in the graph — so the missing feature was supplied
by a sibling the app does not depend on. Building the app alone drops the CLI, and the feature
with it.

`macros` is now declared where it is used. **Per-phase verification adds `cargo check -p
tauri-native`**: the workspace check cannot catch an under-declared feature, and the first
thing to fail is the shipping build.

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
