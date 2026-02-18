"""Utilitarios de composicao e redimensionamento de imagens."""
from __future__ import annotations
import random
from pathlib import Path
from PIL import Image

SUPPORTED = {".jpg", ".jpeg", ".png", ".bmp", ".webp"}

def list_images(folder: str | Path) -> list[Path]:
    folder = Path(folder)
    if not folder.exists():
        return []
    return [p for p in folder.iterdir() if p.suffix.lower() in SUPPORTED]

def pick_random(folder: str | Path) -> Path:
    images = list_images(folder)
    if not images:
        raise FileNotFoundError(f"Nenhuma imagem encontrada em: {folder}")
    return random.choice(images)

def fit_image(img: Image.Image, target_w: int, target_h: int, mode: str = "fill") -> Image.Image:
    """
    Modos:
      fill    - preenche toda a area (corta o excesso)
      fit     - ajusta sem cortar (barras pretas nas bordas)
      stretch - estica sem manter proporcao
      center  - centraliza sem redimensionar
    """
    src_w, src_h = img.size
    if mode == "stretch":
        return img.resize((target_w, target_h), Image.LANCZOS)
    if mode == "center":
        canvas = Image.new("RGB", (target_w, target_h), (0, 0, 0))
        offset_x = (target_w - src_w) // 2
        offset_y = (target_h - src_h) // 2
        canvas.paste(img, (offset_x, offset_y))
        return canvas
    ratio_w = target_w / src_w
    ratio_h = target_h / src_h
    if mode == "fill":
        ratio = max(ratio_w, ratio_h)
    else:  # fit
        ratio = min(ratio_w, ratio_h)
    new_w = int(src_w * ratio)
    new_h = int(src_h * ratio)
    img = img.resize((new_w, new_h), Image.LANCZOS)
    if mode == "fill":
        left = (new_w - target_w) // 2
        top  = (new_h - target_h) // 2
        return img.crop((left, top, left + target_w, top + target_h))
    else:  # fit - centraliza com fundo preto
        canvas = Image.new("RGB", (target_w, target_h), (0, 0, 0))
        offset_x = (target_w - new_w) // 2
        offset_y = (target_h - new_h) // 2
        canvas.paste(img, (offset_x, offset_y))
        return canvas

def build_canvas(total_w: int, total_h: int) -> Image.Image:
    return Image.new("RGB", (total_w, total_h), (0, 0, 0))
