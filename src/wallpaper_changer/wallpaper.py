"""Montagem do wallpaper composto e aplicacao no Windows 11."""
from __future__ import annotations
import ctypes
import math
import winreg
from pathlib import Path
from PIL import Image
from .config import resolve_path, get_project_root
from .image_utils import fit_image, pick_images, build_canvas
from .monitor import Monitor, get_monitors

SPI_SETDESKWALLPAPER  = 0x0014
SPIF_UPDATEINIFILE    = 0x0001
SPIF_SENDWININICHANGE = 0x0002

# Modos suportados
MODES = ["clone", "split1", "split2", "split3", "split4", "quad", "collage"]


# ── Utilitarios Windows ───────────────────────────────────────────────────────

def get_virtual_desktop(monitors: list[Monitor]) -> tuple[int, int, int, int]:
    """Retorna (min_x, min_y, total_width, total_height) do desktop virtual."""
    min_x = min(m.x for m in monitors)
    min_y = min(m.y for m in monitors)
    max_x = max(m.x + m.width  for m in monitors)
    max_y = max(m.y + m.height for m in monitors)
    return min_x, min_y, max_x - min_x, max_y - min_y


def set_wallpaper_style_span() -> None:
    """Configura o Windows para exibir o wallpaper em modo span (estendido)."""
    key = winreg.OpenKey(
        winreg.HKEY_CURRENT_USER,
        r"Control Panel\Desktop",
        0, winreg.KEY_SET_VALUE,
    )
    winreg.SetValueEx(key, "WallpaperStyle", 0, winreg.REG_SZ, "22")
    winreg.SetValueEx(key, "TileWallpaper",  0, winreg.REG_SZ, "0")
    winreg.CloseKey(key)


def set_wallpaper_win(path: str | Path) -> None:
    """Aplica o arquivo de imagem como wallpaper no Windows."""
    abs_path = str(Path(path).resolve())
    set_wallpaper_style_span()
    result = ctypes.windll.user32.SystemParametersInfoW(
        SPI_SETDESKWALLPAPER, 0, abs_path,
        SPIF_UPDATEINIFILE | SPIF_SENDWININICHANGE,
    )
    if not result:
        raise RuntimeError("SystemParametersInfoW falhou ao aplicar o wallpaper")


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


# Mapeamento conteudo -> colunas preferidas para collage
_GRID_COLS: dict[int, int] = {
    1: 1, 2: 2, 3: 2, 4: 2, 5: 3, 6: 3, 7: 4, 8: 4, 9: 3,
}


def _compute_grid_layout(n: int, w: int, h: int) -> list[tuple[int, int, int, int]]:
    """
    Divide a area w×h em n celulas usando grade dinamica adaptada ao aspecto.
    Retorna lista de (x, y, cell_w, cell_h).
    A ultima linha e centralizada quando tem menos celulas que as demais.
    """
    cols  = _GRID_COLS.get(n, max(1, math.ceil(math.sqrt(n))))
    rows  = math.ceil(n / cols)
    cell_h = h // rows
    cells: list[tuple[int, int, int, int]] = []
    placed = 0
    for r in range(rows):
        row_cols = min(cols, n - placed)
        cell_w   = w // row_cols
        offset_x = (w - row_cols * cell_w) // 2
        for c in range(row_cols):
            cells.append((offset_x + c * cell_w, r * cell_h, cell_w, cell_h))
            placed += 1
    return cells


# ── Modos ─────────────────────────────────────────────────────────────────────

