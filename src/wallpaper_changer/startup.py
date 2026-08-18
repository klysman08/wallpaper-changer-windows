"""Gerenciamento de inicializacao automatica com o Windows."""
from __future__ import annotations

import sys
import winreg
from pathlib import Path

_APP_NAME = "WallpaperChanger"
_RUN_KEY = r"Software\Microsoft\Windows\CurrentVersion\Run"
_STARTUP_FLAG = "--startup"


def _get_exe_path() -> str:
    """Retorna o caminho do executavel atual (com flag --startup).

    Only meaningful in a frozen build. The desktop app registers autostart through
    ``tauri-plugin-autostart`` instead, which points at the Tauri executable; this
    module would register ``sys.executable``, i.e. the headless engine, which comes
    up with no window. In a source checkout there is no longer anything sensible to
    name — the ttkbootstrap GUI that used to be the target is gone — so rather than
    write a registry entry that launches nothing, say so.
    """
    if getattr(sys, "frozen", False):
        exe = str(Path(sys.executable).resolve())
        return f'"{exe}" {_STARTUP_FLAG}'
    raise RuntimeError(
        "Autostart cannot be registered from a source checkout; "
        "the desktop app owns it via tauri-plugin-autostart."
    )


def is_startup_launch() -> bool:
    """Return True if the app was launched via Windows startup (--startup flag)."""
    return _STARTUP_FLAG in sys.argv


def is_startup_enabled() -> bool:
    """Verifica se o app esta configurado para iniciar com o Windows."""
    try:
        key = winreg.OpenKey(winreg.HKEY_CURRENT_USER, _RUN_KEY, 0, winreg.KEY_READ)
        val, _ = winreg.QueryValueEx(key, _APP_NAME)
        winreg.CloseKey(key)
        return bool(val)
    except FileNotFoundError:
        return False
    except OSError:
        return False


def set_startup_enabled(enabled: bool) -> None:
    """Ativa ou desativa a inicializacao automatica com o Windows."""
    key = winreg.OpenKey(
        winreg.HKEY_CURRENT_USER, _RUN_KEY, 0, winreg.KEY_SET_VALUE
    )
    if enabled:
        exe_path = _get_exe_path()
        winreg.SetValueEx(key, _APP_NAME, 0, winreg.REG_SZ, exe_path)
    else:
        try:
            winreg.DeleteValue(key, _APP_NAME)
        except FileNotFoundError:
            pass
    winreg.CloseKey(key)
