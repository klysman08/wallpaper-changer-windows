# libmpv (native dependency for video wallpaper)

The **video wallpaper** feature renders video into the desktop WORKERW layer using
[libmpv](https://mpv.io/) via the `python-mpv` binding. `python-mpv` is a pure-Python
ctypes wrapper — it needs the native **`libmpv-2.dll`** at runtime.

If this DLL is missing the rest of the app works normally; only the video-wallpaper
section is disabled (the GUI shows an install hint and the `video` CLI command exits
with a message).

## Install the DLL

1. Download a Windows libmpv build (64-bit, matching your Python):
   - https://sourceforge.net/projects/mpv-player-windows/files/libmpv/
   - Pick the newest `mpv-dev-x86_64-*.7z` (the `-dev` archive contains the DLL).
2. Extract **`libmpv-2.dll`** from the archive into **this folder** (`libmpv/`).

That's it. At startup the app adds this folder to the DLL search path
(`video_wallpaper._prepare_libmpv`), and the PyInstaller build bundles any
`libmpv-2.dll` found here.

## Alternative locations

The loader also searches, in order:

1. `%MPV_DLL_DIR%` — set this env var to a folder containing `libmpv-2.dll`.
2. `libmpv/` (this folder).
3. The project root.

So you can instead drop `libmpv-2.dll` next to the project root, or point
`MPV_DLL_DIR` at an existing mpv install.

> The DLL is intentionally **not committed** (it is large and platform-specific) —
> see `.gitignore`.
