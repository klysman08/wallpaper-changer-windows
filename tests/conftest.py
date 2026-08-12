"""Shared test fixtures.

The engine reads and writes real user files (``%APPDATA%\\WallpaperChanger``), and a
test that forgets to redirect them would quietly rewrite the developer's own
settings — or migrate an installation mid-run. Isolation is autouse rather than
opt-in so that a new test cannot leak by omission.
"""

import pytest


@pytest.fixture(autouse=True)
def isolate_user_directories(tmp_path_factory, monkeypatch):
    """Point every test's config/data directories at throwaway locations.

    Tests that need to assert on these paths can still read them back through
    ``config.get_user_config_dir()`` / ``get_user_data_dir()``, or override the same
    environment variables themselves — a later monkeypatch wins.
    """
    base = tmp_path_factory.mktemp("user-dirs")
    monkeypatch.setenv("WALLPAPER_CHANGER_CONFIG_DIR", str(base / "roaming"))
    monkeypatch.setenv("WALLPAPER_CHANGER_DATA_DIR", str(base / "local"))
