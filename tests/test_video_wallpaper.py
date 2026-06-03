"""Tests for the video wallpaper engine and CLI command.

These never create a real WORKERW window, real host windows, or a real mpv/libmpv
instance — every Win32 and mpv touchpoint is mocked, mirroring the ctypes-mocking
convention used elsewhere in the suite.
"""
from __future__ import annotations

import sys
import types
from pathlib import Path
from unittest import mock

from click.testing import CliRunner

from wallpaper_changer import cli
from wallpaper_changer import video_wallpaper as vw
from wallpaper_changer.monitor import Monitor


# ── Fake mpv ──────────────────────────────────────────────────────────────────

class _FakeMPV:
    instances: list["_FakeMPV"] = []

    def __init__(self, **kwargs):
        self.kwargs = kwargs
        self.mute = kwargs.get("mute", True)
        self.playlist_pos = 0
        self.loaded: list[tuple[str, str]] = []
        self.terminated = False
        _FakeMPV.instances.append(self)

    def loadfile(self, path, mode="replace"):
        self.loaded.append((path, mode))

    def terminate(self):
        self.terminated = True


def _install_fake_mpv(monkeypatch):
    _FakeMPV.instances.clear()
    fake_mod = types.ModuleType("mpv")
    fake_mod.MPV = _FakeMPV
    monkeypatch.setitem(sys.modules, "mpv", fake_mod)


# ── scan_video_folder ─────────────────────────────────────────────────────────

def test_scan_video_folder_filters_and_sorts(tmp_path):
    (tmp_path / "b.mp4").write_bytes(b"x")
    (tmp_path / "a.MKV").write_bytes(b"x")          # case-insensitive extension
    (tmp_path / "note.txt").write_bytes(b"x")       # wrong extension
    (tmp_path / "sub").mkdir()                       # directories ignored

    result = [p.name for p in vw.scan_video_folder(tmp_path)]

    assert result == ["a.MKV", "b.mp4"]


def test_scan_video_folder_missing_dir_returns_empty(tmp_path):
    assert vw.scan_video_folder(tmp_path / "does-not-exist") == []


# ── has_mpv ───────────────────────────────────────────────────────────────────

def test_has_mpv_false_when_import_fails(monkeypatch):
    monkeypatch.setattr(vw, "_prepare_libmpv", lambda: None)
    # Setting the module to None makes `import mpv` raise ImportError.
    monkeypatch.setitem(sys.modules, "mpv", None)

    assert vw.has_mpv() is False


# ── Player lifecycle ──────────────────────────────────────────────────────────

def _patch_win32(monkeypatch):
    """Replace every Win32 touchpoint with harmless fakes."""
    monkeypatch.setattr(vw, "_prepare_libmpv", lambda: None)
    monkeypatch.setattr(vw, "get_desktop_parent", lambda: 4321)
    monkeypatch.setattr(vw, "_window_origin", lambda hwnd: (0, 0))
    monkeypatch.setattr(vw, "_create_host_window", lambda parent, mon, left, top: 1000 + mon.index)
    monkeypatch.setattr(vw, "_refresh_desktop", lambda parent: None)
    monkeypatch.setattr(vw, "user32", mock.MagicMock())


def test_player_not_running_before_start():
    assert vw.VideoWallpaperPlayer().is_running() is False


def test_player_start_creates_one_mpv_per_monitor(monkeypatch):
    _patch_win32(monkeypatch)
    _install_fake_mpv(monkeypatch)

    monitors = [Monitor(0, 0, 0, 1920, 1080), Monitor(1, 1920, 0, 1920, 1080)]
    player = vw.VideoWallpaperPlayer()
    player.configure(
        videos=[Path("a.mp4"), Path("b.mp4")],
        loop=True, sound=True, monitors=monitors,
    )

    player.start()

    assert player.is_running() is True
    assert len(_FakeMPV.instances) == 2
    # Only the first instance carries audio (avoids echo across monitors).
    assert _FakeMPV.instances[0].mute is False
    assert _FakeMPV.instances[1].mute is True
    # Playlist: first file replaces, the rest append.
    assert _FakeMPV.instances[0].loaded == [
        (str(Path("a.mp4")), "replace"),
        (str(Path("b.mp4")), "append"),
    ]
    # Looping the whole playlist forever.
    assert _FakeMPV.instances[0].kwargs["loop_playlist"] == "inf"


