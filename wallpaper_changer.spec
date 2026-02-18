# -*- mode: python ; coding: utf-8 -*-
# PyInstaller spec para WallpaperChanger GUI
# Executar a partir da raiz do projeto:
#   uv run pyinstaller wallpaper_changer.spec --noconfirm

from pathlib import Path
import customtkinter

# ── Caminhos de dados a serem empacotados ─────────────────────────────────────
HERE      = Path(SPECPATH)              # raiz do projeto
SRC       = HERE / "src" / "wallpaper_changer"
CTK_PATH  = Path(customtkinter.__file__).parent   # assets do CustomTkinter

datas = [
    # Temas e icones do CustomTkinter (obrigatorio)
    (str(CTK_PATH), "customtkinter"),
    # Configuracao padrao empacotada junto
    (str(HERE / "config" / "settings.toml"), "config"),
]

# ── Imports que o PyInstaller nao detecta automaticamente ─────────────────────
hidden = [
    "customtkinter",
    "darkdetect",
    "PIL._tkinter_finder",
    "screeninfo",
    "screeninfo.enumerators",
    "screeninfo.enumerators.windows",
    "schedule",
    "click",
    "win32api",
    "win32con",
    "win32gui",
    "pywintypes",
    "tomllib",
    "ctypes.wintypes",
]

# ── Analise de codigo ─────────────────────────────────────────────────────────
a = Analysis(
    [str(HERE / "main.py")],
    pathex=[str(HERE / "src")],
    binaries=[],
    datas=datas,
    hiddenimports=hidden,
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=["tkinter.test", "unittest", "email", "html", "http",
              "xml", "xmlrpc", "logging.handlers", "distutils"],
    noarchive=False,
    optimize=1,
)

# ── Bundle ────────────────────────────────────────────────────────────────────
pyz = PYZ(a.pure)

exe = EXE(
    pyz,
    a.scripts,
    [],
    exclude_binaries=True,
    name="WallpaperChanger",
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=True,
    console=False,          # sem janela de terminal
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
)

coll = COLLECT(
    exe,
    a.binaries,
    a.datas,
    strip=False,
    upx=True,
    upx_exclude=[],
    name="WallpaperChanger",
)
