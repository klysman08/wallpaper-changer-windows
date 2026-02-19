# WallpaperChanger Improvement Plan

## Phase 1 — Initial Rewrite (completed)

- [x] 1. **Codebase refactoring** — Removed dead modes (clone, split1-4, quad). Cleaned up MODES, `apply_wallpaper()`, GUI, CLI.
- [x] 2. **Replace GUI framework** — Replaced `customtkinter` with `ttkbootstrap` (native ttk, dark theme, fast).
- [x] 3. **Keep only Collage** — Removed mode selection UI. Collage-only.
- [x] 4. **Create Windows installer** — Inno Setup `.iss` script + `build_exe.ps1` pipeline.
- [x] 5. **Add start-with-system** — Registry `HKCU\...\Run` key. GUI checkbox. `startup.py` module.

## Phase 2 — Refinements (completed)

- [x] 6. **Improve GUI responsiveness** — Async folder scan (background thread), debounced monitor redraws, scoped mousewheel events, scroll frame width fix.
- [x] 7. **Persist wallpaper sequence on restart** — Random mode now tracks shown images in `state.json` (`random_history` key). No repeats until full cycle. Survives restarts.
- [x] 8. **Remove fade effect** — Removed all fade transition code (`_apply_or_fade`, `_set_wallpaper_fast`, `_smoothstep`, `_get_current_wallpaper`). Removed fade UI checkbox and `fade_in` config option. Wallpaper is now applied directly without animation.

## Key Decisions

| Decision | Choice | Reason |
|---|---|---|
| GUI framework | `ttkbootstrap` | Native ttk = fast rendering, modern dark themes, lightweight |
| Installer | Inno Setup | Industry standard for Windows, free, scriptable |
| Start with system | Registry `Run` key | Standard Windows autostart, no admin rights needed |
| Fade effect | Removed | Windows `SystemParametersInfoW` can't animate smoothly; removed to keep code clean |
| Random persistence | `state.json` history | Tracks shown images per folder, resets when all shown |