def _apply_clone(
    cfg: dict,
    monitors: list[Monitor],
    output_dir: Path,
) -> Path:
    """
    Clone: a mesma imagem e replicada em todos os monitores.
    Cada monitor recebe a imagem adaptada ao seu tamanho individualmente.
    """
    folder    = _get_folder(cfg)
    fit_mode  = cfg["display"]["fit_mode"]
    selection = cfg["general"].get("selection", "random")
    sf        = _get_state_file(cfg)

    imgs = pick_images(str(folder), 1, selection, sf)
    src  = Image.open(imgs[0]).convert("RGB")
    print(f"  [clone] Imagem: {imgs[0].name}")

    sections: list[tuple[Monitor, Image.Image]] = []
    for mon in monitors:
        sections.append((mon, fit_image(src.copy(), mon.width, mon.height, fit_mode)))
        print(f"    -> Monitor {mon.index + 1} ({mon.width}x{mon.height})")

    canvas = _build_canvas_from_sections(monitors, sections)
    out = output_dir / "wallpaper_clone.bmp"
    canvas.save(str(out), "BMP")
    set_wallpaper_win(out)
    return out


def _apply_split(
    cfg: dict,
    monitors: list[Monitor],
    output_dir: Path,
    n: int,
) -> Path:
    """
    Split-N: N imagens diferentes, uma por monitor.
    - split1: 1 imagem que cobre todo o desktop virtual (efeito span).
    - split2-4: imagem diferente em cada um dos N primeiros monitores.
    """
    folder    = _get_folder(cfg)
    fit_mode  = cfg["display"]["fit_mode"]
    selection = cfg["general"].get("selection", "random")
    sf        = _get_state_file(cfg)

    if n == 1:
        # Imagem unica cobre todo o desktop virtual
        _, _, total_w, total_h = get_virtual_desktop(monitors)
        imgs = pick_images(str(folder), 1, selection, sf)
        img  = Image.open(imgs[0]).convert("RGB")
        img  = fit_image(img, total_w, total_h, fit_mode)
        print(f"  [split1] {imgs[0].name} -> canvas {total_w}x{total_h}")
        out = output_dir / "wallpaper_split1.bmp"
        img.save(str(out), "BMP")
        set_wallpaper_win(out)
        return out

    # split2 / split3 / split4
    num_slots = min(n, len(monitors))
    imgs      = pick_images(str(folder), num_slots, selection, sf)
    sections: list[tuple[Monitor, Image.Image]] = []

    for i, mon in enumerate(monitors[:num_slots]):
        img = Image.open(imgs[i]).convert("RGB")
        img = fit_image(img, mon.width, mon.height, fit_mode)
        sections.append((mon, img))
        print(f"  [split{n}] Monitor {mon.index + 1} ({mon.width}x{mon.height}) <- {imgs[i].name}")

    canvas = _build_canvas_from_sections(monitors, sections)
    out = output_dir / f"wallpaper_split{n}.bmp"
    canvas.save(str(out), "BMP")
    set_wallpaper_win(out)
    return out


def _apply_quad(
    cfg: dict,
    monitors: list[Monitor],
    output_dir: Path,
) -> Path:
    """
    Quad: cada monitor e dividido em 4 quadrantes iguais (2x2).
    Cada quadrante recebe uma imagem diferente conforme o modo de selecao.
    Para N monitores sao selecionadas N*4 imagens no total.
    """
    folder    = _get_folder(cfg)
    fit_mode  = cfg["display"]["fit_mode"]
    selection = cfg["general"].get("selection", "random")
    sf        = _get_state_file(cfg)

    total_imgs = 4 * len(monitors)
    imgs       = pick_images(str(folder), total_imgs, selection, sf)

    min_x, min_y, total_w, total_h = get_virtual_desktop(monitors)
    canvas = build_canvas(total_w, total_h)

    img_idx = 0
    for mon in monitors:
        qw = mon.width  // 2
        qh = mon.height // 2
        # ordem: cima-esquerda, cima-direita, baixo-esquerda, baixo-direita
        offsets = [(0, 0), (qw, 0), (0, qh), (qw, qh)]
        labels  = ["TL", "TR", "BL", "BR"]
        for label, (qx, qy) in zip(labels, offsets):
            img = Image.open(imgs[img_idx]).convert("RGB")
            img = fit_image(img, qw, qh, fit_mode)
            paste_x = (mon.x - min_x) + qx
            paste_y = (mon.y - min_y) + qy
            canvas.paste(img, (paste_x, paste_y))
            print(f"  [quad] M{mon.index + 1}-{label} ({qw}x{qh}) <- {imgs[img_idx].name}")
            img_idx += 1

    out = output_dir / "wallpaper_quad.bmp"
    canvas.save(str(out), "BMP")
    set_wallpaper_win(out)
    return out


