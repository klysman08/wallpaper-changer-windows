import importlib
import sys
import types

import pytest


@pytest.fixture
def transparency_module(monkeypatch):
    class DummyCallable:
        def __call__(self, *args, **kwargs):
            return 1

    class DummyUser32:
        def __init__(self):
            self.GetForegroundWindow = DummyCallable()
            self.SetWindowLongW = DummyCallable()
            self.GetWindowLongW = DummyCallable()
            self.SetLayeredWindowAttributes = DummyCallable()
            self.IsWindowVisible = DummyCallable()
            self.GetWindowTextW = DummyCallable()
            self.GetWindowTextLengthW = DummyCallable()
            self.EnumWindows = DummyCallable()

    class DummyDwmapi:
        def __init__(self):
            self.DwmGetWindowAttribute = DummyCallable()

    monkeypatch.setattr(
        "ctypes.windll",
        types.SimpleNamespace(user32=DummyUser32(), dwmapi=DummyDwmapi()),
    )
    monkeypatch.setattr("ctypes.WINFUNCTYPE", lambda *args, **kwargs: lambda fn: fn)
    monkeypatch.setattr("ctypes.wintypes.HWND", int)
    monkeypatch.setattr("ctypes.wintypes.LPARAM", int)
    monkeypatch.setattr("ctypes.wintypes.BOOL", int)
    monkeypatch.setattr("ctypes.wintypes.COLORREF", int)
    monkeypatch.setattr("ctypes.wintypes.BYTE", int)
    monkeypatch.setattr("ctypes.wintypes.DWORD", int)

    if "wallpaper_changer.transparency" in sys.modules:
        del sys.modules["wallpaper_changer.transparency"]

    module = importlib.import_module("wallpaper_changer.transparency")
    return module


def test_deve_ignorar_janela_sem_nome_processo_quando_listar_janelas_visiveis(
    transparency_module, monkeypatch
):
    monkeypatch.setattr(transparency_module, "IsWindowVisible", lambda hwnd: True)
    monkeypatch.setattr(transparency_module, "_is_cloaked", lambda hwnd: False)
    monkeypatch.setattr(transparency_module, "_get_window_title", lambda hwnd: "Secret title")
    monkeypatch.setattr(transparency_module, "_get_process_name_for_hwnd", lambda hwnd: "")
    monkeypatch.setattr(
        transparency_module,
        "EnumWindows",
        lambda callback, _lp: callback(100, 0),
    )

    result = transparency_module.list_visible_windows()

    assert result == []
