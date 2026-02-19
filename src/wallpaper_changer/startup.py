"""Gerenciamento de inicializacao automatica com o Windows."""
from __future__ import annotations

import sys
import winreg
from pathlib import Path

_APP_NAME = "WallpaperChanger"
_RUN_KEY = r"Software\Microsoft\Windows\CurrentVersion\Run"


def _get_exe_path() -> str:
    """Retorna o caminho do executavel atual."""
    if getattr(sys, "frozen", False):
        return str(Path(sys.executable).resolve())
    # Modo desenvolvimento: usa o interpretador Python + modulo
    return f'"{sys.executable}" -m wallpaper_changer.gui'


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
