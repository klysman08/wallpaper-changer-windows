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

# Build distributable (engine sidecar + app + MSI/NSIS installers)
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

**Key files:**
- `src/wallpaper_changer/wallpaper.py` — core composition + Windows API call; `_set_wallpaper_fast()` skips `WM_SETTINGCHANGE` broadcast for fade animation frames
- `src/wallpaper_changer/gui.py` — large ttkbootstrap GUI (~56 KB); includes hotkey recorder, transparency slider, system tray wiring
- `src/wallpaper_changer/image_utils.py` — `fit_mode` logic and JSON state for image selection history
- `src/wallpaper_changer/hotkeys.py` — global hotkey registration via `keyboard` lib; hotkeys defined in `settings.toml`
- `src/wallpaper_changer/scroll_transparency.py` — modifier+wheel window fading. A `pynput` mouse listener gated on `GetAsyncKeyState`, owned by the engine and driven by `hotkeys.scroll_enabled` / `hotkeys.scroll_modifier`. `rpc.py` syncs it on boot and on every `save_config`, so the hook follows the setting without a restart. `pynput` resolves its backend dynamically, so the spec needs the `pynput.*` hidden imports and `build_engine.ps1` asserts `has_scroll_transparency` on the frozen binary.
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

Installers come from Tauri's bundler (MSI + NSIS) — there is no Inno Setup step. "Start with Windows" is a runtime toggle via `tauri-plugin-autostart`, **not** `startup.py`: that module registers `sys.executable`, which inside the frozen sidecar is the headless engine.

Rust owns the global hotkeys (`hotkeys.rs`) so they survive an engine restart and can reach the window. Bindings keep the old GUI's syntax (`ctrl+alt+right`); `parse_shortcut` translates it. Windows grants a global hotkey to one owner, so a binding held by another app (including the legacy GUI, if it is running) will fail to register and is surfaced in the UI.

**No shortcut ships bound.** Every `[hotkeys]` entry in `config/settings.toml` is `""` except `scroll_modifier`, which is not a shortcut. Claiming a dozen combinations on first run steals them from whatever the user already runs, and Windows gives no warning to the loser. Empty means "not registered" everywhere: `register_all` skips it, and `HotkeyManager.update` skips it rather than reporting it as a malformed shortcut.

The app icon is generated, not hand-edited: `assets/icon/wpaper-logo.png` is the source, `uv run python scripts/make_icon.py` squares and pads it into `desktop/app-icon.png` plus `desktop/public/icon.png`, and `cd desktop; bunx tauri icon app-icon.png` regenerates the platform set under `desktop/src-tauri/icons/`.
