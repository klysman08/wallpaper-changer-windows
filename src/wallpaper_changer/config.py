"""Leitura, validacao e persistencia das configuracoes do projeto."""
from __future__ import annotations
import sys
from pathlib import Path

if sys.version_info >= (3, 11):
    import tomllib
else:
    import tomli as tomllib  # type: ignore[no-redef]

# Raiz do projeto: config.py -> wallpaper_changer -> src -> raiz
PROJECT_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_CONFIG = PROJECT_ROOT / "config" / "settings.toml"


def get_project_root() -> Path:
    """Retorna o diretorio raiz do projeto."""
    return PROJECT_ROOT


def resolve_path(raw: str | Path, root: Path | None = None) -> Path:
    """
    Resolve um caminho relativo ou absoluto.
    Caminhos relativos sao resolvidos a partir de `root` (default: PROJECT_ROOT).
    """
    p = Path(raw)
    if p.is_absolute():
        return p
    base = root if root is not None else PROJECT_ROOT
    return (base / p).resolve()


def load_config(path: Path | None = None) -> dict:
    target = path if path is not None else DEFAULT_CONFIG
    if not target.exists():
        raise FileNotFoundError(f"Arquivo de configuracao nao encontrado: {target}")
    with open(target, "rb") as f:
        cfg = tomllib.load(f)
    # Armazena o caminho do arquivo para poder salvar depois
    cfg["_config_path"] = str(target)
    return cfg


def save_config(cfg: dict, path: Path | None = None) -> None:
    """Persiste as configuracoes de volta no arquivo TOML."""
    target = path or Path(cfg.get("_config_path", str(DEFAULT_CONFIG)))
    target.parent.mkdir(parents=True, exist_ok=True)

    lines: list[str] = []

    def _fmt(v: object) -> str:
        if isinstance(v, bool):
            return "true" if v else "false"
        if isinstance(v, str):
            return f'"{v}"'
        return str(v)

    for section, values in cfg.items():
        if section.startswith("_"):
            continue
        if not isinstance(values, dict):
            continue
        lines.append(f"[{section}]")
        for k, v in values.items():
            lines.append(f"{k} = {_fmt(v)}")
        lines.append("")

    target.write_text("\n".join(lines), encoding="utf-8")
