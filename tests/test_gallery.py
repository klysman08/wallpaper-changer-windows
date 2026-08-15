"""Tests for the saved-collage library index.

The index points at real files the user cares about, so the behaviour worth pinning
down is what it does when those files move, and that removing an entry never takes
the picture with it.
"""

from pathlib import Path

from PIL import Image

from wallpaper_changer import gallery


def _saved(tmp_path: Path, name: str) -> Path:
    path = tmp_path / name
    Image.new("RGB", (4, 4)).save(path)
    return path


def test_record_puts_the_newest_save_first(tmp_path):
    first = _saved(tmp_path, "one.png")
    second = _saved(tmp_path, "two.png")

    gallery.record(first, monitor=None, images=["a.jpg"], width=10, height=5)
    gallery.record(second, monitor=1, images=["b.jpg"], width=6, height=4)

    assert [Path(e["path"]).name for e in gallery.entries()] == ["two.png", "one.png"]


def test_record_replaces_the_entry_for_a_file_saved_over(tmp_path):
    target = _saved(tmp_path, "same.png")

    gallery.record(target, monitor=0, images=["old.jpg"], width=8, height=8)
    gallery.record(target, monitor=None, images=["new.jpg"], width=9, height=9)

    entries = gallery.entries()
    assert len(entries) == 1
    assert entries[0]["images"] == ["new.jpg"]


def test_record_treats_windows_path_spellings_as_one_file(tmp_path):
    """A path differing only in case names the same file, and must not be listed twice."""
    target = _saved(tmp_path, "Cased.png")

    gallery.record(target, monitor=None, images=[], width=1, height=1)
    gallery.record(str(target).upper(), monitor=None, images=[], width=1, height=1)

    assert len(gallery.entries()) == 1


def test_entries_drops_files_that_are_no_longer_there(tmp_path):
    kept = _saved(tmp_path, "kept.png")
    gone = _saved(tmp_path, "gone.png")
    gallery.record(kept, monitor=None, images=[], width=1, height=1)
    gallery.record(gone, monitor=None, images=[], width=1, height=1)

    gone.unlink()

    assert [Path(e["path"]).name for e in gallery.entries()] == ["kept.png"]
    # Pruned from the file too, not just from what this call returned.
    assert "gone.png" not in gallery.get_index_file().read_text(encoding="utf-8")


def test_forget_removes_the_entry_but_never_the_image(tmp_path):
    target = _saved(tmp_path, "keep-the-file.png")
    gallery.record(target, monitor=None, images=[], width=1, height=1)

    assert gallery.forget(target) is True
    assert gallery.entries() == []
    assert target.exists()


def test_forget_reports_when_there_was_nothing_to_remove(tmp_path):
    assert gallery.forget(tmp_path / "never-saved.png") is False


def test_find_returns_the_entry_describing_a_file(tmp_path):
    target = _saved(tmp_path, "one-screen.png")
    gallery.record(target, monitor=1, images=[], width=1, height=1)

    assert gallery.find(target)["monitor"] == 1
    assert gallery.find(tmp_path / "other.png") is None


def test_a_corrupt_index_reads_as_empty_rather_than_raising(tmp_path):
    index = gallery.get_index_file()
    index.parent.mkdir(parents=True, exist_ok=True)
    index.write_text("{not json", encoding="utf-8")

    assert gallery.entries() == []


def test_suggest_name_says_which_screen_it_holds():
    assert gallery.suggest_name(None).endswith("_all.png")
    # Monitors are numbered from one everywhere the user can see them.
    assert gallery.suggest_name(0).endswith("_monitor1.png")
    assert gallery.suggest_name(1, ".jpg").endswith("_monitor2.jpg")
