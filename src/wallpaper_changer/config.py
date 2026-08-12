"""Leitura, validacao e persistencia das configuracoes do projeto."""

from __future__ import annotations

import os
import shutil
import sys
import tempfile
import threading
from pathlib import Path

if sys.version_info >= (3, 11):
    import tomllib
else:
    import tomli as tomllib  # type: ignore[no-redef]

# Raiz do projeto:
#   - Desenvolvimento:  config.py -> wallpaper_changer -> src -> raiz
#   - Executavel (.exe): sys.executable fica dentro da pasta de distribuicao
if getattr(sys, "frozen", False):
    # PyInstaller --onedir: o .exe esta na raiz do pacote distribuido
    PROJECT_ROOT = Path(sys.executable).resolve().parent
else:
    PROJECT_ROOT = Path(__file__).resolve().parents[2]
_SAVE_LOCK = threading.Lock()

APP_NAME = "WallpaperChanger"

# Arquivos que pertencem ao usuario, nao a instalacao. Migrados de PROJECT_ROOT/config
# na primeira execucao — ver _migrate_legacy_files().
_USER_FILES = ("settings.toml", "state.json", "transparency.json")


def get_project_root() -> Path:
    """Retorna o diretorio raiz do projeto (ou da instalacao, se congelado)."""
    return PROJECT_ROOT


def get_user_config_dir() -> Path:
    """Diretorio de configuracao do usuario (roaming).

    Antes tudo vivia dentro da pasta de instalacao, o que funciona num checkout mas
    falha assim que o app e instalado em ``C:\\Program Files``: um processo sem
    privilegios nao consegue escrever ali. Configuracao, historico de rotacao e
    transparencia passam a viver em ``%APPDATA%\\WallpaperChanger``.

    ``WALLPAPER_CHANGER_CONFIG_DIR`` sobrepoe o caminho (usado nos testes).
    """
    override = os.environ.get("WALLPAPER_CHANGER_CONFIG_DIR")
    if override:
        return Path(override)
    base = os.environ.get("APPDATA")
    root = Path(base) if base else Path.home() / "AppData" / "Roaming"
    return root / APP_NAME


def get_user_data_dir() -> Path:
    """Diretorio de dados locais (nao-roaming): saida de wallpaper, caches.

    Separado da configuracao porque os BMPs compostos sao grandes e especificos da
    maquina — nao devem viajar num perfil roaming.
    """
    override = os.environ.get("WALLPAPER_CHANGER_DATA_DIR")
    if override:
        return Path(override)
    base = os.environ.get("LOCALAPPDATA")
    root = Path(base) if base else Path.home() / "AppData" / "Local"
    return root / APP_NAME


def get_default_config_path() -> Path:
    """Caminho do settings.toml do usuario."""
    return get_user_config_dir() / "settings.toml"


def get_state_file() -> Path:
    """Caminho do historico de selecao de imagens."""
    return get_user_config_dir() / "state.json"


def get_transparency_file() -> Path:
    """Caminho das opacidades salvas por janela."""
    return get_user_config_dir() / "transparency.json"


def resolve_output_dir(cfg: dict) -> Path:
    """Resolve ``paths.output_folder`` para um diretorio gravavel.

    Um caminho absoluto e respeitado como escolha explicita do usuario. Um caminho
    relativo e resolvido a partir de :func:`get_user_data_dir`, e nao da pasta de
    instalacao — resolver a partir dela e justamente o que quebra apos instalar.
    """
    raw = cfg.get("paths", {}).get("output_folder") or "output"
    p = Path(raw)
    return p if p.is_absolute() else get_user_data_dir() / p


def _migrate_legacy_files() -> None:
    """Copia arquivos do usuario da pasta de instalacao para %APPDATA%, uma vez.

    Preserva configuracoes existentes ao atualizar a partir de uma versao antiga (ou
    de um checkout de desenvolvimento). Nunca sobrescreve o que ja esta em %APPDATA%,
    e nunca remove o original — a migracao e sempre reversivel.
    """
    legacy_dir = PROJECT_ROOT / "config"
    if not legacy_dir.is_dir():
        return
    target_dir = get_user_config_dir()
    for name in _USER_FILES:
        source = legacy_dir / name
        target = target_dir / name
        if not source.is_file() or target.exists():
            continue
        try:
            target_dir.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, target)
        except OSError:
            # Migracao e best-effort: seguimos com os padroes se ela falhar.
            pass


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
    if path is not None:
        target = path
    else:
        # Primeira execucao apos a mudanca para %APPDATA%: traz o que existia na
        # pasta de instalacao antes de decidir que o arquivo esta faltando.
        _migrate_legacy_files()
        target = get_default_config_path()
    if not target.exists():
        raise FileNotFoundError(f"Arquivo de configuracao nao encontrado: {target}")
    with open(target, "rb") as f:
        cfg = tomllib.load(f)
    # Armazena o caminho do arquivo para poder salvar depois
    cfg["_config_path"] = str(target)
    return cfg


def save_config(cfg: dict, path: Path | None = None) -> None:
    """Persiste as configuracoes de volta no arquivo TOML."""
    target = path or Path(cfg.get("_config_path") or get_default_config_path())
    target.parent.mkdir(parents=True, exist_ok=True)

    lines: list[str] = []

    def _fmt(v: object) -> str:
        if isinstance(v, bool):
            return "true" if v else "false"
        if isinstance(v, str):
            # Escape backslashes and double-quotes for valid TOML
            escaped = v.replace("\\", "\\\\").replace('"', '\\"')
            return f'"{escaped}"'
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

    content = "\n".join(lines)
    # A process interruption must never leave settings.toml half-written.
    with _SAVE_LOCK:
        fd, tmp_name = tempfile.mkstemp(
            prefix=f".{target.name}.", suffix=".tmp", dir=target.parent
        )
        try:
            with os.fdopen(fd, "w", encoding="utf-8", newline="\n") as tmp:
                tmp.write(content)
                tmp.flush()
                os.fsync(tmp.fileno())
            os.replace(tmp_name, target)
        except Exception:
            try:
                os.unlink(tmp_name)
            except OSError:
                pass
            raise
