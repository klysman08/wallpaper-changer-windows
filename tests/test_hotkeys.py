from __future__ import annotations

from types import SimpleNamespace

import pytest

from wallpaper_changer import hotkeys


@pytest.mark.parametrize(
    ("raw", "expected"),
    [
        (" Alt + Control + Right ", "ctrl+alt+right"),
        ("win+shift+v", "shift+windows+v"),
        ("CTRL+ALT+.", "ctrl+alt+."),
    ],
)
def test_normalize_hotkey(raw, expected):
    assert hotkeys.normalize_hotkey(raw) == expected


@pytest.mark.parametrize("raw", ["", "ctrl+alt", "ctrl+ctrl+x"])
def test_normalize_hotkey_rejects_invalid_combinations(raw):
    with pytest.raises(ValueError, match="shortcut"):
        hotkeys.normalize_hotkey(raw)


def test_manager_uses_handles_and_reports_duplicate_shortcuts(monkeypatch):
    added = []
    removed = []

    def add_hotkey(combo, callback, **options):
        handle = object()
        added.append((combo, callback, options, handle))
        return handle

    def remove_hotkey(handle):
        removed.append(handle)

    fake_keyboard = SimpleNamespace(
        add_hotkey=add_hotkey,
        remove_hotkey=remove_hotkey,
    )

    monkeypatch.setattr(hotkeys, "_AVAILABLE", True)
    monkeypatch.setattr(hotkeys, "_kb", fake_keyboard)
    manager = hotkeys.HotkeyManager()

    result = manager.update(
        [("alt+ctrl+x", lambda: None), ("ctrl+alt+x", lambda: None)]
    )

    assert result.registered == ("ctrl+alt+x",)
    assert "assigned to more than one action" in result.errors[0]
    assert added[0][2]["trigger_on_release"] is True

    manager.unregister_all()
    assert removed == [added[0][3]]
