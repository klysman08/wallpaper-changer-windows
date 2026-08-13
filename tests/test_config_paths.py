"""Tests for user-scoped config/data locations and the one-time migration.

Config used to live inside the installation directory, which breaks as soon as the
app is installed under ``C:\\Program Files`` — an unprivileged process cannot write
there. These cover the move to %APPDATA% / %LOCALAPPDATA% and the migration that
carries an existing installation's files across.
"""

import pytest

from wallpaper_changer import config


@pytest.fixture
def user_dirs(tmp_path, monkeypatch):
    """Point the config and data directories at throwaway locations."""
    cfg_dir = tmp_path / "roaming"
    data_dir = tmp_path / "local"
    monkeypatch.setenv("WALLPAPER_CHANGER_CONFIG_DIR", str(cfg_dir))
    monkeypatch.setenv("WALLPAPER_CHANGER_DATA_DIR", str(data_dir))
    return cfg_dir, data_dir


# ── Location ──────────────────────────────────────────────────────────────────

def test_config_dir_follows_appdata(monkeypatch, tmp_path):
    monkeypatch.delenv("WALLPAPER_CHANGER_CONFIG_DIR", raising=False)
    monkeypatch.setenv("APPDATA", str(tmp_path))

    assert config.get_user_config_dir() == tmp_path / "WallpaperChanger"


def test_data_dir_follows_localappdata(monkeypatch, tmp_path):
    monkeypatch.delenv("WALLPAPER_CHANGER_DATA_DIR", raising=False)
    monkeypatch.setenv("LOCALAPPDATA", str(tmp_path))

    assert config.get_user_data_dir() == tmp_path / "WallpaperChanger"


def test_config_and_data_dirs_are_separate(user_dirs):
    """Large per-machine output must not travel in a roaming profile."""
    assert config.get_user_config_dir() != config.get_user_data_dir()


def test_user_files_live_under_the_config_dir(user_dirs):
    cfg_dir, _ = user_dirs

    assert config.get_default_config_path().parent == cfg_dir
    assert config.get_state_file().parent == cfg_dir
    assert config.get_transparency_file().parent == cfg_dir


def test_config_dir_is_never_the_install_dir(user_dirs):
    """The whole point of the move: nothing user-writable inside the install."""
    assert config.get_user_config_dir() != config.get_project_root()
    assert config.get_state_file().parent != config.get_project_root() / "config"


# ── Output directory ──────────────────────────────────────────────────────────

def test_relative_output_folder_resolves_under_the_data_dir(user_dirs):
    _, data_dir = user_dirs

    assert config.resolve_output_dir({"paths": {"output_folder": "output"}}) == data_dir / "output"


def test_absolute_output_folder_is_respected(user_dirs, tmp_path):
    explicit = tmp_path / "somewhere" / "else"

    result = config.resolve_output_dir({"paths": {"output_folder": str(explicit)}})

    assert result == explicit


def test_missing_output_folder_falls_back_to_a_default(user_dirs):
    _, data_dir = user_dirs

    assert config.resolve_output_dir({}) == data_dir / "output"
    assert config.resolve_output_dir({"paths": {"output_folder": ""}}) == data_dir / "output"


def test_legacy_relative_output_folder_no_longer_targets_the_install_dir(user_dirs):
    """An existing config saying "assets/output" must not write into the install."""
    result = config.resolve_output_dir({"paths": {"output_folder": "assets/output"}})

    assert config.get_project_root() not in result.parents


# ── Migration ─────────────────────────────────────────────────────────────────

@pytest.fixture
def legacy_install(tmp_path, monkeypatch):
    """A pre-migration installation with user files inside its own directory."""
    root = tmp_path / "install"
    (root / "config").mkdir(parents=True)
    (root / "config" / "settings.toml").write_text(
        '[general]\ncollage_count = 7\n', encoding="utf-8"
    )
    (root / "config" / "state.json").write_text('{"seen": 3}', encoding="utf-8")
    (root / "config" / "transparency.json").write_text('{"app.exe": 128}', encoding="utf-8")
    monkeypatch.setattr(config, "PROJECT_ROOT", root)
    return root


def test_migration_copies_existing_user_files(user_dirs, legacy_install):
    cfg_dir, _ = user_dirs

    config._migrate_legacy_files()

    assert (cfg_dir / "settings.toml").read_text(encoding="utf-8").strip().endswith("7")
    assert (cfg_dir / "state.json").exists()
    assert (cfg_dir / "transparency.json").exists()


def test_migration_leaves_the_original_in_place(user_dirs, legacy_install):
    """Migration must stay reversible — never move, only copy."""
    config._migrate_legacy_files()

    assert (legacy_install / "config" / "settings.toml").exists()


def test_migration_never_overwrites_newer_user_settings(user_dirs, legacy_install):
    cfg_dir, _ = user_dirs
    cfg_dir.mkdir(parents=True)
    (cfg_dir / "settings.toml").write_text("[general]\ncollage_count = 2\n", encoding="utf-8")

    config._migrate_legacy_files()

    assert "2" in (cfg_dir / "settings.toml").read_text(encoding="utf-8")


def test_migration_is_idempotent(user_dirs, legacy_install):
    cfg_dir, _ = user_dirs
    config._migrate_legacy_files()
    (cfg_dir / "settings.toml").write_text("[general]\ncollage_count = 5\n", encoding="utf-8")

    config._migrate_legacy_files()

    assert "5" in (cfg_dir / "settings.toml").read_text(encoding="utf-8")


def test_migration_is_a_noop_without_a_legacy_directory(user_dirs, tmp_path, monkeypatch):
    monkeypatch.setattr(config, "PROJECT_ROOT", tmp_path / "nonexistent")

    config._migrate_legacy_files()  # must not raise

    assert not config.get_user_config_dir().exists()


def test_load_config_migrates_then_reads(user_dirs, legacy_install):
    """First run after upgrading finds the settings that were in the install dir."""
    cfg = config.load_config()

    assert cfg["general"]["collage_count"] == 7
    assert cfg["_config_path"] == str(config.get_default_config_path())


def test_load_config_with_explicit_path_skips_migration(user_dirs, legacy_install, tmp_path):
    explicit = tmp_path / "custom.toml"
    explicit.write_text("[general]\ncollage_count = 1\n", encoding="utf-8")

    cfg = config.load_config(explicit)

    assert cfg["general"]["collage_count"] == 1
    assert not (config.get_user_config_dir() / "settings.toml").exists()


def test_save_config_writes_to_the_user_dir_by_default(user_dirs):
    cfg_dir, _ = user_dirs

    config.save_config({"general": {"collage_count": 4}})

    assert (cfg_dir / "settings.toml").exists()
