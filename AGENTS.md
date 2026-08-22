# AGENTS.md

This file provides guidance to coding agents (Claude Code and others) when working with code in this repository.

## Commands

```powershell
# Run tests (from desktop/src-tauri, which is the workspace root)
cargo test --workspace
cargo test -p wallpaper-core --lib collage::   # one module
cargo check -p tauri-native                    # the shipping build, on its own

# Tests that take over the screen are opt-in
cargo test -p wallpaper-core --test desktop_layer -- --ignored --nocapture

# Lint / format
cargo clippy --workspace --all-targets
cargo fmt -p wallpaper-core

# Run the desktop app (dev)
cd desktop; bun run tauri dev

# The same binary is the CLI
cargo run -p tauri-native -- apply --effect bw
cargo run -p tauri-native -- watch
cargo run -p tauri-native -- video --folder C:/videos

# Build distributable (app + signed NSIS installer + latest.json)
.\scripts\build_app.ps1
.\scripts\build_app.ps1 -NoBundle     # debug binary, no installers
```

## Architecture

Windows-only desktop app for multi-monitor collage wallpapers. **One process**, two
halves:

- **`desktop/`** — Tauri v2 + React + shadcn/ui. Owns the window, tray, and global
  hotkeys.
- **`desktop/src-tauri/crates/wallpaper-core/`** — the engine. Owns everything touching
  Win32: composition, applying, the WORKERW video layer, window transparency, the
  scroll hook. It answers **all 45 methods**, and deliberately has **no `tauri`
  dependency**, so it stays testable headless.

The webview reaches the engine only through the `engine_call` command, which funnels
into `Engine::call` in `desktop/src-tauri/src/engine.rs` — the single choke point that
`lib.rs`, `tray.rs` and `hotkeys.rs` also use. `Core::dispatch` answers by method name;
anything it does not know comes back as `unknown_method`, so that match *is* the
allowlist.

**This used to be two processes.** A Python engine (`src/wallpaper_changer/`) spoke
newline-delimited JSON over stdio to the shell, and the Rust port replaced it one
state-ownership unit at a time through a strangler seam at `Engine::call`. The Python is
gone; `openspec/changes/port-engine-to-rust/tasks.md` records what was learned doing it,
including several behaviours that look arbitrary and are not.

Because the engine is in-process, three things must be given up explicitly on the way
out — they used to die with the sidecar. `Engine::shutdown` stops the rotation timer,
removes the system-wide mouse hook, and destroys the video host windows.

**Where things live:**
- `collage.rs` — grid layout, `fit_image`, and `compose_collage`
- `apply.rs` — writing the BMP and calling `SystemParametersInfoW`
- `images.rs` — folder listing, thumbnails, and **the crate's only image loader and
  resampler**
- `session.rs` — everything that outlives a call: the apply lock, history, the rotation
  timer, unsaved settings
- `video.rs` / `workerw.rs` — libmpv and the desktop layer
- `transparency.rs` / `scroll.rs` — window fading, by slider and by wheel
- `parallel.rs` — the bounded worker pool both composition and thumbnails use
- `cli.rs` (in the shell) — `apply`, `watch`, `video`, driving the same `dispatch`

**Two image rules that are not style preferences:**
- **Never `image::open`.** It picks the decoder from the *file extension*, and roughly
  one picture in ten is named for a format it is not. Use `images::open_image`, which
  sniffs the content. A `.jpeg` that is really a WebP otherwise reaches the JPEG decoder
  and fails with `Illegal start bytes: 5249`.
- **Never `image::imageops::resize`.** Use `images::resize_lanczos3`, which is SIMD.
  Resampling is what composition spends its time on, and the scalar version made the app
  slower than the Python it replaced.

**Tests that touch the real desktop are `#[ignore]`.** `cargo test` must never embed
windows in someone's desktop, apply a wallpaper, or fade a window because they wanted to
check a build.

**Data flow for applying a wallpaper:**
1. `config.rs` — reads `settings.toml` from `%APPDATA%\WallpaperChanger`, migrating it out of an old in-install `config/` on first run
2. `monitor.rs` — enumerates displays with `EnumDisplayMonitors`, computes the virtual desktop
3. `selection.rs` — picks images (random with JSON history, or sequential)
4. `collage.rs` — plans the grid, fits each picture (`fill`/`fit`/`stretch`/`center`/`span`), pastes the composite
5. `effects.rs` — optionally applies `normal`/`bw`/`vintage`/`hdr`
6. `apply.rs` — writes the BMP and calls `SystemParametersInfoW`

