import pytest
from PIL import Image

from wallpaper_changer import wallpaper
from wallpaper_changer.monitor import Monitor


def test_apply_single_wallpaper_builds_canvas_and_delegates_to_transition(tmp_path, monkeypatch):
    """apply_single_wallpaper composes the virtual-desktop canvas and hands it to
    apply_transition — verified without touching any Win32 API."""
    src = tmp_path / "src.png"
    Image.new("RGB", (20, 20), (200, 100, 50)).save(src)
    monitors = [Monitor(0, 0, 0, 16, 16)]

    captured: dict = {}

    def fake_transition(canvas, out, set_fn):
        captured["size"] = canvas.size
        captured["set_fn"] = set_fn
        canvas.save(str(out), "BMP")   # emulate persistence

    monkeypatch.setattr(wallpaper, "apply_transition", fake_transition)

    out = wallpaper.apply_single_wallpaper(src, monitors, tmp_path, fit_mode="fill")

    assert out == tmp_path / "wallpaper_default.bmp"
    assert out.exists()
    assert captured["size"] == (16, 16)   # virtual desktop = the single 16x16 monitor
    # the setter handed to apply_transition must be the real Win32 applier, not None/other
    assert captured["set_fn"] is wallpaper.set_wallpaper_win


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
