# libmpv (native dependency for the video wallpaper)

The **video wallpaper** renders video into the desktop `WORKERW` layer using
[libmpv](https://mpv.io/). The engine loads **`libmpv-2.dll`** at runtime, through
`libloading` — deliberately not the `libmpv`/`libmpv2` crates, which link at build
time and would make the DLL a hard requirement of the build.

If the DLL is missing the rest of the app works normally; only the video section is
disabled. `get_capabilities` reports `has_mpv: false`, the video screen says so, and
the `video` CLI command exits with a message.

## Install the DLL

1. Download a Windows libmpv build (64-bit):
   - https://sourceforge.net/projects/mpv-player-windows/files/libmpv/
   - Pick the newest `mpv-dev-x86_64-*.7z` — the `-dev` archive is the one with the
     DLL in it.
2. Extract **`libmpv-2.dll`** from the archive into **this folder** (`libmpv/`).

That is all a checkout needs. `libmpv-2.dll` is about 112 MB, which is most of the
installer's download.

## Where the loader looks

In order, first hit wins (`video::library_candidates`):

1. The directory `video::set_search_dir` was given. In a packaged build the shell
   passes Tauri's resolved resource directory, which is the only place the DLL
   exists there — the core cannot resolve a Tauri path itself.
2. `%MPV_DLL_DIR%` — set this to point at an existing mpv install.
3. `libmpv/` under the project root, then the project root itself. This is the
   checkout case, and it is why extracting the DLL here is enough.
4. `libmpv/` beside the executable, then beside the executable itself.

Each directory is tried for `libmpv-2.dll`, then `mpv-2.dll`, then `mpv-1.dll`.

**The DLL is loaded when playback is first asked for, not at startup.** `has_mpv`
answers from the file's presence, because mapping 112 MB on every launch cost every
user who never plays a video. A DLL that exists but will not load therefore reports
`no_mpv` at `start_inputs` rather than a bare error.

> The DLL is intentionally **not committed** — it is large and platform-specific.
> See `.gitignore`, which ignores everything in this folder except this README.
