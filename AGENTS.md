# AGENTS.md

This file provides guidance to coding agents (Claude Code and others) when working with code in this repository.

## Commands

```powershell
# Install dependencies
uv sync --dev

# Run tests
uv run pytest
uv run pytest tests/test_wallpaper.py   # single test module

# Lint / format
uv run ruff check src/
uv run ruff format src/

# Run GUI
uv run wallpaper-changer-gui

# Run CLI
uv run wallpaper-changer apply
uv run wallpaper-changer apply --mode split2 --config path/to/settings.toml
uv run wallpaper-changer watch

# Run the Tauri desktop app (dev)
cd desktop; bun run tauri dev

# Build distributable (engine sidecar + app + signed NSIS installer + latest.json)
.\scripts\build_app.ps1
.\scripts\build_app.ps1 -NoBundle     # debug binary, no installers
.\scripts\build_app.ps1 -SkipEngine   # reuse the staged engine
.\scripts\build_engine.ps1            # engine sidecar only
```

## Architecture

Windows-only desktop app for multi-monitor collage wallpapers. Two processes:

- **`desktop/`** — Tauri v2 + React + shadcn/ui. Owns the window, tray, and global hotkeys.
- **`desktop/src-tauri/crates/wallpaper-core/`** — the native engine. Owns everything touching Win32: composition, the WORKERW video layer, window transparency, the scroll hook. **44 of the 45 RPC methods are answered here.**
- **`src/wallpaper_changer/`** — what is left of the Python engine, on its way out. It still answers exactly one method, `notify`, and phase 9 of the Rust port deletes it.

They speak newline-delimited JSON over stdio (`rpc.py` ⟷ `desktop/src-tauri/src/engine.rs`). Rust spawns the engine and correlates requests by id; the webview reaches it only through the `engine_call` command, and only via the engine's own method allowlist (`Engine._METHODS`).

The legacy ttkbootstrap GUI (`gui.py`, `uv run wallpaper-changer-gui`) and the CLI still work against the same modules.

**Data flow for applying a wallpaper:**
1. `config.py` — loads `config/settings.toml` relative to the package root; raises `FileNotFoundError` if missing
2. `monitor.py` — enumerates displays via `screeninfo`, computes virtual desktop dimensions
3. `image_utils.py` — selects images (random with JSON history, or sequential); resizes with `fit_mode` (`fill`/`fit`/`stretch`/`center`/`span`)
4. `wallpaper.py` — composites a BMP collage, optionally applies effects (`normal`/`bw`/`vintage`/`hdr`), and calls `ctypes.windll.user32.SystemParametersInfoW` to apply
5. GUI preview (`gui.py`) renders a live scaled thumbnail of the layout using the same pipeline

**The preview is editable.** `plan_collage()` in `wallpaper.py` is the single source of truth for which image lands in which rectangle; `compose_collage` draws from it and the `preview` RPC returns it as `cells` (composite pixel coords + `image_index`) so the UI can lay a hit target over every picture. Never reimplement the grid rules in TypeScript — a drag would then swap the wrong images the moment `_GRID_COLS` changes. Dragging one cell onto another swaps those entries in the pinned selection and re-renders; clicking one opens a picker fed by `get_thumbnails` (the webview cannot read local files, so pictures only reach it as base64). Because the user can edit the list, it can end up shorter than the grid — `compose_collage` wraps with a modulo rather than raising.

**Two CSS traps this UI has already hit.** Tailwind's preflight caps every image at `max-width: 100%`, which silently clamps an inline width over 100% while honouring the inline height — any image deliberately oversized inside a frame (the focused-monitor zoom) needs `max-w-none`. And `[data-slot="card"]:hover` in `index.css` applies a transform, so while the pointer is over a card that card becomes the containing block for its `position: fixed` descendants *and* clips them: a full-window overlay or a cursor-following ghost inside a Card must be portalled to `document.body`.

**Key files:**
- `src/wallpaper_changer/wallpaper.py` — core composition + Windows API call; `_set_wallpaper_fast()` skips `WM_SETTINGCHANGE` broadcast for fade animation frames
- `src/wallpaper_changer/gui.py` — large ttkbootstrap GUI (~56 KB); includes hotkey recorder, transparency slider, system tray wiring
- `src/wallpaper_changer/image_utils.py` — `fit_mode` logic and JSON state for image selection history
- `src/wallpaper_changer/hotkeys.py` — global hotkey registration via `keyboard` lib; hotkeys defined in `settings.toml`
- `desktop/src-tauri/crates/wallpaper-core/src/scroll.rs` — modifier+wheel window fading, **native since the Rust port's phase 7** (it was `scroll_transparency.py` on `pynput`). A `WH_MOUSE_LL` hook on its own thread, gated on `GetAsyncKeyState`, driven by `hotkeys.scroll_enabled` / `hotkeys.scroll_modifier`. `Engine::spawn` syncs it on boot and `save_config` on every save, so the hook follows the setting without a restart. Windows silently unhooks a callback slower than `LowLevelHooksTimeout` (300 ms), so the callback only sends a wheel count down a channel and a worker thread does the process lookup, the `SetLayeredWindowAttributes` call and the 0.6 s debounced save. That save is a read-modify-write of `transparency.json`, so a fade saved from the window is not clobbered by the next scroll.
- `src/wallpaper_changer/i18n.py` — `t()` decorator used throughout; supported languages: `en`, `pt_BR`, `ja`
- `src/wallpaper_changer/notifications.py` — Windows toast notifications via `win10toast`

