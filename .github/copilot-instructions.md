<!-- Copilot / AI agent instructions for contributors and automated agents -->
# Project summary

This repository implements a Windows wallpaper changer (split-screen and random modes). The CLI entrypoint is provided by `src/wallpaper_changer/cli.py` and exposed as the `wallpaper-changer` script in [pyproject.toml](pyproject.toml).

# Big-picture architecture

- CLI layer: [src/wallpaper_changer/cli.py](src/wallpaper_changer/cli.py) — `click` commands `apply` and `watch` drive behavior.
- Configuration: [config/settings.toml](config/settings.toml) — loaded via `load_config()` in [src/wallpaper_changer/config.py](src/wallpaper_changer/config.py). The code resolves a default config path relative to the package root.
- Core image logic: [src/wallpaper_changer/image_utils.py](src/wallpaper_changer/image_utils.py) — image listing, random selection, and resizing (`fit_mode`).
- Composition & OS integration: [src/wallpaper_changer/wallpaper.py](src/wallpaper_changer/wallpaper.py) — builds final BMP image and calls the Windows API (`SystemParametersInfoW`).
- Monitor detection: [src/wallpaper_changer/monitor.py](src/wallpaper_changer/monitor.py) — uses `screeninfo` to enumerate displays and compute virtual desktop size.

# Key workflows & commands

- Run CLI (examples in the top-level README):

  - Apply immediately:

    ```powershell
    uv run wallpaper-changer apply
    uv run wallpaper-changer apply --mode split2
    ```

  - Watch mode (scheduled updates):

    ```powershell
    uv run wallpaper-changer watch
    ```

  Note: the script entry is defined in [pyproject.toml](pyproject.toml) as `wallpaper-changer = "wallpaper_changer.cli:main"`.

# Project-specific conventions & patterns

- src-layout: uses `src/` packaging. Imports use `from wallpaper_changer import ...` and package metadata is in [pyproject.toml](pyproject.toml).
- Configuration resolution: `load_config()` resolves `config/settings.toml` relative to package location (see `DEFAULT_CONFIG` in [src/wallpaper_changer/config.py](src/wallpaper_changer/config.py)). When running, prefer passing `--config` to override.
- Image formats & output: final images are saved as BMP files (Windows API compatibility) into the folder set by `paths.output_folder` in the config (default `assets/output`). `fit_mode` options are `fill`, `fit`, `stretch`, `center` (see [image_utils.py](src/wallpaper_changer/image_utils.py)).
- Error handling patterns:
  - Missing config -> `FileNotFoundError` (raised by `load_config`).
  - Missing images for a section -> logged as a warning/printed and skipped (see loop in `apply_split` in [wallpaper.py](src/wallpaper_changer/wallpaper.py)).

# Integration points & external dependencies

- OS: calls `ctypes.windll.user32.SystemParametersInfoW` (Windows-only). AI agents must not attempt to run these calls during dry analysis; treat them as side-effects.
- Key third-party packages (declared in [pyproject.toml](pyproject.toml)): `Pillow`, `pywin32`, `pystray`, `schedule`, `click`, `screeninfo`.
- Packaging/build: `hatchling` is the build backend (see [pyproject.toml](pyproject.toml)).

# What an AI agent should do first

1. Read the CLI in [src/wallpaper_changer/cli.py](src/wallpaper_changer/cli.py) to understand entry points and flags (`--mode`, `--config`).
2. Inspect [config/settings.toml](config/settings.toml) to find configurable paths and defaults (especially `paths.random_folder` and `paths.monitor_*`).
3. For changes touching image generation, review `image_utils.py` and `wallpaper.py` together to ensure output remains BMP and uses `fit_mode` consistently.
4. Avoid executing `set_wallpaper_win()` on CI or analysis runs — mock or stub calls that hit `ctypes.windll`.

# Examples of in-repo references

- To change the default mode: edit [config/settings.toml](config/settings.toml) `general.mode`.
- To override config at runtime: `uv run wallpaper-changer apply --config path/to/settings.toml`.
- To add a wallpaper source for monitor 1: add images to `assets/wallpapers/monitor_1` and keep file extensions in `{.jpg,.jpeg,.png,.bmp,.webp}`.

# Notes for patching and PRs

- Keep changes small and focused: preserve existing CLI flags and config keys unless a breaking change is intentionally introduced and documented.
- When adding behavior that would call the Windows API, provide a non-destructive test mode or a clear mock for CI.

If any section is unclear or you'd like me to add examples (unit-test stubs, mocking patterns, or a small CI-safe runner), tell me which area to expand.
