# WallpaperChanger Improvement Plan

## Analysis Summary

**Current issues:**
- GUI uses `customtkinter` — known for slow startup and sluggish rendering
- 7 wallpaper modes exist but only Collage should remain
- Fade effect sets wallpaper rapidly using same file path — Windows caches by path, so intermediate frames are ignored
- No Windows installer (only PyInstaller --onedir loose folder)
- No "start with system" option

## Tasks

- [x] 1. **Codebase refactoring** — Remove dead modes (clone, split1-4, quad). Clean up MODES, `apply_wallpaper()`, GUI cards, CLI commands. Simplify config defaults.
- [x] 2. **Replace GUI framework** — Replace `customtkinter` with `ttkbootstrap` (native ttk widgets, modern dark theme, much faster rendering). Rewrite `gui.py`.
- [x] 3. **Keep only Collage** — Remove mode selection UI. Always use collage mode. Simplify settings.
- [x] 4. **Fix fade effect** — Use alternating file names so Windows sees new wallpaper each frame. Increase frame delay for visible transition.
- [x] 5. **Create Windows installer** — Inno Setup `.iss` script + updated `build_exe.ps1` pipeline.
- [x] 6. **Add start-with-system** — Registry key in `HKCU\...\Run`. GUI checkbox. `startup.py` module.
- [x] 7. **Verify & test** — All modules import OK, GUI launches, PyInstaller build succeeds (4.6 MB exe).

## Key Decisions

| Decision | Choice | Reason |
|---|---|---|
| GUI framework | `ttkbootstrap` | Native ttk = fast rendering, modern dark themes, lightweight, compatible with existing tk.Canvas code |
| Installer | Inno Setup | Industry standard for Windows installers, free, scriptable |
| Fade fix | Alternating filenames + longer delay | Windows caches wallpaper by path; different paths force re-read |
| Start with system | Registry `Run` key | Standard Windows autostart mechanism, no admin rights needed |
