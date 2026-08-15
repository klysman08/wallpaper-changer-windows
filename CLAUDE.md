# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

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
- **`src/wallpaper_changer/`** — Python engine (3.11+, `src/` layout). Owns everything touching Win32: composition, the WORKERW video layer, window transparency.

They speak newline-delimited JSON over stdio (`rpc.py` ⟷ `desktop/src-tauri/src/engine.rs`). Rust spawns the engine and correlates requests by id; the webview reaches it only through the `engine_call` command, and only via the engine's own method allowlist (`Engine._METHODS`).

The legacy ttkbootstrap GUI (`gui.py`, `uv run wallpaper-changer-gui`) and the CLI still work against the same modules.

**Data flow for applying a wallpaper:**
1. `config.py` — loads `config/settings.toml` relative to the package root; raises `FileNotFoundError` if missing
2. `monitor.py` — enumerates displays via `screeninfo`, computes virtual desktop dimensions
3. `image_utils.py` — selects images (random with JSON history, or sequential); resizes with `fit_mode` (`fill`/`fit`/`stretch`/`center`/`span`)
4. `wallpaper.py` — composites a BMP collage, optionally applies effects (`normal`/`bw`/`vintage`/`hdr`), and calls `ctypes.windll.user32.SystemParametersInfoW` to apply
5. GUI preview (`gui.py`) renders a live scaled thumbnail of the layout using the same pipeline

**The preview is editable.** `plan_collage()` in `wallpaper.py` is the single source of truth for which image lands in which rectangle; `compose_collage` draws from it and the `preview` RPC returns it as `cells` (composite pixel coords + `image_index`) so the UI can lay a hit target over every picture. Never reimplement the grid rules in TypeScript — a drag would then swap the wrong images the moment `_GRID_COLS` changes. Dragging one cell onto another swaps those entries in the pinned selection and re-renders; clicking one opens a picker fed by `get_thumbnails` (the webview cannot read local files, so pictures only reach it as base64). Because the user can edit the list, it can end up shorter than the grid — `compose_collage` wraps with a modulo rather than raising.

**A collage can be kept.** "Save as image" in the preview exports it to a file — one monitor's share (`crop_to_monitor`) or the whole virtual desktop — composed afresh at full resolution rather than reusing the preview's window-sized PNG. Saved files are indexed by `gallery.py` (`gallery.json` beside the settings, images in `paths.saved_folder` — resolved by `resolve_saved_dir`, defaulting to `%LOCALAPPDATA%\WallpaperChanger\saved` — which the Gallery screen lets the user point anywhere), and the Gallery screen lists them. Moving that folder only moves *new* saves: index entries hold absolute paths, so the gallery keeps listing pictures wherever they were written. `suggest_collage_path` and `list_saved_collages` both take a config overlay, so an unsaved folder change is still where the save dialog opens. That index is derived data: it is reconciled against the disk on every read, so a file deleted from Explorer just disappears from the list, and "Remove" drops the entry while **leaving the image alone** — the app never deletes the user's pictures. Applying one back (`apply_saved_collage`) neither recomposes it nor re-applies an effect the file already baked in, and picks its placement from what the index says the file is: a desktop-wide export spans every screen (`apply_desktop_image`), a single-screen crop is placed on each screen (`apply_single_wallpaper`). It is deliberately kept out of the wallpaper history, which replays image *selections* through the collage composer — a flattened picture put through that would come back as a collage of itself.

**shadcn primitives are Base UI parts, and some of them throw.** `DropdownMenuLabel` is a `Menu.GroupLabel` and raises "MenuGroupContext is missing" unless it sits inside `DropdownMenuGroup` / `DropdownMenuRadioGroup`. A throw during render unmounts the whole tree, and because the window is native the result is not a white page with a console — it is a **black window** over an app whose tray, hotkeys and engine all still work, which reads as a wallpaper bug rather than a UI crash. `ErrorBoundary` in `main.tsx` now catches that, shows the message and writes it to the app log; it is the difference between a bug report and a mystery.

**Two CSS traps this UI has already hit.** Tailwind's preflight caps every image at `max-width: 100%`, which silently clamps an inline width over 100% while honouring the inline height — any image deliberately oversized inside a frame (the focused-monitor zoom) needs `max-w-none`. And `[data-slot="card"]:hover` in `index.css` applies a transform, so while the pointer is over a card that card becomes the containing block for its `position: fixed` descendants *and* clips them: a full-window overlay or a cursor-following ghost inside a Card must be portalled to `document.body`.

**Key files:**
- `src/wallpaper_changer/wallpaper.py` — core composition + Windows API call; `_set_wallpaper_fast()` skips `WM_SETTINGCHANGE` broadcast for fade animation frames
- `src/wallpaper_changer/gui.py` — large ttkbootstrap GUI (~56 KB); includes hotkey recorder, transparency slider, system tray wiring
- `src/wallpaper_changer/image_utils.py` — `fit_mode` logic and JSON state for image selection history
- `src/wallpaper_changer/hotkeys.py` — global hotkey registration via `keyboard` lib; hotkeys defined in `settings.toml`
- `src/wallpaper_changer/i18n.py` — `t()` decorator used throughout; supported languages: `en`, `pt_BR`, `ja`
- `src/wallpaper_changer/notifications.py` — Windows toast notifications via `win10toast`
- `src/wallpaper_changer/gallery.py` — index of collages the user exported to image files

