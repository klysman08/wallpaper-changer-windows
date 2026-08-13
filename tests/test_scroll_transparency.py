"""Modifier+wheel transparency: the pure logic, without installing a real hook."""

from __future__ import annotations

from unittest.mock import patch

import pytest

from wallpaper_changer import scroll_transparency as st


@pytest.mark.parametrize(
    ("raw", "expected"),
    [
        ("alt", "alt"),
        ("ALT", "alt"),
        ("  Ctrl  ", "ctrl"),
        ("control", "ctrl"),
        ("shift", "shift"),
        ("win", "win"),
        ("windows", "win"),
        ("super", "win"),
        ("meta", "win"),
    ],
)
def test_normalize_modifier_accepts_the_spellings_users_have(raw, expected):
    assert st.normalize_modifier(raw) == expected


@pytest.mark.parametrize("raw", ["", None, "nonsense", "ctrl+alt"])
def test_normalize_modifier_falls_back_rather_than_raising(raw):
    # A bad value in settings.toml must not stop the engine from starting.
    assert st.normalize_modifier(raw) == st.DEFAULT_MODIFIER


def test_every_supported_modifier_normalizes_to_itself():
    for name in st.SUPPORTED_MODIFIERS:
        assert st.normalize_modifier(name) == name


def test_scrolling_up_makes_the_window_more_opaque():
    assert st.next_alpha(200, 1) == 200 + st.STEP


def test_scrolling_down_makes_the_window_more_transparent():
    assert st.next_alpha(200, -1) == 200 - st.STEP


def test_alpha_is_clamped_to_the_usable_range():
    assert st.next_alpha(st.MAX_ALPHA, 5) == st.MAX_ALPHA
    assert st.next_alpha(st.MIN_ALPHA, -5) == st.MIN_ALPHA


def test_a_window_can_never_be_scrolled_past_invisible():
    # The floor matches the slider's minimum, so anything reached by scrolling
    # can still be dragged back by hand.
    assert st.next_alpha(0, -100) == st.MIN_ALPHA
    assert st.MIN_ALPHA > 0


def test_modifier_is_down_checks_the_right_virtual_keys():
    seen: list[int] = []

    class FakeUser32:
        # Must stay an instance method: it stands in for a ctypes function
        # pointer reached through windll.user32.
        def GetAsyncKeyState(self, vk):  # noqa: N802, PLR6301
            seen.append(vk)
            return 0

    with patch.object(st.ctypes, "windll") as windll:
        windll.user32 = FakeUser32()
        assert st.modifier_is_down("ctrl") is False
        assert seen == [0x11]

        seen.clear()
        # Windows has separate left and right keys; either one counts.
        assert st.modifier_is_down("win") is False
        assert seen == [0x5B, 0x5C]


def test_modifier_is_down_is_true_when_the_high_bit_is_set():
    class FakeUser32:
        def GetAsyncKeyState(self, vk):  # noqa: N802, PLR6301
            return -32768  # high bit set, as Win32 reports a held key

    with patch.object(st.ctypes, "windll") as windll:
        windll.user32 = FakeUser32()
        assert st.modifier_is_down("alt") is True


def test_status_reports_what_is_installed_not_what_was_asked_for():
    listener = st.ScrollTransparency()
    status = listener.status()
    assert status["running"] is False
    assert status["modifier"] == st.DEFAULT_MODIFIER
    assert set(status) == {"running", "modifier", "available"}


def test_stop_is_safe_when_nothing_was_started():
    st.ScrollTransparency().stop()


def test_scroll_is_ignored_while_the_modifier_is_up():
    listener = st.ScrollTransparency()
    with patch.object(st, "modifier_is_down", return_value=False), \
         patch.object(st.transparency, "get_foreground_window") as fg:
        listener._on_scroll(0, 0, 0, 1)
    fg.assert_not_called()


def test_scroll_applies_and_records_the_new_alpha():
    changes: list[dict] = []
    listener = st.ScrollTransparency(on_change=changes.append)

    with patch.object(st, "modifier_is_down", return_value=True), \
         patch.object(st.transparency, "get_foreground_window", return_value=42), \
         patch.object(
             st.transparency, "_get_process_name_for_hwnd", return_value="app.exe"
         ), \
         patch.object(st.transparency, "set_window_opacity") as set_opacity, \
         patch.object(listener, "_schedule_save"):
        listener._on_scroll(0, 0, 0, -1)

    expected = st.MAX_ALPHA - st.STEP
    set_opacity.assert_called_once_with(42, expected)
    assert changes == [{"hwnd": 42, "process": "app.exe", "alpha": expected}]


def test_the_process_name_is_resolved_once_per_window():
    listener = st.ScrollTransparency()
    with patch.object(st, "modifier_is_down", return_value=True), \
         patch.object(st.transparency, "get_foreground_window", return_value=7), \
         patch.object(
             st.transparency, "_get_process_name_for_hwnd", return_value="app.exe"
         ) as lookup, \
         patch.object(st.transparency, "set_window_opacity"), \
         patch.object(listener, "_schedule_save"):
        for _ in range(5):
            listener._on_scroll(0, 0, 0, -1)

    # Resolving it opens a process handle, and a fast scroll fires dozens of
    # events a second.
    assert lookup.call_count == 1


def test_a_failing_handler_never_escapes_onto_the_hook_thread():
    # An exception here would kill the listener and silently disable the feature.
    listener = st.ScrollTransparency()
    with patch.object(st, "modifier_is_down", side_effect=RuntimeError("boom")):
        listener._on_scroll(0, 0, 0, 1)
