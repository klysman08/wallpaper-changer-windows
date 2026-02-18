"""Montagem do wallpaper composto e aplicacao no Windows 11."""
from __future__ import annotations
import ctypes
import winreg
from pathlib import Path
from PIL import Image
from .config import resolve_path
from .image_utils import fit_image, pick_random, build_canvas
from .monitor import Monitor, get_monitors

SPI_SETDESKWALLPAPER  = 0x0014
SPIF_UPDATEINIFILE    = 0x0001
SPIF_SENDWININICHANGE = 0x0002

# Chaves de pasta dos monitores no config (1-based)
MONITOR_PATH_KEYS = ["monitor_1", "monitor_2", "monitor_3", "monitor_4"]


def get_virtual_desktop(monitors: list[Monitor]) -> tuple[int, int, int, int]:
    """Retorna (min_x, min_y, total_width, total_height) do desktop virtual."""
    min_x = min(m.x for m in monitors)
    min_y = min(m.y for m in monitors)
    max_x = max(m.x + m.width  for m in monitors)
    max_y = max(m.y + m.height for m in monitors)
    return min_x, min_y, max_x - min_x, max_y - min_y


def set_wallpaper_style_span() -> None:
    """Configura o Windows para exibir o wallpaper em modo 'span' (estendido)."""
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
        raise RuntimeError("Falha ao aplicar o wallpaper via SystemParametersInfoW")


def _resolve_folder(cfg: dict, key: str) -> str:
    """Retorna o caminho absoluto de uma pasta definida no config."""
    return str(resolve_path(cfg["paths"][key]))


def build_wallpaper_canvas(
    monitors: list[Monitor],
    sections: list[tuple[Monitor, str]],
    fit_mode: str,
) -> Image.Image:
    """
    Monta o canvas no tamanho exato do desktop virtual.
    Cada section (monitor, pasta) recebe uma imagem aleatoria da pasta,
    ajustada e colada nas coordenadas reais do monitor.
    Monitores nao presentes em sections recebem fundo preto.
    """
    min_x, min_y, total_w, total_h = get_virtual_desktop(monitors)
    canvas = build_canvas(total_w, total_h)

    for monitor, folder in sections:
        try:
            img_path = pick_random(folder)
            img = Image.open(img_path).convert("RGB")
            img = fit_image(img, monitor.width, monitor.height, fit_mode)
            paste_x = monitor.x - min_x
            paste_y = monitor.y - min_y
            canvas.paste(img, (paste_x, paste_y))
            print(f"  [+] Monitor {monitor.index + 1} ({monitor.width}x{monitor.height}) <- {img_path.name}")
        except FileNotFoundError as e:
            print(f"  [AVISO] {e}")

    return canvas


def apply_random(cfg: dict, monitors: list[Monitor], output_dir: Path) -> Path:
    """Uma unica imagem espalhada por todos os monitores (modo random)."""
    _, _, total_w, total_h = get_virtual_desktop(monitors)
    fit_mode = cfg["display"]["fit_mode"]
    folder   = _resolve_folder(cfg, "random_folder")
    img_path = pick_random(folder)
    img      = Image.open(img_path).convert("RGB")
    img      = fit_image(img, total_w, total_h, fit_mode)
    print(f"  [+] Imagem: {img_path.name} -> canvas {total_w}x{total_h}")
    out = output_dir / "wallpaper_random.bmp"
    img.save(str(out), "BMP")
    set_wallpaper_win(out)
    return out


def apply_split(cfg: dict, monitors: list[Monitor], output_dir: Path, splits: int = 2) -> Path:
    """
    Atribui uma imagem diferente a cada monitor (split2 = 2 monitores,
    split4 = ate 4 monitores).

    Usa as posicoes reais dos monitores detectados. Se o numero de monitores
    fisicos for menor que `splits`, apenas os monitores disponiveis recebem
    imagem; os demais ficam com fundo preto no canvas final.
    """
    if not monitors:
        raise ValueError("Nenhum monitor detectado para aplicar o wallpaper.")

    fit_mode     = cfg["display"]["fit_mode"]
    num_sections = min(splits, len(monitors))

    sections: list[tuple[Monitor, str]] = [
        (monitors[i], _resolve_folder(cfg, MONITOR_PATH_KEYS[i]))
        for i in range(num_sections)
    ]

    canvas = build_wallpaper_canvas(monitors, sections, fit_mode)
    out    = output_dir / f"wallpaper_split{splits}.bmp"
    canvas.save(str(out), "BMP")
    set_wallpaper_win(out)
    return out