def test_player_stop_terminates_and_destroys(monkeypatch):
    _patch_win32(monkeypatch)
    _install_fake_mpv(monkeypatch)

    player = vw.VideoWallpaperPlayer()
    player.configure(
        videos=[Path("a.mp4")], loop=False, sound=False,
        monitors=[Monitor(0, 0, 0, 800, 600)],
    )
    player.start()
    assert player.is_running() is True
    assert _FakeMPV.instances[0].kwargs["loop_playlist"] == "no"

    player.stop()

    assert player.is_running() is False
    assert _FakeMPV.instances[0].terminated is True
    assert vw.user32.DestroyWindow.call_count == 1


def test_player_next_prev_navigation_wraps_and_syncs(monkeypatch):
    _patch_win32(monkeypatch)
    _install_fake_mpv(monkeypatch)

    monitors = [Monitor(0, 0, 0, 800, 600), Monitor(1, 800, 0, 800, 600)]
    player = vw.VideoWallpaperPlayer()
    player.configure(
        videos=[Path("a.mp4"), Path("b.mp4"), Path("c.mp4")],
        loop=True, sound=False, monitors=monitors,
    )
    player.start()

    assert player.next_video() == "b.mp4"
    # both monitors jump to the same index (re-synced)
    assert [inst.playlist_pos for inst in _FakeMPV.instances] == [1, 1]
    assert player.next_video() == "c.mp4"
    assert player.next_video() == "a.mp4"          # wraps forward past the end
    assert player.prev_video() == "c.mp4"          # wraps backward past the start
    assert [inst.playlist_pos for inst in _FakeMPV.instances] == [2, 2]


def test_player_next_prev_noop_when_stopped():
    player = vw.VideoWallpaperPlayer()
    player.configure(videos=[Path("a.mp4")], monitors=[Monitor(0, 0, 0, 800, 600)])
    # Not started — must not raise, just returns the first/empty name.
    assert player.next_video() == "a.mp4"
    assert player.prev_video() == "a.mp4"


def test_player_start_raises_without_desktop_layer(monkeypatch):
    _patch_win32(monkeypatch)
    _install_fake_mpv(monkeypatch)
    monkeypatch.setattr(vw, "get_desktop_parent", lambda: None)

    player = vw.VideoWallpaperPlayer()
    player.configure(videos=[Path("a.mp4")], monitors=[Monitor(0, 0, 0, 800, 600)])

    try:
        player.start()
    except RuntimeError:
        pass
    else:
        raise AssertionError("expected RuntimeError when no desktop layer is found")
    assert player.is_running() is False


# ── CLI ───────────────────────────────────────────────────────────────────────

def test_cli_video_reports_when_mpv_missing(monkeypatch):
    monkeypatch.setattr(cli, "has_mpv", lambda: False)
    runner = CliRunner()

    result = runner.invoke(cli.main, ["video", "--folder", "."])

    assert result.exit_code == 0
    assert "python-mpv" in result.output


def test_cli_video_reports_when_no_videos(monkeypatch):
    monkeypatch.setattr(cli, "has_mpv", lambda: True)
    monkeypatch.setattr(cli, "scan_video_folder", lambda folder: [])
    runner = CliRunner()

    result = runner.invoke(cli.main, ["video", "--folder", "C:/empty"])

    assert result.exit_code == 0
    assert "Nenhum video encontrado" in result.output


def test_cli_video_configures_starts_and_stops(monkeypatch):
    calls: dict = {}

    class FakePlayer:
        def configure(self, **kw):
            calls["configure"] = kw

        def start(self):
            calls["started"] = True

        def is_running(self):
            return False        # exit the keep-alive loop immediately

        def stop(self):
            calls["stopped"] = True

    monkeypatch.setattr(cli, "has_mpv", lambda: True)
    monkeypatch.setattr(cli, "scan_video_folder", lambda folder: [Path("x.mp4")])
    monkeypatch.setattr(cli, "get_monitors", lambda: [Monitor(0, 0, 0, 800, 600)])
    monkeypatch.setattr(cli, "VideoWallpaperPlayer", FakePlayer)
    runner = CliRunner()

    result = runner.invoke(
        cli.main, ["video", "--folder", "C:/videos", "--loop", "--sound"]
    )

    assert result.exit_code == 0
    assert calls.get("started") is True
    assert calls.get("stopped") is True
    assert calls["configure"]["loop"] is True
    assert calls["configure"]["sound"] is True