**Output format:** Always BMP (required by `SystemParametersInfoW`). Written to `paths.output_folder`; a relative value resolves under `%LOCALAPPDATA%\WallpaperChanger`, never the install directory.

**User files** live outside the installation, so the app works when installed under `Program Files`:
- `%APPDATA%\WallpaperChanger\` — `settings.toml`, `state.json`, `transparency.json`
- `%LOCALAPPDATA%\WallpaperChanger\` — composed wallpaper output

`config.py` migrates these out of the old in-install `config/` directory on first run (copy, never move; never overwrites). Override both locations with `WALLPAPER_CHANGER_CONFIG_DIR` / `WALLPAPER_CHANGER_DATA_DIR` — `tests/conftest.py` does this for every test so the suite cannot touch real user files.

## Testing conventions

Tests mock `ctypes.windll` calls — never invoke `set_wallpaper_win()` or `SystemParametersInfoW` in tests or analysis. Use `unittest.mock.patch` on `wallpaper_changer.wallpaper.ctypes` for any code path that touches the Windows API.

Collage grid supports 1–8 images per monitor (validated in CLI and GUI). Effect and fit-mode choices are string literals; adding a new one requires updating both `image_utils.py`/`wallpaper.py` **and** the CLI `click.Choice` in `cli.py`.

## Build notes

`wallpaper_changer_rpc.spec` freezes the engine sidecar; `scripts/build_engine.ps1` builds it, probes the frozen binary (`ping`/`get_config`/`get_monitors` — a missing hidden import only shows up at runtime), and stages it into `desktop/src-tauri/engine/`, which `tauri.conf.json` ships as a bundle resource. PyInstaller 6 puts bundled data under `_internal/`, but `config.py` derives `PROJECT_ROOT` from `sys.executable` when frozen, so the script also copies a default `settings.toml` next to the exe as the migration seed.

When adding a dependency that uses dynamic imports, update `hiddenimports` in the spec. `wallpaper_changer.spec` (legacy ttkbootstrap GUI) is kept for building the old GUI by hand.

Installers come from Tauri's bundler (NSIS only) — there is no Inno Setup step. The MSI target was dropped when the in-app updater landed: an NSIS update applied over an MSI install adds a second program entry instead of upgrading in place. "Start with Windows" is a runtime toggle via `tauri-plugin-autostart`, **not** `startup.py`: that module registers `sys.executable`, which inside the frozen sidecar is the headless engine. The autostart entry carries a `--minimized` argument, so a boot-time launch stays in the tray while a manual one opens the window; `general.start_minimized` asks for the same thing unconditionally. `sync_autostart` enables it once on first run (marked by an `autostart-initialized` file in the app config dir, so turning it off sticks) and rewrites the entry on later launches so an older registration picks up the argument. It is a no-op in debug builds: the entry would point at `target/debug`, and the marker is shared with the installed build. The window is hidden in `tauri.conf.json` and shown once *both* the webview has loaded and that decision has been read from the engine (`Startup` in `lib.rs`) — either half can finish last.

Rotation and video playback are session state, not preferences: `Session::remember()` writes `general.rotation_active` / `video.enabled` to `settings.toml` the moment they are toggled (from anywhere — window, tray, hotkey), `Session::restore_session()` brings them back at startup, and `save_config` overwrites those two keys with the live values so a stale draft from the window cannot undo a hotkey. The restore runs from `Engine::spawn`, **not** through an RPC method: the sidecar used to do it on a daemon thread, which meant it bypassed the port's strangler seam entirely and had to move across the moment each half was ported, or there would have been two rotation timers and two video players.

Updates are `tauri-plugin-updater` against a static manifest: the installed app polls `releases/latest/download/latest.json`, which GitHub resolves to the newest **non-prerelease** release — marking a release as a prerelease hides it from every installed copy. A bundled build therefore needs the minisign key in the environment (`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`); `build_app.ps1` refuses to start without it, checks the `.sig` afterwards, and stages the installer plus a generated `latest.json` into `dist/release/`. **Both files must be uploaded to every release** — one without `latest.json` breaks the check for everyone. The public half lives in `tauri.conf.json` under `plugins.updater.pubkey`; losing the private half means no installed copy can verify an update again. GitHub serves uploaded assets with spaces rewritten to dots, so the script stages under the dotted name to keep the manifest URL honest. The check itself runs in Rust because the webview's CSP allows no remote origin; `general.check_updates` turns off the automatic one, and `use-update.ts` skips it in dev builds. A found update only lights up a sidebar item — it never interrupts.

Rust owns the global hotkeys (`hotkeys.rs`) so they survive an engine restart and can reach the window. Bindings keep the old GUI's syntax (`ctrl+alt+right`); `parse_shortcut` translates it. Windows grants a global hotkey to one owner, so a binding held by another app (including the legacy GUI, if it is running) will fail to register and is surfaced in the UI.

**No shortcut ships bound.** Every `[hotkeys]` entry in `config/settings.toml` is `""` except `scroll_modifier`, which is not a shortcut. Claiming a dozen combinations on first run steals them from whatever the user already runs, and Windows gives no warning to the loser. Empty means "not registered" everywhere: `register_all` skips it, and `HotkeyManager.update` skips it rather than reporting it as a malformed shortcut.

The app icon is generated, not hand-edited: `assets/icon/wpaper-logo.png` is the source, `uv run python scripts/make_icon.py` squares and pads it into `desktop/app-icon.png` plus `desktop/public/icon.png`, and `cd desktop; bunx tauri icon app-icon.png` regenerates the platform set under `desktop/src-tauri/icons/`.
