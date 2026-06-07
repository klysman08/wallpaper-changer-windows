"""Montagem do wallpaper composto e aplicacao no Windows 11."""
from __future__ import annotations

import ctypes
import math
import winreg
from pathlib import Path

from PIL import Image, ImageEnhance, ImageFilter, ImageOps

from .config import resolve_path, get_project_root
from .image_utils import fit_image, pick_images, build_canvas
from .monitor import Monitor
from .transition import apply_transition

SPI_SETDESKWALLPAPER  = 0x0014
SPIF_UPDATEINIFILE    = 0x0001
SPIF_SENDWININICHANGE = 0x0002

MODES = ["collage"]
EFFECTS = ("normal", "bw", "vintage", "hdr")


# ── Utilitarios Windows ───────────────────────────────────────────────────────

def get_virtual_desktop(monitors: list[Monitor]) -> tuple[int, int, int, int]:
    """Retorna (min_x, min_y, total_width, total_height) do desktop virtual."""
    min_x = min(m.x for m in monitors)
    min_y = min(m.y for m in monitors)
    max_x = max(m.x + m.width for m in monitors)
    max_y = max(m.y + m.height for m in monitors)
    return min_x, min_y, max_x - min_x, max_y - min_y


def set_wallpaper_style_span() -> None:
    """Configura o Windows para exibir o wallpaper em modo span (estendido)."""
    key = winreg.OpenKey(
        winreg.HKEY_CURRENT_USER,
        r"Control Panel\Desktop",
        0,
        winreg.KEY_SET_VALUE,
    )
    winreg.SetValueEx(key, "WallpaperStyle", 0, winreg.REG_SZ, "22")
    winreg.SetValueEx(key, "TileWallpaper", 0, winreg.REG_SZ, "0")
    winreg.CloseKey(key)


def set_wallpaper_win(path: str | Path) -> None:
    """Aplica o arquivo de imagem como wallpaper no Windows."""
    abs_path = str(Path(path).resolve())
    set_wallpaper_style_span()
    # SPIF_SENDWININICHANGE omitted intentionally: it broadcasts WM_SETTINGCHANGE which
    # causes Explorer to run its own animated crossfade over WorkerW, making the system
    # fade visible even when transition="none". SPIF_UPDATEINIFILE is sufficient to
    # persist the path; SystemParametersInfoW itself applies the wallpaper immediately.
    result = ctypes.windll.user32.SystemParametersInfoW(
        SPI_SETDESKWALLPAPER,
        0,
        abs_path,
        SPIF_UPDATEINIFILE,
    )
    if not result:
        raise RuntimeError("SystemParametersInfoW falhou ao aplicar o wallpaper")




# ── Resolucao de pasta e estado ───────────────────────────────────────────────

def _get_folder(cfg: dict) -> Path:
    return resolve_path(cfg["paths"]["wallpapers_folder"])


def _get_state_file(cfg: dict) -> Path:
    return get_project_root() / "config" / "state.json"


def apply_effect(canvas: Image.Image, effect: str = "normal") -> Image.Image:
    """Apply a visual effect to the final canvas before saving it as wallpaper."""
    if effect == "normal":
        return canvas
    if effect == "bw":
        return ImageOps.grayscale(canvas).convert("RGB")
    if effect == "vintage":
        gray = ImageOps.grayscale(canvas)
        sepia = ImageOps.colorize(gray, black="#3b2a1a", white="#d8c3a5").convert("RGB")
        return ImageEnhance.Color(sepia).enhance(0.9)
    if effect == "hdr":
        enhanced = ImageEnhance.Contrast(canvas).enhance(1.35)
        enhanced = ImageEnhance.Sharpness(enhanced).enhance(1.45)
        return enhanced.filter(ImageFilter.DETAIL)
    raise ValueError(f"Efeito de imagem invalido: {effect}")


# ── Montagem do canvas ────────────────────────────────────────────────────────

def _build_canvas_from_sections(
    monitors: list[Monitor],
    sections: list[tuple[Monitor, Image.Image]],
) -> Image.Image:
    """Cola os trechos de imagem no canvas do virtual desktop."""
    min_x, min_y, total_w, total_h = get_virtual_desktop(monitors)
    canvas = build_canvas(total_w, total_h)
    for monitor, img in sections:
        paste_x = monitor.x - min_x
        paste_y = monitor.y - min_y
        canvas.paste(img, (paste_x, paste_y))
    return canvas


# ── Layout de grade para collage ──────────────────────────────────────────────

_GRID_COLS: dict[int, int] = {
    1: 1, 2: 2, 3: 2, 4: 2, 5: 3, 6: 3, 7: 4, 8: 4, 9: 3,
}


