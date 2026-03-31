from pathlib import Path

import pytest
from PIL import Image

from wallpaper_changer import wallpaper


def test_deve_aplicar_fade_sem_erro_quando_fade_esta_ativo(tmp_path, monkeypatch):
    old_wallpaper = tmp_path / "old.bmp"
    out_path = tmp_path / "new.bmp"
    Image.new("RGB", (8, 8), (255, 0, 0)).save(old_wallpaper, "BMP")
    canvas = Image.new("RGB", (8, 8), (0, 255, 0))

    fast_calls: list[Path] = []
    set_calls: list[Path] = []

    monkeypatch.setattr(wallpaper, "_FADE_FRAMES", 3)
    monkeypatch.setattr(wallpaper, "_FADE_DELAY", 0.0)
    monkeypatch.setattr(wallpaper, "_get_current_wallpaper", lambda: old_wallpaper)
    monkeypatch.setattr(wallpaper, "set_wallpaper_style_span", lambda: None)
    monkeypatch.setattr(wallpaper, "_set_wallpaper_fast", lambda path: fast_calls.append(Path(path)))
    monkeypatch.setattr(wallpaper, "set_wallpaper_win", lambda path: set_calls.append(Path(path)))

    wallpaper._apply_or_fade(canvas, out_path, fade_in=True)

    assert out_path.exists()
    assert len(fast_calls) == 2
    assert set_calls == [out_path]


@pytest.mark.parametrize("effect", ["normal", "bw", "vintage", "hdr"])
def test_deve_retornar_imagem_rgb_quando_aplicar_efeitos_suportados(effect):
    canvas = Image.new("RGB", (12, 12), (40, 80, 120))

    result = wallpaper.apply_effect(canvas, effect)

    assert result.mode == "RGB"
    assert result.size == canvas.size


def test_deve_lancar_erro_quando_efeito_invalido():
    canvas = Image.new("RGB", (12, 12), (40, 80, 120))

    with pytest.raises(ValueError, match="Efeito de imagem invalido"):
        wallpaper.apply_effect(canvas, "invalid")
