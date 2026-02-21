# WallpaperChanger

> The most powerful free collage wallpaper manager for Windows — multi-monitor, multi-language, auto-rotation, and zero bloat.

![Python](https://img.shields.io/badge/Python-3.11%2B-blue?logo=python)
![Platform](https://img.shields.io/badge/Platform-Windows%2010%20%7C%2011-0078D4?logo=windows)
![License](https://img.shields.io/badge/License-MIT-green)
![Version](https://img.shields.io/badge/Version-3.0-orange)

---

![WallpaperChanger](image.png)

## Why WallpaperChanger?

Most wallpaper apps fall into two categories: simple slideshows that only handle one image at a time, or paid tools locked behind subscriptions. **WallpaperChanger** fills the gap by offering features that usually require premium software — completely free and open-source.

| Problem with other apps | WallpaperChanger solution |
|---|---|
| Single image per screen only | **Collage mode** — grid of 1–8 images per monitor |
| No real multi-monitor awareness | Detects every monitor, respects resolution and position |
| Always visible in the taskbar | Runs silently in the **system tray** |
| English-only interfaces | **3 languages** — English, Português (Brasil), 日本語 |
| No auto-start or background rotation | **Start with Windows** → launches to tray with auto-rotation |
| Paid or ad-supported | 100% free, MIT licensed, no telemetry |
| Complex installers or manual setup | One-click **Windows Installer** with language selection |

---

## Features

| Feature | Description |
|---|---|
| **Collage grid** | Automatic layout with 1 to 8 images per monitor |
| **Same images on all monitors** | Option to replicate the same collage on every screen |
| **Random or sequential selection** | Switch between images randomly or in order |
| **Image fit modes** | Fill, Fit, Stretch, Center, or Span |
| **Auto rotation** | Change wallpaper at configurable intervals (seconds) |
| **Start with Windows** | Launches to system tray with auto-rotation enabled |
| **System tray** | App lives in the notification area — right-click for quick actions |
| **Multi-language GUI** | English, Português (Brasil), 日本語 — switchable in settings |
| **Global hotkeys** | Next / Previous / Stop / Default wallpaper via keyboard shortcuts |
| **Wallpaper history** | Navigate back to previously applied wallpapers |
| **Default wallpaper** | Assign a fallback image applied via hotkey |
| **Windows Installer** | Setup.exe via Inno Setup — includes language selection during install |
| **CLI** | Full command-line control for scripting and automation |

---

## Quick Start

### Option A — Installer (recommended)

1. Download **`WallpaperChanger_Setup.exe`** from the [Releases](https://github.com/klysman08/wallpaper-changer-windows/releases) page
2. Run the installer — choose your language, shortcuts, and startup preference
3. Launch the app and point it at your wallpapers folder

### Option B — From source

```powershell
# 1. Clone the repository
git clone https://github.com/klysman08/wallpaper-changer-windows.git
cd wallpaper-changer-windows/wallpaper-changer

# 2. Create virtual environment and install dependencies
uv sync

# 3. Start the GUI
uv run python -c "from wallpaper_changer.gui import run; run()"
```

### Prerequisites (source only)

| Tool | Min Version | Link |
|---|---|---|
| Windows | 10 / 11 | — |
| Python | 3.11+ | https://python.org |
| uv | 0.4+ | https://docs.astral.sh/uv/ |

---

## Graphical Interface

### Monitor Detection

WallpaperChanger automatically detects all connected monitors, showing a live preview with resolution and position. Click **Detect** to refresh after plugging in a display.

### Collage

Each monitor is divided into an automatic grid with **1 to 8 images**.

- Choose the number of images with the numeric buttons
- Enable **"Same images on all monitors"** to replicate the same set

### Settings

- **Image selection** — `Random` or `Sequential`
- **Screen fit** — `Fill`, `Fit`, `Stretch`, `Center`, `Span`
- **Auto rotation** — set the interval in seconds and click **Start Watch**

### Start with Windows

When this option is enabled, the app registers itself to launch at login. On startup it goes **directly to the system tray** and automatically begins the wallpaper rotation — no window pops up, no interaction needed.

### Language

Switch between **English**, **Português (Brasil)**, and **日本語** from the Language section inside the app. The change is saved immediately; restart the app to apply it fully. The installer also lets you pick the default language during installation.

### Wallpapers Folder

Define the source folder for images.
Supported formats: `jpg`, `jpeg`, `png`, `bmp`, `webp`.

### Global Hotkeys

| Action | Default shortcut |
|---|---|
| Next wallpaper | `Ctrl+Alt+Right` |
| Previous wallpaper | `Ctrl+Alt+Left` |
| Stop/Start Watch | `Ctrl+Alt+S` |
| Default wallpaper | `Ctrl+Alt+D` |

All shortcuts are fully customizable from the GUI.

### System Tray

Closing the window (✕) or clicking **Tray** minimizes the app to the notification area. Right-click the tray icon for: **Show**, **Apply Now**, **Quit**.

---

## CLI

```powershell
# Apply wallpaper immediately
uv run wallpaper-changer apply

# Apply with options
uv run wallpaper-changer apply --collage-count 6 --selection random

# Watch mode (auto change at configured interval)
uv run wallpaper-changer watch
```

---

## Build

### Portable executable (PyInstaller)

```powershell
cd wallpaper-changer
.\scripts\build_exe.ps1 -NoInstaller
```

Result in `dist\WallpaperChanger\`.

### Windows Installer (Inno Setup)

Prerequisite: [Inno Setup 6](https://jrsoftware.org/isinfo.php) installed.

```powershell
cd wallpaper-changer
.\scripts\build_exe.ps1
```

Result: `dist\WallpaperChanger_Setup.exe`.

---

## Project Structure

```
wallpaper-changer/
├── main.py                  # PyInstaller entry point
├── pyproject.toml           # Dependencies and metadata
├── wallpaper_changer.spec   # PyInstaller spec
├── installer.iss            # Inno Setup script
├── config/
│   └── settings.toml        # App settings (language, paths, hotkeys…)
├── scripts/
│   └── build_exe.ps1        # Build script
└── src/wallpaper_changer/
    ├── __init__.py
    ├── cli.py               # Command-line interface
    ├── config.py            # Config read/write (TOML)
    ├── gui.py               # Graphical interface (ttkbootstrap)
    ├── hotkeys.py           # Global hotkey registration
    ├── i18n.py              # Internationalization (en, pt_BR, ja)
    ├── image_utils.py       # Image selection and resizing
    ├── monitor.py           # Monitor detection (Win32)
    ├── startup.py           # Windows startup registration
    └── wallpaper.py         # Wallpaper assembly, and application
```

---

## License

MIT — free for personal and commercial use.
