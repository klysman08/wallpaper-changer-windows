"""Leitura e validacao das configuracoes do projeto."""
from __future__ import annotations
import sys
from pathlib import Path

if sys.version_info >= (3, 11):
    import tomllib
else:
    import tomli as tomllib  # type: ignore[no-redef]

# Sobe 3 niveis: config.py -> wallpaper_changer -> src -> raiz do projeto
DEFAULT_CONFIG = Path(__file__).resolve().parents[2] / "config" / "settings.toml"

def load_config(path: Path | None = None) -> dict:
    target = path if path is not None else DEFAULT_CONFIG
    if not target.exists():
        raise FileNotFoundError(f"Arquivo de configuracao nao encontrado: {target}")
    with open(target, "rb") as f:
        return tomllib.load(f)
