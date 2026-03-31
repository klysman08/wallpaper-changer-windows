from click.testing import CliRunner

from wallpaper_changer.cli import main


def test_deve_falhar_quando_collage_count_menor_que_limite_minimo():
    runner = CliRunner()

    result = runner.invoke(main, ["apply", "--collage-count", "0"])

    assert result.exit_code != 0
    assert "0 is not in the range 1<=x<=8" in result.output


def test_deve_falhar_quando_collage_count_maior_que_limite_maximo():
    runner = CliRunner()

    result = runner.invoke(main, ["apply", "--collage-count", "9"])

    assert result.exit_code != 0
    assert "9 is not in the range 1<=x<=8" in result.output


def test_deve_falhar_quando_effect_for_invalido():
    runner = CliRunner()

    result = runner.invoke(main, ["apply", "--effect", "nope"])

    assert result.exit_code != 0
    assert "Invalid value for '--effect'" in result.output
