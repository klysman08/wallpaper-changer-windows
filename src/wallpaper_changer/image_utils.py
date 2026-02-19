"""Utilitarios de composicao, selecao e redimensionamento de imagens."""
from __future__ import annotations
import json
import random
from pathlib import Path
from PIL import Image

SUPPORTED = {".jpg", ".jpeg", ".png", ".bmp", ".webp"}

# ── Listagem ──────────────────────────────────────────────────────────────────

def list_images(folder: str | Path) -> list[Path]:
    """Retorna todas as imagens suportadas na pasta (sem sub-pastas)."""
    folder = Path(folder)
    if not folder.exists():
        return []
    return [p for p in folder.iterdir() if p.suffix.lower() in SUPPORTED]


def list_images_sorted_by_date(folder: str | Path) -> list[Path]:
    """Retorna imagens ordenadas da mais recente para a mais antiga (mtime)."""
    images = list_images(folder)
    return sorted(images, key=lambda p: p.stat().st_mtime, reverse=True)


# ── Selecao ───────────────────────────────────────────────────────────────────

def _load_state(state_file: Path) -> dict:
    if state_file.exists():
        try:
            return json.loads(state_file.read_text(encoding="utf-8"))
        except Exception:
            pass
    return {}


def _save_state(state_file: Path, state: dict) -> None:
    state_file.parent.mkdir(parents=True, exist_ok=True)
    state_file.write_text(json.dumps(state, indent=2), encoding="utf-8")


def pick_images(
    folder: str | Path,
    count: int,
    method: str = "random",
    state_file: Path | None = None,
) -> list[Path]:
    """
    Seleciona `count` imagens da pasta.

    method:
      random     - imagens aleatorias (sem repeticao dentro do lote)
      sequential - percorre as imagens do mais recente ao mais antigo,
                   retomando de onde parou (estado salvo em state_file)
    """
    if method == "sequential":
        images = list_images_sorted_by_date(folder)
        if not images:
            raise FileNotFoundError(f"Nenhuma imagem em: {folder}")
        sf = state_file or (Path(folder).parent.parent / "config" / "state.json")
        state = _load_state(sf)
        folder_key = str(Path(folder).resolve())
        idx = state.get(folder_key, 0)
        # seleciona `count` imagens a partir de idx, com wrap-around
        result: list[Path] = []
        for i in range(count):
            result.append(images[(idx + i) % len(images)])
        state[folder_key] = (idx + count) % len(images)
        _save_state(sf, state)
        return result
    else:  # random — sem repeticao entre ciclos, persistente
        images = list_images(folder)
        if not images:
            raise FileNotFoundError(f"Nenhuma imagem em: {folder}")

        sf = state_file or (Path(folder).parent.parent / "config" / "state.json")
        state = _load_state(sf)
        folder_key = str(Path(folder).resolve())
        history_key = folder_key + ":random_history"

        # Historico de nomes de arquivo ja exibidos neste ciclo
        shown: list[str] = state.get(history_key, [])
        shown_set = set(shown)

        # Imagens ainda nao exibidas neste ciclo
        available = [p for p in images if p.name not in shown_set]

        # Se nao ha suficientes, reinicia o ciclo
        if len(available) < count:
            shown = []
            shown_set = set()
            available = list(images)

        if count >= len(available):
            picked = list(available)
            # Completa com aleatorias se necessario
            while len(picked) < count:
                picked.append(random.choice(images))
        else:
            picked = random.sample(available, count)

        # Atualiza historico
        shown.extend(p.name for p in picked)
        state[history_key] = shown
        _save_state(sf, state)
        return picked


# ── Compat: pick_random (mantido para nao quebrar imports antigos) ────────────

def pick_random(folder: str | Path) -> Path:
    return pick_images(folder, 1, "random")[0]


# ── Redimensionamento ─────────────────────────────────────────────────────────

def fit_image(img: Image.Image, target_w: int, target_h: int, mode: str = "fill") -> Image.Image:
    """
    Modos suportados:
      fill    (Preencher)    – escala para preencher, corta o excesso
      fit     (Ajustar)      – escala sem cortar, adiciona barras pretas
      stretch (Ampliar)      – distorce para preencher exatamente
      center  (Centralizar)  – sem escala, centraliza em fundo preto
      span    (Estender)     – alias de fill (imagem se expande para cobrir)
    """
    if mode == "span":
        mode = "fill"

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
    ratio   = max(ratio_w, ratio_h) if mode == "fill" else min(ratio_w, ratio_h)
    new_w   = int(src_w * ratio)
    new_h   = int(src_h * ratio)
    img     = img.resize((new_w, new_h), Image.LANCZOS)

    if mode == "fill":
        left = (new_w - target_w) // 2
        top  = (new_h - target_h) // 2
        return img.crop((left, top, left + target_w, top + target_h))
    else:  # fit – centraliza com fundo preto
        canvas = Image.new("RGB", (target_w, target_h), (0, 0, 0))
        offset_x = (target_w - new_w) // 2
        offset_y = (target_h - new_h) // 2
        canvas.paste(img, (offset_x, offset_y))
        return canvas


def build_canvas(total_w: int, total_h: int) -> Image.Image:
    return Image.new("RGB", (total_w, total_h), (0, 0, 0))
