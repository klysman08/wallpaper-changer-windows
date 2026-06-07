"""Tests for the wallpaper transition entry point.

The transition is now the operating system's native fade, so ``apply_transition`` only
composes/persists the image and hands it to ``set_wallpaper_fn`` (which, in production,
calls SystemParametersInfo). These tests use a plain callback in place of that Win32
call — no real wallpaper is applied.
"""
from __future__ import annotations

from PIL import Image

from wallpaper_changer import transition as tr


def _canvas(size=(8, 8)):
    return Image.new("RGB", size, (10, 20, 30))


def test_apply_transition_persists_and_applies(tmp_path):
    out = tmp_path / "wp.bmp"
    calls = []

    tr.apply_transition(_canvas(), out, lambda p: calls.append(p))

    assert out.exists()                 # composed image was written
    assert calls == [out]               # and handed to the wallpaper setter exactly once


def test_apply_transition_writes_a_valid_bmp(tmp_path):
    out = tmp_path / "wp.bmp"

    tr.apply_transition(_canvas((16, 9)), out, lambda p: None)

    with Image.open(out) as im:
        assert im.format == "BMP"
        assert im.size == (16, 9)


def test_apply_transition_applies_the_persisted_path(tmp_path):
    """The path handed to the setter is the same one that was written, already on disk."""
    out = tmp_path / "wp.bmp"
    applied = {}

    def fake_set(path):
        applied["path"] = path
        applied["exists_at_apply"] = path.exists()

    tr.apply_transition(_canvas(), out, fake_set)

    assert applied["path"] == out
    assert applied["exists_at_apply"] is True   # written before it is applied
