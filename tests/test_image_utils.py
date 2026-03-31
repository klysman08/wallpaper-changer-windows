import json

from wallpaper_changer.image_utils import _load_state, _save_state


def test_deve_retornar_dict_vazio_quando_arquivo_de_estado_nao_existe(tmp_path):
    state_file = tmp_path / "non_existent.json"

    result = _load_state(state_file)

    assert result == {}


def test_deve_retornar_estado_quando_json_valido(tmp_path):
    state_file = tmp_path / "valid_state.json"
    expected = {"key": "value"}
    state_file.write_text(json.dumps(expected), encoding="utf-8")

    result = _load_state(state_file)

    assert result == expected


def test_deve_retornar_dict_vazio_quando_json_esta_malformado(tmp_path):
    state_file = tmp_path / "malformed_state.json"
    state_file.write_text("{malformed: json}", encoding="utf-8")

    result = _load_state(state_file)

    assert result == {}


def test_deve_persistir_arquivo_de_estado_quando_salvar_json(tmp_path):
    state_file = tmp_path / "subdir" / "state.json"
    expected = {"test": 123}

    _save_state(state_file, expected)

    assert state_file.exists()
    assert json.loads(state_file.read_text(encoding="utf-8")) == expected
