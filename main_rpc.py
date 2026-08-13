"""Entry point para o executavel PyInstaller do sidecar RPC.

Espelha main.py: PyInstaller precisa de um script de nivel superior que importe o
pacote, porque apontar a spec direto para ``src/wallpaper_changer/rpc.py`` o executa
como ``__main__`` sem pacote pai e quebra os imports relativos.
"""
import sys

from wallpaper_changer.rpc import main

if __name__ == "__main__":
    # Used by the build pipeline to verify that all startup imports were bundled.
    if "--self-test" in sys.argv:
        sys.exit(0)
    sys.exit(main())