**Output format:** Always BMP (required by `SystemParametersInfoW`). Written to `paths.output_folder`; a relative value resolves under `%LOCALAPPDATA%\WallpaperChanger`, never the install directory.

**User files** live outside the installation, so the app works when installed under `Program Files`:
- `%APPDATA%\WallpaperChanger\` — `settings.toml`, `state.json`, `transparency.json`, `gallery.json`
- `%LOCALAPPDATA%\WallpaperChanger\` — composed wallpaper output, and `saved/` for exported collages

`config.py` migrates these out of the old in-install `config/` directory on first run (copy, never move; never overwrites). Override both locations with `WALLPAPER_CHANGER_CONFIG_DIR` / `WALLPAPER_CHANGER_DATA_DIR` — `tests/conftest.py` does this for every test so the suite cannot touch real user files.

## Testing conventions

Tests mock `ctypes.windll` calls — never invoke `set_wallpaper_win()` or `SystemParametersInfoW` in tests or analysis. Use `unittest.mock.patch` on `wallpaper_changer.wallpaper.ctypes` for any code path that touches the Windows API.

Collage grid supports 1–8 images per monitor (validated in CLI and GUI). Effect and fit-mode choices are string literals; adding a new one requires updating both `image_utils.py`/`wallpaper.py` **and** the CLI `click.Choice` in `cli.py`.

## Build notes

`wallpaper_changer_rpc.spec` freezes the engine sidecar; `scripts/build_engine.ps1` builds it, probes the frozen binary (`ping`/`get_config`/`get_monitors` — a missing hidden import only shows up at runtime), and stages it into `desktop/src-tauri/engine/`, which `tauri.conf.json` ships as a bundle resource. PyInstaller 6 puts bundled data under `_internal/`, but `config.py` derives `PROJECT_ROOT` from `sys.executable` when frozen, so the script also copies a default `settings.toml` next to the exe as the migration seed.

When adding a dependency that uses dynamic imports, update `hiddenimports` in the spec. `wallpaper_changer.spec` (legacy ttkbootstrap GUI) is kept for building the old GUI by hand.

Installers come from Tauri's bundler (NSIS only) — there is no Inno Setup step. The MSI target was dropped when the in-app updater landed: an NSIS update applied over an MSI install adds a second program entry instead of upgrading in place. "Start with Windows" is a runtime toggle via `tauri-plugin-autostart`, **not** `startup.py`: that module registers `sys.executable`, which inside the frozen sidecar is the headless engine. The autostart entry carries a `--minimized` argument, so a boot-time launch stays in the tray while a manual one opens the window; `general.start_minimized` asks for the same thing unconditionally. `sync_autostart` enables it once on first run (marked by an `autostart-initialized` file in the app config dir, so turning it off sticks) and rewrites the entry on later launches so an older registration picks up the argument. It is a no-op in debug builds: the entry would point at `target/debug`, and the marker is shared with the installed build. The window is hidden in `tauri.conf.json` and shown once *both* the webview has loaded and that decision has been read from the engine (`Startup` in `lib.rs`) — either half can finish last.

Rotation and video playback are session state, not preferences: `Engine._remember()` writes `general.rotation_active` / `video.enabled` to `settings.toml` the moment they are toggled (from anywhere — window, tray, hotkey), `restore_session()` brings them back at startup, and `save_config` overwrites those two keys with the live values so a stale draft from the window cannot undo a hotkey.

Updates are `tauri-plugin-updater` against a static manifest: the installed app polls `releases/latest/download/latest.json`, which GitHub resolves to the newest **non-prerelease** release — marking a release as a prerelease hides it from every installed copy. A bundled build therefore needs the minisign key in the environment (`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`); `build_app.ps1` refuses to start without it, checks the `.sig` afterwards, and stages the installer plus a generated `latest.json` into `dist/release/`. **Both files must be uploaded to every release** — one without `latest.json` breaks the check for everyone. The public half lives in `tauri.conf.json` under `plugins.updater.pubkey`; losing the private half means no installed copy can verify an update again. GitHub serves uploaded assets with spaces rewritten to dots, so the script stages under the dotted name to keep the manifest URL honest. The check itself runs in Rust because the webview's CSP allows no remote origin; `general.check_updates` turns off the automatic one, and `use-update.ts` skips it in dev builds. A found update only lights up a sidebar item — it never interrupts.

Rust owns the global hotkeys (`hotkeys.rs`) so they survive an engine restart and can reach the window. Bindings keep the old GUI's syntax (`ctrl+alt+right`); `parse_shortcut` translates it. Windows grants a global hotkey to one owner, so a binding held by another app (including the legacy GUI, if it is running) will fail to register and is surfaced in the UI.
