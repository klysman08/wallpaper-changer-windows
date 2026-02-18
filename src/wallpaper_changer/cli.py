"""Interface de linha de comando (CLI) do WallpaperChanger."""
from __future__ import annotations
import time
from pathlib import Path
import click
import schedule
from .config import load_config
from .monitor import get_monitors
from .wallpaper import apply_random, apply_split

@click.group()
def main() -> None:
    """WallpaperChanger - Controle de papel de parede para Windows 11."""

@main.command("apply")
@click.option("--mode",   default=None, help="random | split2 | split4")
@click.option("--config", default=None, help="Caminho para settings.toml")
def apply_cmd(mode, config):
    """Aplica o wallpaper imediatamente."""
    cfg      = load_config(Path(config) if config else None)
    monitors = get_monitors()
    out_dir  = Path(cfg["paths"]["output_folder"])
    out_dir.mkdir(parents=True, exist_ok=True)
    active_mode = mode or cfg["general"]["mode"]
    click.echo(f"[INFO] Modo: {active_mode} | Monitores: {len(monitors)}")
    if active_mode == "random":
        out = apply_random(cfg, monitors, out_dir)
    elif active_mode == "split2":
        out = apply_split(cfg, monitors, out_dir, splits=2)
    elif active_mode == "split4":
        out = apply_split(cfg, monitors, out_dir, splits=4)
    else:
        raise click.BadParameter(f"Modo invalido: {active_mode}")
    click.echo(f"[OK] Wallpaper aplicado -> {out}")

@main.command("watch")
@click.option("--config", default=None, help="Caminho para settings.toml")
def watch_cmd(config):
    """Troca o wallpaper automaticamente no intervalo configurado."""
    cfg      = load_config(Path(config) if config else None)
    interval = cfg["general"]["interval"]
    if interval <= 0:
        click.echo("[ERRO] Defina interval > 0 em settings.toml para usar watch.")
        return
    click.echo(f"[INFO] Trocando wallpaper a cada {interval}s. Ctrl+C para sair.")
    def job():
        monitors = get_monitors()
        out_dir  = Path(cfg["paths"]["output_folder"])
        out_dir.mkdir(parents=True, exist_ok=True)
        mode = cfg["general"]["mode"]
        if mode == "random":
            apply_random(cfg, monitors, out_dir)
        elif mode == "split2":
            apply_split(cfg, monitors, out_dir, splits=2)
        elif mode == "split4":
            apply_split(cfg, monitors, out_dir, splits=4)
        click.echo("[OK] Wallpaper atualizado.")
    job()
    schedule.every(interval).seconds.do(job)
    try:
        while True:
            schedule.run_pending()
            time.sleep(1)
    except KeyboardInterrupt:
        click.echo("\n[INFO] Encerrado.")
