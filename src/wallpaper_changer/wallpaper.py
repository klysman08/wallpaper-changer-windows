"""Montagem do wallpaper composto e aplicacao no Windows 11."""
from __future__ import annotations

import ctypes
import math
import time as _time
import winreg
from pathlib import Path

from PIL import Image

from .config import resolve_path, get_project_root
from .image_utils import fit_image, pick_images, build_canvas
from .monitor import Monitor, get_monitors

SPI_SETDESKWALLPAPER  = 0x0014
SPIF_UPDATEINIFILE    = 0x0001
SPIF_SENDWININICHANGE = 0x0002

MODES = ["collage"]


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
    result = ctypes.windll.user32.SystemParametersInfoW(
        SPI_SETDESKWALLPAPER,
        0,
        abs_path,
        SPIF_UPDATEINIFILE | SPIF_SENDWININICHANGE,
    )
    if not result:
        raise RuntimeError("SystemParametersInfoW falhou ao aplicar o wallpaper")


def _get_current_wallpaper() -> Path | None:
    """Le o caminho do wallpaper atual a partir do registro do Windows."""
    try:
        key = winreg.OpenKey(winreg.HKEY_CURRENT_USER, r"Control Panel\Desktop")
        val, _ = winreg.QueryValueEx(key, "Wallpaper")
        winreg.CloseKey(key)
        p = Path(val)
        return p if p.exists() else None
    except Exception:
        return None


def _apply_or_fade(canvas: Image.Image, out: Path, fade_in: bool) -> None:
    """
    Salva o canvas em *out* e aplica como wallpaper.

    Se *fade_in* e True, gera frames intermediarios de transicao suave.
    Usa dois nomes de arquivo alternados (_fade_a / _fade_b) para que o
    Windows perceba um caminho diferente a cada frame e re-leia a imagem.
    """
    if not fade_in:
        canvas.save(str(out), "BMP")
        set_wallpaper_win(out)
        return

    old_path = _get_current_wallpaper()
    if old_path is None:
        canvas.save(str(out), "BMP")
        set_wallpaper_win(out)
        return

    try:
        old_img = Image.open(old_path).convert("RGB")
        if old_img.size != canvas.size:
            old_img = old_img.resize(canvas.size, Image.LANCZOS)
    except Exception:
        canvas.save(str(out), "BMP")
        set_wallpaper_win(out)
        return

    fade_dir = out.parent
    tmp_a = fade_dir / "_fade_a.bmp"
    tmp_b = fade_dir / "_fade_b.bmp"
    n_frames = 8

    for i in range(1, n_frames + 1):
        alpha = i / n_frames
        frame = Image.blend(old_img, canvas, alpha)
        # Alterna entre dois caminhos para forcar o Windows a recarregar
        tmp = tmp_a if (i % 2 == 1) else tmp_b
        frame.save(str(tmp), "BMP")
        set_wallpaper_win(tmp)
        _time.sleep(0.15)

    # Salva a imagem final no destino real
    canvas.save(str(out), "BMP")
    set_wallpaper_win(out)

    # Limpeza dos arquivos temporarios
    for f in (tmp_a, tmp_b):
        try:
            f.unlink()
        except Exception:
            pass


# ── Resolucao de pasta e estado ───────────────────────────────────────────────

def _get_folder(cfg: dict) -> Path:
    return resolve_path(cfg["paths"]["wallpapers_folder"])


def _get_state_file(cfg: dict) -> Path:
    return get_project_root() / "config" / "state.json"


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
) -> Path:
    """
    Collage: cada monitor e preenchido com N imagens em grade automatica.
    N = cfg['general']['collage_count'] (padrao 4, range 1-8).
    O layout e calculado por _compute_grid_layout().
    """
    folder = _get_folder(cfg)
    fit_mode = cfg["display"]["fit_mode"]
    selection = cfg["general"].get("selection", "random")
    sf = _get_state_file(cfg)
    count = max(1, int(cfg["general"].get("collage_count", 4)))
    same_for_all = bool(cfg["general"].get("collage_same_for_all", False))
    fade_in = bool(cfg.get("general", {}).get("fade_in", False))

    # Quantidade de imagens a selecionar
    if same_for_all:
        imgs = pick_images(str(folder), count, selection, sf)
    else:
        imgs = pick_images(str(folder), count * len(monitors), selection, sf)

    min_x, min_y, total_w, total_h = get_virtual_desktop(monitors)
    canvas = build_canvas(total_w, total_h)

    img_idx = 0
    for mon in monitors:
        cells = _compute_grid_layout(count, mon.width, mon.height)
        for j, (cell_x, cell_y, cell_w, cell_h) in enumerate(cells):
            src_idx = j if same_for_all else img_idx
            img = Image.open(imgs[src_idx]).convert("RGB")
            img = fit_image(img, cell_w, cell_h, fit_mode)
            paste_x = (mon.x - min_x) + cell_x
            paste_y = (mon.y - min_y) + cell_y
            canvas.paste(img, (paste_x, paste_y))
            if not same_for_all:
                img_idx += 1

    out = output_dir / "wallpaper_collage.bmp"
    _apply_or_fade(canvas, out, fade_in)
    return out


# ── Entrada principal ─────────────────────────────────────────────────────────

def apply_wallpaper(cfg: dict, monitors: list[Monitor], output_dir: Path) -> Path:
    """Aplica o wallpaper no modo collage."""
    if not monitors:
        raise ValueError("Nenhum monitor detectado.")
    return _apply_collage(cfg, monitors, output_dir)
