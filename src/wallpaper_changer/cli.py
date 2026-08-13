"""Interface de linha de comando (CLI) do WallpaperChanger."""
from __future__ import annotations

import time
from pathlib import Path

import click
import schedule

from .config import load_config, resolve_output_dir
from .monitor import get_monitors
from .video_wallpaper import VideoWallpaperPlayer, has_mpv, scan_video_folder
from .wallpaper import EFFECTS, apply_wallpaper


@click.group()
def main() -> None:
    """WallpaperChanger — Collage wallpaper para Windows."""


@main.command("apply")
@click.option("--selection", default=None, help="random | sequential")
@click.option(
    "--collage-count",
    default=None,
    type=click.IntRange(1, 8),
    help="Imagens por monitor (1-8)",
)
@click.option(
    "--effect",
    default=None,
    type=click.Choice(EFFECTS),
    help="Efeito visual: normal | bw | vintage | hdr",
)
@click.option("--config", default=None, help="Caminho para settings.toml")
def apply_cmd(
    selection: str | None,
    collage_count: int | None,
    effect: str | None,
    config: str | None,
) -> None:
    """Aplica o wallpaper collage imediatamente."""
    cfg = load_config(Path(config) if config else None)
    monitors = get_monitors()
    out_dir = resolve_output_dir(cfg)
    out_dir.mkdir(parents=True, exist_ok=True)

    if selection:
        cfg["general"]["selection"] = selection
    if collage_count:
        cfg["general"]["collage_count"] = collage_count
    if effect:
        cfg["display"]["effect"] = effect

    active_sel = cfg["general"].get("selection", "random")
    count = cfg["general"].get("collage_count", 4)
    active_effect = cfg["display"].get("effect", "normal")
    click.echo(
        f"[INFO] Collage {count} imgs | Selecao: {active_sel} | "
        f"Efeito: {active_effect} | Monitores: {len(monitors)}"
    )

    out, _imgs = apply_wallpaper(cfg, monitors, out_dir)
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
        out_dir = resolve_output_dir(cfg)
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


@main.command("video")
@click.option("--folder", default=None, help="Pasta com os videos")
@click.option("--loop/--no-loop", default=None, help="Repetir a playlist (padrao: config)")
@click.option("--sound/--no-sound", default=None, help="Reproduzir o audio do video")
@click.option("--config", default=None, help="Caminho para settings.toml")
def video_cmd(
    folder: str | None,
    loop: bool | None,
    sound: bool | None,
    config: str | None,
) -> None:
    """Define um video como wallpaper (renderizado na camada WORKERW)."""
    cfg = load_config(Path(config) if config else None)
    vcfg = cfg.get("video", {})
    folder = folder or vcfg.get("folder", "")
    loop = vcfg.get("loop", True) if loop is None else loop
    sound = vcfg.get("sound", False) if sound is None else sound

    if not has_mpv():
        click.echo("[ERRO] python-mpv/libmpv-2.dll nao disponivel. Instale python-mpv "
                   "e coloque libmpv-2.dll no PATH (ou em MPV_DLL_DIR).")
        return
    if not folder:
        click.echo("[ERRO] Informe --folder ou defina [video].folder em settings.toml.")
        return

    videos = scan_video_folder(folder)
    if not videos:
        click.echo(f"[ERRO] Nenhum video encontrado em: {folder}")
        return

    monitors = get_monitors()
    if not monitors:
        click.echo("[ERRO] Nenhum monitor detectado.")
        return

    player = VideoWallpaperPlayer()
    player.configure(videos=videos, loop=loop, sound=sound, monitors=monitors)
    try:
        player.start()
    except Exception as exc:
        click.echo(f"[ERRO] {exc}")
        return

    click.echo(f"[OK] Reproduzindo {len(videos)} video(s) | Loop: {loop} | "
               f"Som: {sound} | Monitores: {len(monitors)}. Ctrl+C para parar.")
    try:
        while player.is_running():
            time.sleep(1)
    except KeyboardInterrupt:
        click.echo("\n[INFO] Parando video...")
    finally:
        player.stop()
