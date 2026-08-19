# -*- mode: python ; coding: utf-8 -*-
# PyInstaller spec para o sidecar RPC consumido pelo app Tauri (desktop/).
# Executar a partir da raiz do projeto:
#   uv run pyinstaller wallpaper_changer_rpc.spec --noconfirm
#
# Diferencas em relacao a wallpaper_changer.spec (GUI):
#   - entry point e o loop RPC, nao a GUI ttkbootstrap
#   - console=True: o processo fala JSON por stdin/stdout e o Tauri o inicia com
#     CREATE_NO_WINDOW, entao nenhuma janela de terminal aparece
#   - saida vai direto para desktop/src-tauri/engine/, empacotada como recurso Tauri
#   - tkinter/ttkbootstrap ficam de fora: o sidecar nao tem interface

from pathlib import Path

HERE = Path(SPECPATH)
DIST_NAME = "wallpaper-changer-rpc"

datas = [
    (str(HERE / "config" / "settings.toml"), "config"),
]

# libmpv used to be bundled here, and at 112 MB it was most of this sidecar. The
# video wallpaper is native now and the Rust side loads the DLL itself, so it ships
# as a Tauri resource instead - see "libmpv" in tauri.conf.json.
binaries = []

hidden = [
    "logging.handlers",
    "screeninfo",
    "screeninfo.enumerators",
    "screeninfo.enumerators.windows",
    "win32api",
    "win32con",
    "win32gui",
    "pywintypes",
    "tomllib",
    "ctypes.wintypes",
]

a = Analysis(
    [str(HERE / "main_rpc.py")],
    pathex=[str(HERE / "src")],
    binaries=binaries,
    datas=datas,
    hiddenimports=hidden,
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=[
        # Sem GUI neste processo — o Tauri desenha a interface.
        "tkinter", "ttkbootstrap", "pystray", "click", "schedule",
        "unittest", "email", "html", "http", "xml", "xmlrpc", "distutils",
    ],
    noarchive=False,
    optimize=1,
)

pyz = PYZ(a.pure)

exe = EXE(
    pyz,
    a.scripts,
    [],
    exclude_binaries=True,
    name=DIST_NAME,
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=True,
    console=True,  # precisa de stdio; a janela e suprimida por CREATE_NO_WINDOW
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
    name=DIST_NAME,
)
