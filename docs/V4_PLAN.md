# WallpaperChanger 4.0

## Product overview

WallpaperChanger is a Windows 10/11 desktop and command-line application for
multi-monitor wallpaper management. Its primary model is a persisted TOML
configuration covering general rotation, paths, display rendering, shortcuts, and
video playback. Runtime state adds detected monitors, random-selection history,
wallpaper history, window opacity rules, and the active video player.

The application supports image collages of one to eight images per monitor, random
or sequential selection, five fit modes, four effects, timed rotation, a default
wallpaper, global shortcuts, system-tray operation, Windows startup, per-application
transparency, and hardware-accelerated video wallpaper through libmpv. The same image
pipeline is exposed through the CLI.

## Reliability findings

The previous GUI allowed keyboard, tray, mouse-listener, scan, scheduler, and render
threads to call Tk methods. Tk is single-threaded; this explained both intermittent
crashes and shortcuts that appeared to do nothing. Repeated shortcut presses could
also start overlapping wallpaper renders. Shortcut duplicates were silently lost in
a dictionary, registrations were removed by text instead of their returned handles,
and settings were written directly to the live TOML file.

## Delivered plan

1. Route all external callbacks through a main-thread event queue.
2. Run automatic rotation with Tk timers and serialize wallpaper application.
3. Normalize and validate shortcuts, report conflicts, register on key release, keep
   registration handles, and suspend shortcuts during recording.
4. Save settings atomically using a flushed temporary file and `os.replace`.
5. Capture rotating crash logs in
   `%LOCALAPPDATA%\\WallpaperChanger\\logs\\wallpaper-changer.log`.
6. Replace the single long settings page with Wallpaper, Video, and Tools & Shortcuts
   tabs while keeping primary actions and status visible.
7. Add regression coverage for shortcut normalization, duplicates, registration
   lifecycle, and settings persistence.
