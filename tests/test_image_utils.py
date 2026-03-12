import json
from pathlib import Path
import pytest
from unittest.mock import patch, MagicMock

# Create a mock for PIL before importing the module that uses it
import sys
mock_pil = MagicMock()
sys.modules["PIL"] = mock_pil
sys.modules["PIL.Image"] = mock_pil.Image

from wallpaper_changer.image_utils import _load_state, _save_state

def test_load_state_file_not_exists(tmp_path):
    state_file = tmp_path / "non_existent.json"
    assert _load_state(state_file) == {}

def test_load_state_valid_json(tmp_path):
    state_file = tmp_path / "valid_state.json"
    data = {"key": "value"}
    state_file.write_text(json.dumps(data), encoding="utf-8")
    assert _load_state(state_file) == data

def test_load_state_malformed_json(tmp_path):
    state_file = tmp_path / "malformed_state.json"
    state_file.write_text("{malformed: json}", encoding="utf-8")
    # _load_state should catch the exception and return {}
    assert _load_state(state_file) == {}

def test_save_state(tmp_path):
    state_file = tmp_path / "subdir" / "state.json"
    data = {"test": 123}
    _save_state(state_file, data)

    assert state_file.exists()
    loaded_data = json.loads(state_file.read_text(encoding="utf-8"))
    assert loaded_data == data
