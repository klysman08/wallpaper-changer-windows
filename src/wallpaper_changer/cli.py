"""Interface de linha de comando (CLI) do WallpaperChanger."""
from __future__ import annotations

import time
from pathlib import Path

import click
import schedule

from .config import load_config, resolve_path
from .monitor import get_monitors
from .wallpaper import apply_wallpaper


@click.group()
def main() -> None:
    """WallpaperChanger — Collage wallpaper para Windows."""


@main.command("apply")
@click.option("--selection", default=None, help="random | sequential")
@click.option("--collage-count", default=None, type=int, help="Imagens por monitor (1-8)")
@click.option("--config", default=None, help="Caminho para settings.toml")
def apply_cmd(selection: str | None, collage_count: int | None, config: str | None) -> None:
    """Aplica o wallpaper collage imediatamente."""
    cfg = load_config(Path(config) if config else None)
    monitors = get_monitors()
    out_dir = resolve_path(cfg["paths"]["output_folder"])
    out_dir.mkdir(parents=True, exist_ok=True)

    if selection:
        cfg["general"]["selection"] = selection
    if collage_count:
        cfg["general"]["collage_count"] = collage_count

    active_sel = cfg["general"].get("selection", "random")
    count = cfg["general"].get("collage_count", 4)
    click.echo(f"[INFO] Collage {count} imgs | Selecao: {active_sel} | Monitores: {len(monitors)}")

    out = apply_wallpaper(cfg, monitors, out_dir)
    click.echo(f"[OK] Wallpaper aplicado -> {out}")


@main.command("watch")
@click.option("--config", default=None, help="Caminho para settings.toml")
def watch_cmd(config: str | None) -> None:
    """Troca o wallpaper automaticamente no intervalo configurado."""
    cfg = load_config(Path(config) if config else None)
    interval = cfg["general"]["interval"]
    if interval <= 0:
        click.echo("[ERRO] Defina interval > 0 em settings.toml para usar watch.")
        return
    click.echo(f"[INFO] Trocando wallpaper a cada {interval}s. Ctrl+C para sair.")

    def job() -> None:
        monitors = get_monitors()
        out_dir = resolve_path(cfg["paths"]["output_folder"])
        out_dir.mkdir(parents=True, exist_ok=True)
        apply_wallpaper(cfg, monitors, out_dir)
        click.echo("[OK] Wallpaper atualizado.")

    job()
    schedule.every(interval).seconds.do(job)
    try:
        while True:
            schedule.run_pending()
            time.sleep(1)
    except KeyboardInterrupt:
        click.echo("\n[INFO] Encerrado.")
