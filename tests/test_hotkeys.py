from __future__ import annotations

from collections.abc import Callable

import pytest

from wallpaper_changer import hotkeys


class FakeBackend:
    def __init__(self) -> None:
        self.bindings: dict[str, Callable[[], None]] = {}
        self.unregister_count = 0
        self.closed = False

    def replace(
        self, bindings: dict[str, Callable[[], None]]
    ) -> hotkeys.HotkeyRegistration:
        self.bindings = bindings
        return hotkeys.HotkeyRegistration(tuple(bindings), ())

    def unregister_all(self) -> None:
        self.unregister_count += 1
        self.bindings = {}

    def close(self) -> None:
        self.closed = True


@pytest.mark.parametrize(
    ("raw", "expected"),
    [
        (" Alt + Control + Right ", "ctrl+alt+right"),
        ("win+shift+v", "shift+windows+v"),
        ("CTRL+ALT+.", "ctrl+alt+."),
        ("control+page down", "ctrl+pagedown"),
    ],
)
def test_normalize_hotkey(raw, expected):
    assert hotkeys.normalize_hotkey(raw) == expected


@pytest.mark.parametrize(
    "raw",
    ["", "ctrl+alt", "ctrl+ctrl+x", "ctrl+x+y", "ctrl+not-a-key"],
)
def test_normalize_hotkey_rejects_invalid_combinations(raw):
    with pytest.raises(ValueError, match="shortcut"):
        hotkeys.normalize_hotkey(raw)


def test_hotkey_is_converted_to_native_flags_and_virtual_key():
    modifiers, vk = hotkeys._to_windows_hotkey("ctrl+alt+right")

    assert modifiers == (
        hotkeys._MOD_CONTROL | hotkeys._MOD_ALT | hotkeys._MOD_NOREPEAT
    )
    assert vk == hotkeys._KEY_TO_VK["right"]


def test_manager_normalizes_and_reports_duplicate_shortcuts():
    backend = FakeBackend()
    manager = hotkeys.HotkeyManager(backend=backend)

    result = manager.update(
        [("alt+ctrl+x", lambda: None), ("ctrl+alt+x", lambda: None)]
    )

    assert result.registered == ("ctrl+alt+x",)
    assert "assigned to more than one action" in result.errors[0]
    assert tuple(backend.bindings) == ("ctrl+alt+x",)

    manager.unregister_all()
    assert backend.unregister_count == 1

    manager.close()
    assert backend.closed is True
