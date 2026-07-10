from wallpaper_changer.config import load_config, save_config


def test_save_config_round_trips_and_leaves_no_temporary_file(tmp_path):
    target = tmp_path / "settings.toml"
    config = {
        "general": {"interval": 60, "enabled": True},
        "paths": {"wallpapers_folder": 'C:\\Pictures\\"Best"'},
    }

    save_config(config, target)
    loaded = load_config(target)

    assert loaded["general"] == {"interval": 60, "enabled": True}
    assert loaded["paths"]["wallpapers_folder"] == 'C:\\Pictures\\"Best"'
    assert list(tmp_path.glob("*.tmp")) == []
