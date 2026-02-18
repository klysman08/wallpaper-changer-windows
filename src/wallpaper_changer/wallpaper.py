"""Montagem do wallpaper composto e aplicacao no Windows 11."""
from __future__ import annotations
import ctypes
import winreg
from pathlib import Path
from PIL import Image
from .image_utils import fit_image, pick_random, build_canvas
from .monitor import Monitor

SPI_SETDESKWALLPAPER  = 0x0014
SPIF_UPDATEINIFILE    = 0x0001
SPIF_SENDWININICHANGE = 0x0002

def get_virtual_desktop(monitors: list[Monitor]) -> tuple[int, int, int, int]:
    """Retorna (min_x, min_y, total_width, total_height) do desktop virtual."""
    min_x = min(m.x for m in monitors)
    min_y = min(m.y for m in monitors)
    max_x = max(m.x + m.width  for m in monitors)
    max_y = max(m.y + m.height for m in monitors)
    return min_x, min_y, max_x - min_x, max_y - min_y

def set_wallpaper_style_span() -> None:
    key = winreg.OpenKey(
        winreg.HKEY_CURRENT_USER,
        r"Control Panel\Desktop",
        0, winreg.KEY_SET_VALUE
    )
    winreg.SetValueEx(key, "WallpaperStyle", 0, winreg.REG_SZ, "22")
    winreg.SetValueEx(key, "TileWallpaper",  0, winreg.REG_SZ, "0")
    winreg.CloseKey(key)

def set_wallpaper_win(path: str | Path) -> None:
    abs_path = str(Path(path).resolve())
    set_wallpaper_style_span()
    result = ctypes.windll.user32.SystemParametersInfoW(
        SPI_SETDESKWALLPAPER, 0, abs_path,
        SPIF_UPDATEINIFILE | SPIF_SENDWININICHANGE,
    )
    if not result:
        raise RuntimeError("Falha ao aplicar o wallpaper via SystemParametersInfoW")

def build_wallpaper_canvas(
    monitors: list[Monitor],
    sections: list[tuple[Monitor, str]],
    fit_mode: str
) -> tuple[Image.Image, int, int]:
    """
    Monta o canvas no tamanho exato do desktop virtual.
    Cada monitor recebe sua imagem posicionada nas coordenadas reais.
    Retorna (canvas, offset_x, offset_y).
    """
    min_x, min_y, total_w, total_h = get_virtual_desktop(monitors)
    canvas = build_canvas(total_w, total_h)
    for monitor, folder in sections:
        try:
            img_path = pick_random(folder)
            img = Image.open(img_path).convert("RGB")
            img = fit_image(img, monitor.width, monitor.height, fit_mode)
            # Ajusta coordenadas relativas ao desktop virtual
            paste_x = monitor.x - min_x
            paste_y = monitor.y - min_y
            canvas.paste(img, (paste_x, paste_y))
            print(f"  [+] Monitor {monitor.index} ({monitor.width}x{monitor.height}) <- {img_path.name}")
        except FileNotFoundError as e:
            print(f"  [AVISO] {e}")
    return canvas, min_x, min_y

def apply_random(config: dict, monitors: list[Monitor], output_dir: Path) -> Path:
    """Uma unica imagem espalhada por todos os monitores."""
    min_x, min_y, total_w, total_h = get_virtual_desktop(monitors)
    fit_mode = config["display"]["fit_mode"]
    img_path = pick_random(config["paths"]["random_folder"])
    img = Image.open(img_path).convert("RGB")
    img = fit_image(img, total_w, total_h, fit_mode)
    print(f"  [+] Imagem: {img_path.name} -> canvas {total_w}x{total_h}")
    out = output_dir / "wallpaper_random.bmp"
    img.save(str(out), "BMP")
    set_wallpaper_win(out)
    return out

def apply_split(config: dict, monitors: list[Monitor], output_dir: Path, splits: int = 2) -> Path:
    """Imagem diferente por monitor (split2) ou por quadrante (split4)."""
    fit_mode = config["display"]["fit_mode"]
    min_x, min_y, total_w, total_h = get_virtual_desktop(monitors)

    if splits == 2:
        # Um wallpaper por monitor fisico (usa posicoes reais)
        mon0 = monitors[0] if len(monitors) > 0 else Monitor(0, min_x, min_y, total_w // 2, total_h)
        mon1 = monitors[1] if len(monitors) > 1 else Monitor(1, min_x + total_w // 2, min_y, total_w // 2, total_h)
        sections = [
            (mon0, config["paths"]["monitor_1"]),
            (mon1, config["paths"]["monitor_2"]),
        ]
    else:
        # Divide o desktop virtual em 4 quadrantes iguais
        half_w, half_h = total_w // 2, total_h // 2
        sections = [
            (Monitor(0, min_x,          min_y,          half_w, half_h), config["paths"]["monitor_1"]),
            (Monitor(1, min_x + half_w, min_y,          half_w, half_h), config["paths"]["monitor_2"]),
            (Monitor(2, min_x,          min_y + half_h, half_w, half_h), config["paths"]["monitor_3"]),
            (Monitor(3, min_x + half_w, min_y + half_h, half_w, half_h), config["paths"]["monitor_4"]),
        ]

    canvas, _, _ = build_wallpaper_canvas(monitors, sections, fit_mode)
    out = output_dir / f"wallpaper_split{splits}.bmp"
    canvas.save(str(out), "BMP")
    set_wallpaper_win(out)
    return out