**The preview is editable.** `plan_collage()` in `collage.rs` is the single source of truth for which image lands in which rectangle; `compose_collage` draws from it and the `preview` RPC returns it as `cells` (composite pixel coords + `image_index`) so the UI can lay a hit target over every picture. Never reimplement the grid rules in TypeScript — a drag would then swap the wrong images the moment `columns_for` changes. Dragging one cell onto another swaps those entries in the pinned selection and re-renders; clicking one opens a picker fed by `get_thumbnails` (the webview cannot read local files, so pictures only reach it as base64). Because the user can edit the list, it can end up shorter than the grid — `compose_collage` wraps with a modulo rather than raising.

**Two CSS traps this UI has already hit.** Tailwind's preflight caps every image at `max-width: 100%`, which silently clamps an inline width over 100% while honouring the inline height — any image deliberately oversized inside a frame (the focused-monitor zoom) needs `max-w-none`. And `[data-slot="card"]:hover` in `index.css` applies a transform, so while the pointer is over a card that card becomes the containing block for its `position: fixed` descendants *and* clips them: a full-window overlay or a cursor-following ghost inside a Card must be portalled to `document.body`.

**Key files:**
- `collage.rs` — `plan_collage`, `fit_image`, `compose_collage`. Every number is integer arithmetic copied literally from the Python; an off-by-one shifts a crop.
- `apply.rs` — the BMP write and `SystemParametersInfoW`. `SPIF_SENDWININICHANGE` is omitted on purpose, to suppress Explorer's crossfade.
- `images.rs` — folder listing, thumbnails, and the crate's only image loader (`open_image`, content-sniffing) and resampler (`resize_lanczos3`, SIMD).
- `session.rs` — the apply lock (never queues: a second concurrent apply is told `busy`), the history, the rotation timer, and the live-but-unsaved settings.
- `video.rs` / `workerw.rs` — libmpv loaded at runtime, and the WORKERW desktop layer. One dedicated thread owns every host window, because `DestroyWindow` may only be called by the thread that created it and fails *silently* otherwise.
- `scroll.rs` — modifier+wheel window fading. Windows silently unhooks a `WH_MOUSE_LL` callback slower than `LowLevelHooksTimeout` (300 ms), so the callback only sends a wheel count down a channel and a worker thread does the process lookup, the `SetLayeredWindowAttributes` call and the 0.6 s debounced save. That save is a read-modify-write of `transparency.json`, so a fade saved from the window is not clobbered by the next scroll.
- `i18n.rs` — the translation tables; supported languages `en`, `pt_BR`, `ja`.
- `parallel.rs` — `map_bounded`, the four-worker pool composition and thumbnails share. Four rather than the core count because each worker holds a decoded full-size image.

**Output format:** Always BMP (required by `SystemParametersInfoW`). Written to `paths.output_folder`; a relative value resolves under `%LOCALAPPDATA%\WallpaperChanger`, never the install directory.

**User files** live outside the installation, so the app works when installed under `Program Files`:
- `%APPDATA%\WallpaperChanger\` — `settings.toml`, `state.json`, `transparency.json`
- `%LOCALAPPDATA%\WallpaperChanger\` — composed wallpaper output

`config.rs` migrates these out of the old in-install `config/` directory on first run (copy, never move; never overwrites). Override both locations with `WALLPAPER_CHANGER_CONFIG_DIR` / `WALLPAPER_CHANGER_DATA_DIR` — the `Sandbox` helper in `lib.rs` does this for every test that touches them, behind a process-wide lock, so the suite cannot reach real user files.

## Testing conventions

**Never let `cargo test` touch the real desktop.** Applying a wallpaper, fading a
window, or embedding anything in the desktop layer belongs in an `#[ignore]`d test —
see `tests/desktop_layer.rs`. Win32 behaviour is otherwise expressed through traits
with fakes: `WallpaperSetter`, `Notifier`, `scroll::Desktop`.

**The golden images are Pillow's output**, frozen while the Python engine still
existed, and there is no way to make more of them. When composition changes
deliberately, re-derive the *bound* from `fit_drift_against_the_goldens`; never
regenerate the goldens from Rust, which would leave them comparing the port to itself.

