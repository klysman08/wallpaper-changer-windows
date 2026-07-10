"""Entry point para o executavel PyInstaller."""
import sys

from wallpaper_changer.gui import run

if __name__ == "__main__":
    # Used by the build pipeline to verify that all startup imports were bundled.
    if "--self-test" not in sys.argv:
        run()
