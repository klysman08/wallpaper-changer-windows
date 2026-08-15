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


# ── plan_collage ──────────────────────────────────────────────────────────────
#
# The preview hands these rectangles to the UI as mouse targets, so they have to
# describe the picture that compose_collage actually draws.

def test_plan_collage_covers_each_monitor_without_overlapping():
    monitors = [Monitor(0, 0, 0, 400, 200), Monitor(1, 400, 0, 400, 200)]

    cells = wallpaper.plan_collage(monitors, count=4, same_for_all=False)

    assert len(cells) == 8
    assert sorted(c["image_index"] for c in cells) == list(range(8))
    covered = sum(c["width"] * c["height"] for c in cells)
    assert covered == 400 * 200 * 2


def test_plan_collage_offsets_cells_by_the_monitors_place_on_the_desktop():
    """Coordinates are composite-relative, so a screen left of the origin still
    lands at a non-negative offset."""
    monitors = [Monitor(0, -1920, 0, 1920, 1080), Monitor(1, 0, 0, 1920, 1080)]

    cells = wallpaper.plan_collage(monitors, count=1, same_for_all=False)

    assert [(c["x"], c["y"]) for c in cells] == [(0, 0), (1920, 0)]


def test_plan_collage_repeats_the_short_list_when_sharing_images():
    monitors = [Monitor(0, 0, 0, 400, 200), Monitor(1, 400, 0, 400, 200)]

    cells = wallpaper.plan_collage(monitors, count=2, same_for_all=True)

    assert [c["image_index"] for c in cells] == [0, 1, 0, 1]


def test_compose_collage_draws_every_cell_the_plan_describes(tmp_path):
    """The two must not drift: a plan the composition ignores would put the UI's
    hit targets over the wrong pictures."""
    folder = tmp_path / "pics"
    folder.mkdir()
    colours = [(255, 0, 0), (0, 255, 0), (0, 0, 255), (255, 255, 0)]
    for i, colour in enumerate(colours):
        Image.new("RGB", (40, 40), colour).save(folder / f"{i}.png")

    monitors = [Monitor(0, 0, 0, 400, 200)]
    cfg = {
        "paths": {"wallpapers_folder": str(folder)},
        "display": {"fit_mode": "stretch", "effect": "normal"},
        "general": {"collage_count": 4, "collage_same_for_all": False},
    }
    preset = [str(folder / f"{i}.png") for i in range(4)]

    canvas, used = wallpaper.compose_collage(
        cfg, monitors, preset_images=preset, state_file=tmp_path / "state.json"
    )

    assert used == preset
    for cell in wallpaper.plan_collage(monitors, 4, same_for_all=False):
        centre = (cell["x"] + cell["width"] // 2, cell["y"] + cell["height"] // 2)
        assert canvas.getpixel(centre) == colours[cell["image_index"]]


def test_compose_collage_repeats_a_short_selection_rather_than_failing(tmp_path):
    """The preview lets the user edit the selection, so a list shorter than the
    grid is reachable — and a wallpaper with a repeat beats no wallpaper."""
    folder = tmp_path / "pics"
    folder.mkdir()
    Image.new("RGB", (40, 40), (10, 20, 30)).save(folder / "only.png")

    cfg = {
        "paths": {"wallpapers_folder": str(folder)},
        "display": {"fit_mode": "stretch", "effect": "normal"},
        "general": {"collage_count": 4, "collage_same_for_all": False},
    }

    canvas, _ = wallpaper.compose_collage(
        cfg,
        [Monitor(0, 0, 0, 400, 200)],
        preset_images=[str(folder / "only.png")],
        state_file=tmp_path / "state.json",
    )

    assert canvas.getpixel((10, 10)) == (10, 20, 30)
    assert canvas.getpixel((390, 190)) == (10, 20, 30)


def test_crop_to_monitor_returns_that_screens_share_of_the_composite():
    """Two 100x50 screens side by side, painted in halves: the crop must pick one."""
    monitors = [Monitor(0, 0, 0, 100, 50), Monitor(1, 100, 0, 100, 50)]
    canvas = Image.new("RGB", (200, 50), (10, 10, 10))
    canvas.paste(Image.new("RGB", (100, 50), (250, 0, 0)), (100, 0))

    right = wallpaper.crop_to_monitor(canvas, monitors, 1)

    assert right.size == (100, 50)
    assert right.getpixel((50, 25)) == (250, 0, 0)


def test_crop_to_monitor_handles_a_screen_left_of_the_origin():
    """A secondary monitor can sit at a negative x; the composite starts at zero."""
    monitors = [Monitor(0, 0, 0, 100, 50), Monitor(1, -100, 0, 100, 50)]
    canvas = Image.new("RGB", (200, 50), (10, 10, 10))
    canvas.paste(Image.new("RGB", (100, 50), (0, 0, 250)), (0, 0))

    left = wallpaper.crop_to_monitor(canvas, monitors, 1)

    assert left.getpixel((50, 25)) == (0, 0, 250)


def test_crop_to_monitor_rejects_a_screen_that_is_not_there():
    canvas = Image.new("RGB", (10, 10))

    with pytest.raises(ValueError):
        wallpaper.crop_to_monitor(canvas, [Monitor(0, 0, 0, 10, 10)], 3)


def test_apply_desktop_image_spans_the_virtual_desktop_without_repeating(
    tmp_path, monkeypatch
):
    """A saved desktop-wide collage lands once across every screen, not per screen."""
    src = tmp_path / "saved.png"
    Image.new("RGB", (200, 50), (7, 7, 7)).save(src)
    monitors = [Monitor(0, 0, 0, 100, 50), Monitor(1, 100, 0, 100, 50)]
    captured: dict = {}

    def fake_transition(canvas, out, set_fn):
        captured["size"] = canvas.size
        captured["set_fn"] = set_fn
        canvas.save(str(out), "BMP")

    monkeypatch.setattr(wallpaper, "apply_transition", fake_transition)

    out = wallpaper.apply_desktop_image(src, monitors, tmp_path, fit_mode="stretch")

    assert captured["size"] == (200, 50)
    assert captured["set_fn"] is wallpaper.set_wallpaper_win
    assert out == tmp_path / "wallpaper_saved.bmp"