def _compute_grid_layout(n: int, w: int, h: int) -> list[tuple[int, int, int, int]]:
    """
    Divide a area w x h em n celulas usando grade dinamica adaptada ao aspecto.
    Retorna lista de (x, y, cell_w, cell_h).
    A ultima linha e centralizada quando tem menos celulas que as demais.
    """
    cols = _GRID_COLS.get(n, max(1, math.ceil(math.sqrt(n))))
    rows = math.ceil(n / cols)
    cell_h = h // rows
    cells: list[tuple[int, int, int, int]] = []
    placed = 0
    for r in range(rows):
        row_cols = min(cols, n - placed)
        cell_w = w // row_cols
        offset_x = (w - row_cols * cell_w) // 2
        for c in range(row_cols):
            cells.append((offset_x + c * cell_w, r * cell_h, cell_w, cell_h))
            placed += 1
    return cells


# ── Collage ───────────────────────────────────────────────────────────────────

def _apply_collage(
    cfg: dict,
    monitors: list[Monitor],
    output_dir: Path,
    preset_images: list[str] | None = None,
) -> tuple[Path, list[str]]:
    """
    Collage: cada monitor e preenchido com N imagens em grade automatica.
    N = cfg['general']['collage_count'] (padrao 4, range 1-8).
    O layout e calculado por _compute_grid_layout().
    """
    folder = _get_folder(cfg)
    fit_mode = cfg["display"]["fit_mode"]
    effect = cfg["display"].get("effect", "normal")
    selection = cfg["general"].get("selection", "random")
    sf = _get_state_file(cfg)
    count = max(1, int(cfg["general"].get("collage_count", 4)))
    same_for_all = bool(cfg["general"].get("collage_same_for_all", False))

    # Quantidade de imagens a selecionar
    if preset_images:
        imgs = [Path(p) for p in preset_images]
    elif same_for_all:
        imgs = pick_images(str(folder), count, selection, sf)
    else:
        imgs = pick_images(str(folder), count * len(monitors), selection, sf)

    min_x, min_y, total_w, total_h = get_virtual_desktop(monitors)
    canvas = build_canvas(total_w, total_h)

    # When same_for_all, pre-load each source image once per unique cell size
    # to avoid redundant File I/O and fit_image processing across monitors.
    _fitted_cache: dict[tuple, Image.Image] = {}

    img_idx = 0
    for mon in monitors:
        cells = _compute_grid_layout(count, mon.width, mon.height)
        for j, (cell_x, cell_y, cell_w, cell_h) in enumerate(cells):
            src_idx = j if same_for_all else img_idx
            cache_key = (src_idx, cell_w, cell_h)
            if cache_key not in _fitted_cache:
                raw = Image.open(imgs[src_idx]).convert("RGB")
                _fitted_cache[cache_key] = fit_image(raw, cell_w, cell_h, fit_mode)
            img = _fitted_cache[cache_key]
            paste_x = (mon.x - min_x) + cell_x
            paste_y = (mon.y - min_y) + cell_y
            canvas.paste(img, (paste_x, paste_y))
            if not same_for_all:
                img_idx += 1

    out = output_dir / "wallpaper_collage.bmp"
    canvas = apply_effect(canvas, effect)
    apply_transition(canvas, out, set_wallpaper_win)
    return out, [str(p) for p in imgs]


# ── Wallpaper padrao (imagem unica) ──────────────────────────────────────────

def apply_single_wallpaper(
    image_path: str | Path,
    monitors: list[Monitor],
    output_dir: Path,
    fit_mode: str = "fill",
    effect: str = "normal",
) -> Path:
    """Apply a single image as wallpaper across all monitors."""
    img = Image.open(str(image_path)).convert("RGB")
    min_x, min_y, total_w, total_h = get_virtual_desktop(monitors)
    canvas = build_canvas(total_w, total_h)
    for mon in monitors:
        fitted = fit_image(img, mon.width, mon.height, fit_mode)
        paste_x = mon.x - min_x
        paste_y = mon.y - min_y
        canvas.paste(fitted, (paste_x, paste_y))
    out = output_dir / "wallpaper_default.bmp"
    canvas = apply_effect(canvas, effect)
    apply_transition(canvas, out, set_wallpaper_win)
    return out


# ── Entrada principal ─────────────────────────────────────────────────────────

def apply_wallpaper(
    cfg: dict,
    monitors: list[Monitor],
    output_dir: Path,
    preset_images: list[str] | None = None,
) -> tuple[Path, list[str]]:
    """Aplica o wallpaper no modo collage.

    Returns:
        (output_path, list_of_image_paths_used)
    """
    if not monitors:
        raise ValueError("Nenhum monitor detectado.")
    return _apply_collage(cfg, monitors, output_dir, preset_images)