def _apply_collage(
    cfg: dict,
    monitors: list[Monitor],
    output_dir: Path,
) -> Path:
    """
    Collage: cada monitor e preenchido com N imagens em grade automatica.
    N = cfg['general']['collage_count'] (padrao 4, range 2-9).
    O layout e calculado por _compute_grid_layout().
    """
    folder    = _get_folder(cfg)
    fit_mode  = cfg["display"]["fit_mode"]
    selection = cfg["general"].get("selection", "random")
    sf        = _get_state_file(cfg)
    count     = max(1, int(cfg["general"].get("collage_count", 4)))

    total_imgs = count * len(monitors)
    imgs       = pick_images(str(folder), total_imgs, selection, sf)

    min_x, min_y, total_w, total_h = get_virtual_desktop(monitors)
    canvas = build_canvas(total_w, total_h)

    img_idx = 0
    for mon in monitors:
        cells = _compute_grid_layout(count, mon.width, mon.height)
        for cell_x, cell_y, cell_w, cell_h in cells:
            img = Image.open(imgs[img_idx]).convert("RGB")
            img = fit_image(img, cell_w, cell_h, fit_mode)
            paste_x = (mon.x - min_x) + cell_x
            paste_y = (mon.y - min_y) + cell_y
            canvas.paste(img, (paste_x, paste_y))
            print(f"  [collage] M{mon.index + 1} ({cell_w}x{cell_h}) <- {imgs[img_idx].name}")
            img_idx += 1

    out = output_dir / "wallpaper_collage.bmp"
    canvas.save(str(out), "BMP")
    set_wallpaper_win(out)
    return out


# ── Entrada principal ─────────────────────────────────────────────────────────

def apply_wallpaper(cfg: dict, monitors: list[Monitor], output_dir: Path) -> Path:
    """
    Despacha para o modo correto com base em cfg['general']['mode'].
    Modos: clone | split1 | split2 | split3 | split4 | quad | collage
    """
    if not monitors:
        raise ValueError("Nenhum monitor detectado.")

    mode = cfg["general"]["mode"]

    if mode == "clone":
        return _apply_clone(cfg, monitors, output_dir)
    elif mode == "split1":
        return _apply_split(cfg, monitors, output_dir, 1)
    elif mode == "split2":
        return _apply_split(cfg, monitors, output_dir, 2)
    elif mode == "split3":
        return _apply_split(cfg, monitors, output_dir, 3)
    elif mode == "split4":
        return _apply_split(cfg, monitors, output_dir, 4)
    elif mode == "quad":
        return _apply_quad(cfg, monitors, output_dir)
    elif mode == "collage":
        return _apply_collage(cfg, monitors, output_dir)
    else:
        raise ValueError(f"Modo invalido: '{mode}'. Use: {MODES}")


# ── Aliases de compatibilidade (CLI legado) ───────────────────────────────────

def apply_random(cfg: dict, monitors: list[Monitor], output_dir: Path) -> Path:
    """Compat: equivale a split1 com selection=random."""
    cfg = {**cfg, "general": {**cfg["general"], "mode": "split1", "selection": "random"}}
    return apply_wallpaper(cfg, monitors, output_dir)


def apply_split(cfg: dict, monitors: list[Monitor], output_dir: Path, splits: int = 2) -> Path:
    """Compat: encaminha para apply_wallpaper com split{splits}."""
    cfg = {**cfg, "general": {**cfg["general"], "mode": f"split{splits}"}}
    return apply_wallpaper(cfg, monitors, output_dir)
