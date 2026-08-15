# WallpaperChanger

> The most powerful free collage & live video wallpaper manager for Windows. Multi-monitor, multi-language, auto-rotation, smooth transitions, and zero bloat.

[![Tauri](https://img.shields.io/badge/Tauri-2.x-24C8DB?logo=tauri&logoColor=white&style=flat-square)](https://tauri.app)
[![React](https://img.shields.io/badge/React-19-61DAFB?logo=react&logoColor=white&style=flat-square)](https://react.dev)
[![Rust](https://img.shields.io/badge/Rust-2021-000000?logo=rust&logoColor=white&style=flat-square)](https://rust-lang.org)
[![Python](https://img.shields.io/badge/Python-3.11%2B-blue?logo=python&logoColor=white&style=flat-square)](https://python.org)
[![Platform](https://img.shields.io/badge/Platform-Windows%2010%20%7C%2011-0078D4?logo=windows&logoColor=white&style=flat-square)](https://microsoft.com)
[![License](https://img.shields.io/badge/License-MIT-22c55e?style=flat-square)](https://opensource.org/license/mit)
[![Docs](https://img.shields.io/badge/Docs-Live%20Site-df7356?style=flat-square)](https://wallpaper.astrofocus.app/)

---

![WallpaperChanger managing collage and live video wallpapers across three monitors](og.png)

## Why WallpaperChanger?

Most wallpaper utilities are either simplistic slideshows that cannot handle multiple displays, or resource-heavy tools locked behind subscriptions and ads. **WallpaperChanger** is built to bridge this gap, offering premium, highly customizable features completely free, open-source, and offline.

| Feature | Other Wallpaper Utilities | WallpaperChanger Solution |
| :--- | :--- | :--- |
| **Multi-Monitor Layouts** | Single image stretched or forced per screen | **Collage Mode**: custom grid of 1-8 images per monitor |
| **Live Wallpapers** | Restricted, resource-heavy, or paid | **Video Wallpaper**: stable, aspect-correct playback via `libmpv` |
| **Transitions** | Instant, jarring cuts | **Native Fade**: uses the built-in Windows wallpaper transition |
| **System Footprint** | Taskbar hog, telemetry, ads, account required | **Zero Bloat**: runs silently in the system tray, telemetry-free |
| **Language Support** | English only | **Multi-Language**: full English, Português, and Japanese UI |
| **Automation & Scripts** | GUI-only configuration | **CLI Console**: apply wallpapers, play videos, and run watch cycles |

---

## Architecture

The desktop app is two processes that speak newline-delimited JSON-RPC over stdio.

```
+------------------------------------------+
|  desktop/  -  Tauri 2 shell (Rust)        |
|  React 19 + shadcn/ui + Tailwind 4        |
|  Owns: window, tray, global hotkeys       |
+---------------------+--------------------+
                      |  JSON-RPC over stdio
+---------------------v--------------------+
|  src/wallpaper_changer/  -  engine        |
|  Python 3.11+, frozen with PyInstaller    |
|  Owns: everything touching Win32          |
+------------------------------------------+
```

**Why the split.** Every Win32 behaviour that took real effort to get right - the `WorkerW` video layer, layered-window alpha, `SystemParametersInfoW` composition - stays in Python, untouched. The Tauri shell replaces only the interface. Rust owns the global hotkeys so they survive an engine restart and can reach the window itself, and it supervises the engine process; the webview can never spawn anything, it can only call methods on the engine's own allowlist.

| Layer | Technology |
| :--- | :--- |
| **Shell & window** | Tauri 2 (Rust 2021), WebView2 |
| **Interface** | React 19, TypeScript, Vite 8 |
| **Components** | shadcn/ui on Base UI, Tailwind CSS 4, lucide icons |
| **Native integration** | `tauri-plugin-global-shortcut`, `-autostart`, `-dialog`, `-notification`, `-opener`, `-log` |
| **Engine** | Python 3.11+, Pillow, pywin32, screeninfo, python-mpv |
| **Packaging** | PyInstaller sidecar + Tauri bundler (NSIS) |

The legacy ttkbootstrap interface (`uv run wallpaper-changer-gui`) and the CLI still run against the same engine modules.

---

## Features

- **Collage Grid**: automatic grid layouts of 1 to 8 images per monitor.
- **Live Preview**: the interface renders the real composited collage before anything touches your desktop, so effect and fit changes are visible immediately. Monitor outlines are drawn over it, any screen can be zoomed to on its own, the whole thing expands to fill the window, and what you are looking at can be applied as-is.
- **Editable Preview**: drag one picture in the preview onto another to swap them, or click one to choose a different image, before anything is applied.
- **Save & Gallery**: keep any collage as an image file - the whole desktop or a single monitor's share of it, composed at full resolution - and find every one you have saved on the Gallery screen, ready to view again or put straight back on the desktop.
- **Video Wallpaper**: render `.mp4`, `.mkv`, `.webm`, `.mov`, and other common formats behind your desktop icons using `libmpv`, with optional audio, looping, and playlist controls.
- **Stable Video Rendering**: D3D11 presentation with stability-focused software decoding, avoiding fragile hardware-decoder surfaces while retaining smooth GPU composition.
- **Native Fade Transitions**: lets Windows apply its built-in wallpaper fade, with no custom animation loop or desktop flicker.
- **Action Bar**: a persistent bar at the bottom of every screen for apply, previous, next, rotation, and video playback.
- **Auto-Rotation**: watch cycles rotate your backgrounds at a set interval.
- **Window Transparency**: slider control, a toggle hotkey, or hold a modifier of your choice (`Alt`, `Ctrl`, `Shift`, `Win`) and scroll the wheel to fade the focused window.
- **Global Hotkeys**: nothing is bound out of the box - assign only the shortcuts you want, from the Hotkeys screen.
- **System Tray**: closing hides to the notification area, with a right-click menu for the common actions, including starting and stopping rotation.
- **Start with Windows**: on by default from the first launch, and a runtime toggle, so it can be turned off again without reinstalling. A boot-time launch goes straight to the tray; **Start minimized** does the same when you open the app yourself.
- **Resumes Where You Left Off**: rotation and the video wallpaper are remembered as they are switched, and come back on the next launch.
- **Windows Installer**: NSIS installer produced by the Tauri bundler, signed so the app can update itself in place.

---

## Quick Start

### Option A - Installer (recommended)

1. Download the `-setup.exe` from the [GitHub Releases](https://github.com/klysman08/wallpaper-changer-windows/releases) page. From then on the app checks for new releases itself and can install them for you.
2. Run the installer.
3. Point the application at your wallpapers or video directory and press **Apply Now** or **Play video**.

### Option B - From source

```powershell
# 1. Clone the repository
git clone https://github.com/klysman08/wallpaper-changer-windows.git
cd wallpaper-changer-windows

# 2. Sync the Python engine's dependencies
uv sync --dev

# 3. Run the desktop app (spawns the engine through `uv run`, no packaging step)
cd desktop
bun install
bun run tauri dev
```

#### Prerequisites

| Tool | Minimum Version | Reference |
| :--- | :--- | :--- |
| **Windows** | 10 / 11 | - |
| **Python** | 3.11+ | [python.org](https://python.org) |
| **uv** | 0.4+ | [docs.astral.sh/uv](https://docs.astral.sh/uv/) |
| **Rust** | 1.77+ | [rustup.rs](https://rustup.rs) |
| **Bun** | 1.1+ | [bun.sh](https://bun.sh) |
| **WebView2** | Runtime | Preinstalled on Windows 11 |

---

## Detailed Configuration

### 1. Video Wallpapers

Point the app at a directory of background videos. `libmpv` renders each display into the desktop `WorkerW` layer while the app keeps playback controls responsive and tears down native resources safely.

- Supports loop and single playback.
- Optional audio toggle, applied live without restarting playback.
- Aspect ratio preservation (keeps 9:16 vertical clips intact on horizontal displays).
- Previous and next controls keep every monitor on the same playlist item.

### 2. Image Effects

Switch rendering styles on the current collage:

- **Normal**
- **Black & White** (greyscale conversion)
- **Vintage** (sepia styling)
- **HDR** (dynamic contrast enhancement)

### 3. Window Transparency

- Adjust alpha (20 to 255) for any open window with the slider.
- **Scroll to adjust**: turn it on from the Transparency screen, pick `Alt`, `Ctrl`, `Shift`, or `Win`, then hold that key and turn the wheel to fade the focused window. Off by default, because it installs a system-wide mouse hook.
- Opacity is remembered per executable, not per window, so it survives closing and reopening the application.
- Settings persist in `transparency.json` under `%APPDATA%\WallpaperChanger\`.

---

## Global Hotkeys

**No shortcut is bound after installation.** A global hotkey belongs to a single process on Windows, so claiming a dozen combinations on first run would silently take them from applications you already use. Assign the ones you want from the Hotkeys screen; the clear button unbinds one again.

Bindings use the syntax `ctrl+alt+right`, `alt+a`, `ctrl+alt+.`. The suggestions below are what earlier versions shipped with, kept here as a starting point.

| Action | Suggested Shortcut |
| :--- | :--- |
| Next wallpaper | `Ctrl+Alt+Right` |
| Previous wallpaper | `Ctrl+Alt+Left` |
| Stop / start rotation | `Ctrl+Alt+S` |
| Default wallpaper | `Ctrl+Alt+D` |
| Toggle window | `Ctrl+Alt+W` |
| Toggle active window opacity | `Alt+A` |
| Effect: normal | `Ctrl+Alt+1` |
| Effect: black & white | `Ctrl+Alt+2` |
| Effect: vintage | `Ctrl+Alt+3` |
| Effect: HDR | `Ctrl+Alt+4` |
| Toggle video wallpaper | `Ctrl+Alt+V` |
| Toggle video sound | `Ctrl+Alt+M` |
| Next video | `Ctrl+Alt+.` |
| Previous video | `Ctrl+Alt+,` |

If a binding fails to register, another application already owns it - the interface reports which ones did not take.

---

## Command Line Interface

Execute core wallpaper routines from PowerShell or script pipelines:

```powershell
# Apply wallpaper immediately
uv run wallpaper-changer apply

# Apply collage with 6 images per monitor randomly selected
uv run wallpaper-changer apply --collage-count 6 --selection random

# Apply with a custom image effect
uv run wallpaper-changer apply --effect vintage

# Enable watch mode (auto rotation)
uv run wallpaper-changer watch

# Launch video wallpaper playback loop
uv run wallpaper-changer video --folder "C:\Videos\live" --loop
```

---

## Configuration

User files live outside the installation directory, so the app works correctly when installed under `Program Files`:

| Location | Contents |
| :--- | :--- |
| `%APPDATA%\WallpaperChanger\` | `settings.toml`, `state.json`, `transparency.json` |
| `%LOCALAPPDATA%\WallpaperChanger\` | Composed wallpaper output |

On first run the app copies any settings from an older in-install `config/` directory into `%APPDATA%`; it never moves or overwrites. Both locations can be redirected with the `WALLPAPER_CHANGER_CONFIG_DIR` and `WALLPAPER_CHANGER_DATA_DIR` environment variables.

```toml
[general]
mode                 = "collage"
selection            = "random"
interval             = 300
collage_count        = 4
collage_same_for_all = false
language             = "en"

[paths]
wallpapers_folder = "C:\\Users\\Public\\Pictures"
output_folder     = "output"          # relative paths resolve under %LOCALAPPDATA%
default_wallpaper = ""

[display]
fit_mode = "fill"                     # fill | fit | stretch | center | span
effect   = "normal"                   # normal | bw | vintage | hdr

# Empty means "not registered". Assign shortcuts from the Hotkeys screen.
[hotkeys]
next_wallpaper      = ""
prev_wallpaper      = ""
stop_watch          = ""
default_wallpaper   = ""
toggle_transparency = ""
toggle_window       = ""
# Not shortcuts: hold scroll_modifier and turn the wheel to fade the focused
# window. Off by default because it installs a system-wide mouse hook.
scroll_enabled      = false
scroll_modifier     = "alt"           # alt | ctrl | shift | win
effect_normal       = ""
effect_bw           = ""
effect_vintage      = ""
effect_hdr          = ""
toggle_video        = ""
toggle_video_sound  = ""
next_video          = ""
prev_video          = ""

[video]
enabled = false
folder  = "C:/Users/YourName/Videos/Wallpapers"
loop    = false
sound   = true
```

---

## Build Pipelines

The Tauri bundler produces the installers, so there is no separate Inno Setup step and nothing is written to the registry at install time.

```powershell
# Full release: engine sidecar, app, and both installers
.\scripts\build_app.ps1

# Debug binary, no installers (fast smoke test)
.\scripts\build_app.ps1 -NoBundle

# Reuse the already-staged engine
.\scripts\build_app.ps1 -SkipEngine

# Engine sidecar only
.\scripts\build_engine.ps1
```

`build_engine.ps1` freezes the Python engine with PyInstaller, probes the frozen binary to catch missing hidden imports, and stages it into `desktop/src-tauri/engine/`, which Tauri ships as a bundle resource. Installers land in `desktop/src-tauri/target/release/bundle/`.

### Tests and linting

```powershell
uv run pytest                       # Python engine
uv run ruff check src/              # Python lint
cd desktop/src-tauri; cargo test    # Rust shell
cd desktop; bun run build           # Type-check and build the interface
```

---

## Project Structure

```
wallpaper-changer/
├── main.py                       # CLI & legacy GUI entry point
├── main_rpc.py                   # Engine sidecar entry point
├── pyproject.toml                # Python dependencies & metadata
├── wallpaper_changer_rpc.spec    # PyInstaller spec for the sidecar
├── assets/icon/wpaper-logo.png   # Icon source
├── config/settings.toml          # Default settings, seeded into %APPDATA%
├── scripts/
│   ├── build_app.ps1             # Engine + app + installers
│   ├── build_engine.ps1          # Engine sidecar only
│   └── make_icon.py              # Logo to icon source
├── desktop/                      # Tauri 2 desktop application
│   ├── src/
│   │   ├── App.tsx               # Sidebar shell, sections, action bar
│   │   ├── components/           # Screens plus shadcn/ui primitives
│   │   └── lib/                  # Typed engine client and React hooks
│   └── src-tauri/src/
│       ├── lib.rs                # Plugins, setup, engine_call command
│       ├── engine.rs             # Sidecar supervision and JSON-RPC
│       ├── hotkeys.rs            # Global shortcut registration
│       └── tray.rs               # System tray icon and menu
└── src/wallpaper_changer/
    ├── rpc.py                    # JSON-RPC adapter over the engine
    ├── cli.py                    # Command-line router
    ├── config.py                 # TOML parsing and user-directory migration
    ├── gallery.py                # Index of collages saved as image files
    ├── gui.py                    # Legacy ttkbootstrap interface
    ├── hotkeys.py                # Win32 global hotkey registration
    ├── i18n.py                   # Locales (en, pt_BR, ja)
    ├── image_utils.py            # Grids, sizing, and effect modifiers
    ├── monitor.py                # Win32 screen positioning
    ├── transition.py             # Native Windows wallpaper transition
    ├── transparency.py           # Win32 window alpha
    ├── video_wallpaper.py        # libmpv player lifecycle
    ├── workerw.py                # Desktop WorkerW discovery
    └── wallpaper.py              # Canvas rendering and composition
```

---

## Support This Project

WallpaperChanger is free and open-source. If it is useful to you, you can support its development:

- **[Support the project](https://buy.stripe.com/4gMdRa7XW6dt8Ph9KX9Ve01)**
- **[Source code](https://github.com/klysman08/wallpaper-changer-windows)**

Built by [klysman08](https://github.com/klysman08).

---

## License

MIT - free for personal and commercial usage.