**The conformance corpus** (`tests/conformance/*.json`) pins the protocol envelope and
is language-neutral. A method answering `unknown_method` is a failure, not a skip.

Collage grid supports 1–8 images per monitor, validated in `cli.rs` and in the engine.
Effect and fit-mode choices are string literals; adding one means updating
`effects.rs`/`collage.rs` **and** the `EFFECTS` list in `cli.rs`.

## Build notes

The engine compiles into the app, so there is no sidecar to stage and
`scripts/build_app.ps1` builds the frontend and runs Tauri's bundler. **`libmpv-2.dll`
ships as a Tauri resource** (`tauri.conf.json`), and `Engine::spawn` hands the resolved
resource directory to `video::set_search_dir` because the core cannot resolve a Tauri
path itself. At 112 MB it is most of the download.


Installers come from Tauri's bundler (NSIS only) — there is no Inno Setup step. The MSI target was dropped when the in-app updater landed: an NSIS update applied over an MSI install adds a second program entry instead of upgrading in place. "Start with Windows" is a runtime toggle via `tauri-plugin-autostart`, **not** the engine's `startup.rs`, which reads and writes a different `Run` value and is kept only for `get_startup_enabled`. The autostart entry carries a `--minimized` argument, so a boot-time launch stays in the tray while a manual one opens the window; `general.start_minimized` asks for the same thing unconditionally. `sync_autostart` enables it once on first run (marked by an `autostart-initialized` file in the app config dir, so turning it off sticks) and rewrites the entry on later launches so an older registration picks up the argument. It is a no-op in debug builds: the entry would point at `target/debug`, and the marker is shared with the installed build. The window is hidden in `tauri.conf.json` and shown once *both* the webview has loaded and that decision has been read from the engine (`Startup` in `lib.rs`) — either half can finish last.

Rotation and video playback are session state, not preferences: `Session::remember()` writes `general.rotation_active` / `video.enabled` to `settings.toml` the moment they are toggled (from anywhere — window, tray, hotkey), `Session::restore_session()` brings them back at startup, and `save_config` overwrites those two keys with the live values so a stale draft from the window cannot undo a hotkey. The restore runs from `Engine::spawn`, **not** through an RPC method: the sidecar used to do it on a daemon thread, which meant it bypassed the port's strangler seam entirely and had to move across the moment each half was ported, or there would have been two rotation timers and two video players.

Updates are `tauri-plugin-updater` against a static manifest: the installed app polls `releases/latest/download/latest.json`, which GitHub resolves to the newest **non-prerelease** release — marking a release as a prerelease hides it from every installed copy. A bundled build therefore needs the minisign key in the environment (`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`); `build_app.ps1` refuses to start without it, checks the `.sig` afterwards, and stages the installer plus a generated `latest.json` into `dist/release/`. **Both files must be uploaded to every release** — one without `latest.json` breaks the check for everyone. The public half lives in `tauri.conf.json` under `plugins.updater.pubkey`; losing the private half means no installed copy can verify an update again. GitHub serves uploaded assets with spaces rewritten to dots, so the script stages under the dotted name to keep the manifest URL honest. The check itself runs in Rust because the webview's CSP allows no remote origin; `general.check_updates` turns off the automatic one, and `use-update.ts` skips it in dev builds. A found update only lights up a sidebar item — it never interrupts.

Rust owns the global hotkeys (`hotkeys.rs`) so they survive an engine restart and can reach the window. Bindings keep the old GUI's syntax (`ctrl+alt+right`); `parse_shortcut` translates it. Windows grants a global hotkey to one owner, so a binding held by another app (including the legacy GUI, if it is running) will fail to register and is surfaced in the UI.

**No shortcut ships bound.** Every `[hotkeys]` entry in `config/settings.toml` is `""` except `scroll_modifier`, which is not a shortcut. Claiming a dozen combinations on first run steals them from whatever the user already runs, and Windows gives no warning to the loser. Empty means "not registered" everywhere: `register_all` skips it, and `HotkeyManager.update` skips it rather than reporting it as a malformed shortcut.

The app icon is generated, not hand-edited: `assets/icon/wpaper-logo.png` is the source, `uv run python scripts/make_icon.py` squares and pads it into `desktop/app-icon.png` plus `desktop/public/icon.png`, and `cd desktop; bunx tauri icon app-icon.png` regenerates the platform set under `desktop/src-tauri/icons/`.
