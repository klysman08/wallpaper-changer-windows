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

# Build distributable (exe + Inno Setup installer)
.\scripts\build_exe.ps1
.\scripts\build_exe.ps1 -NoInstaller   # exe only, skips Inno Setup
```

## Architecture

Windows-only desktop app (Python 3.11+) that assembles collage wallpapers for multi-monitor setups. Uses `src/` layout — all imports are `from wallpaper_changer import ...`.

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
- `src/wallpaper_changer/i18n.py` — `t()` decorator used throughout; supported languages: `en`, `pt_BR`, `ja`
- `src/wallpaper_changer/notifications.py` — Windows toast notifications via `win10toast`

**Output format:** Always BMP (required by `SystemParametersInfoW`). Written to `paths.output_folder` (default `assets/output`).

## Testing conventions

Tests mock `ctypes.windll` calls — never invoke `set_wallpaper_win()` or `SystemParametersInfoW` in tests or analysis. Use `unittest.mock.patch` on `wallpaper_changer.wallpaper.ctypes` for any code path that touches the Windows API.

Collage grid supports 1–8 images per monitor (validated in CLI and GUI). Effect and fit-mode choices are string literals; adding a new one requires updating both `image_utils.py`/`wallpaper.py` **and** the CLI `click.Choice` in `cli.py`.

## Build notes

The PyInstaller spec (`wallpaper_changer.spec`) bundles `config/settings.toml` as a data file and explicitly lists hidden imports for ttkbootstrap and win32 modules. When adding a new dependency that uses dynamic imports, update the `hiddenimports` list in the spec.

The Inno Setup script (`installer.iss`) registers the app for Windows startup via the registry — the same logic lives in `src/wallpaper_changer/startup.py`.
