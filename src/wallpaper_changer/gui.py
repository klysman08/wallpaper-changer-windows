"""Interface grafica (GUI) do WallpaperChanger – CustomTkinter."""
from __future__ import annotations

import threading
import time
import tkinter as tk
from pathlib import Path
from tkinter import filedialog, messagebox

import customtkinter as ctk
import schedule

from .config import load_config, save_config, resolve_path
from .monitor import Monitor, get_monitors
from .wallpaper import apply_random, apply_split

# ── Tema global ──────────────────────────────────────────────────────────────
ctk.set_appearance_mode("dark")
ctk.set_default_color_theme("blue")

_MODES     = ["random", "split2", "split4"]
_FIT_MODES = ["fill", "fit", "stretch", "center"]

_MODE_LABELS = {
    "random": "Aleatório (1 imagem)",
    "split2": "Split 2 – dois monitores",
    "split4": "Split 4 – quatro monitores",
}
_FIT_LABELS = {
    "fill":    "Fill (preenche, corta)",
    "fit":     "Fit (sem corte, barras)",
    "stretch": "Stretch (distorce)",
    "center":  "Center (sem redimensionar)",
}

_MON_COLORS = ["#2D6BE4", "#E44B2D", "#2DBA4E", "#D4A027"]
_BG_CANVAS  = "#1a1a2e"


# ── App principal ─────────────────────────────────────────────────────────────
class WallpaperChangerApp(ctk.CTk):
    def __init__(self) -> None:
        super().__init__()

        self.title("WallpaperChanger")
        self.geometry("740x860")
        self.minsize(680, 780)
        self.grid_columnconfigure(0, weight=1)
        self.grid_rowconfigure(2, weight=1)

        self._cfg      = load_config()
        self._monitors: list[Monitor] = []
        self._watching  = False
        self._watch_thr: threading.Thread | None = None

        self._build_header()
        self._build_monitor_panel()
        self._build_settings_tabs()
        self._build_action_bar()
        self._build_status_bar()

        self._refresh_monitors()

    # ── Header ────────────────────────────────────────────────────────────────
    def _build_header(self) -> None:
        hdr = ctk.CTkFrame(self, corner_radius=12, fg_color=("#1f3a6e", "#0d1b40"))
        hdr.grid(row=0, column=0, sticky="ew", padx=16, pady=(16, 6))
        hdr.grid_columnconfigure(1, weight=1)

        ctk.CTkLabel(
            hdr, text="🖼", font=ctk.CTkFont(size=36),
        ).grid(row=0, column=0, rowspan=2, padx=(18, 12), pady=12)

        ctk.CTkLabel(
            hdr, text="WallpaperChanger",
            font=ctk.CTkFont(size=22, weight="bold"),
            anchor="w",
        ).grid(row=0, column=1, sticky="w", pady=(12, 0))

        ctk.CTkLabel(
            hdr, text="Painel de controle para Windows 11",
            font=ctk.CTkFont(size=12),
            text_color=("gray60", "gray70"),
            anchor="w",
        ).grid(row=1, column=1, sticky="w", pady=(0, 12))

        self._lbl_mon_count = ctk.CTkLabel(
            hdr, text="", font=ctk.CTkFont(size=12),
            text_color=("gray60", "gray70"),
        )
        self._lbl_mon_count.grid(row=0, column=2, rowspan=2, padx=18, pady=12)

    # ── Monitor preview panel ─────────────────────────────────────────────────
    def _build_monitor_panel(self) -> None:
        frame = ctk.CTkFrame(self, corner_radius=10)
        frame.grid(row=1, column=0, sticky="ew", padx=16, pady=6)
        frame.grid_columnconfigure(0, weight=1)

        ctk.CTkLabel(
            frame, text="  Disposição dos Monitores",
            font=ctk.CTkFont(size=13, weight="bold"),
            anchor="w",
        ).grid(row=0, column=0, sticky="w", padx=12, pady=(10, 4))

        self._canvas = tk.Canvas(
            frame, height=160, bg=_BG_CANVAS,
            highlightthickness=0, bd=0,
        )
        self._canvas.grid(row=1, column=0, sticky="ew", padx=12, pady=(0, 12))
        self._canvas.bind("<Configure>", lambda _e: self._draw_monitors())

        self._refresh_btn = ctk.CTkButton(
            frame, text="↻ Detectar Monitores", width=160, height=28,
            font=ctk.CTkFont(size=12),
            corner_radius=6,
            command=self._refresh_monitors,
        )
        self._refresh_btn.grid(row=0, column=0, sticky="e", padx=12, pady=(10, 4))

    # ── Tabs de configuração ──────────────────────────────────────────────────
    def _build_settings_tabs(self) -> None:
        self._tabview = ctk.CTkTabview(self, corner_radius=10)
        self._tabview.grid(row=2, column=0, sticky="nsew", padx=16, pady=6)
        self._tabview.add("  Geral  ")
        self._tabview.add("  Pastas  ")
        self._tabview.set("  Geral  ")

        self._build_tab_general()
        self._build_tab_paths()

    # ── Tab: Geral ────────────────────────────────────────────────────────────
    def _build_tab_general(self) -> None:
        tab = self._tabview.tab("  Geral  ")
        tab.grid_columnconfigure(1, weight=1)

        # Modo
        ctk.CTkLabel(tab, text="Modo de exibição:", anchor="w",
                     font=ctk.CTkFont(size=13)).grid(
            row=0, column=0, sticky="w", padx=(16, 8), pady=(20, 10))

        current_mode = self._cfg["general"]["mode"]
        self._mode_var = ctk.StringVar(value=_MODE_LABELS.get(current_mode, current_mode))
        mode_menu = ctk.CTkOptionMenu(
            tab,
            values=list(_MODE_LABELS.values()),
            variable=self._mode_var,
            width=240,
            font=ctk.CTkFont(size=12),
            command=self._on_mode_change,
        )
        mode_menu.grid(row=0, column=1, sticky="w", padx=(0, 16), pady=(20, 10))

        # Fit mode
        ctk.CTkLabel(tab, text="Ajuste de imagem:", anchor="w",
                     font=ctk.CTkFont(size=13)).grid(
            row=1, column=0, sticky="w", padx=(16, 8), pady=10)

        current_fit = self._cfg["display"]["fit_mode"]
        self._fit_var = ctk.StringVar(value=_FIT_LABELS.get(current_fit, current_fit))
        ctk.CTkOptionMenu(
            tab,
            values=list(_FIT_LABELS.values()),
            variable=self._fit_var,
            width=240,
            font=ctk.CTkFont(size=12),
        ).grid(row=1, column=1, sticky="w", padx=(0, 16), pady=10)

        # Intervalo
        ctk.CTkLabel(tab, text="Intervalo de rotação:", anchor="w",
                     font=ctk.CTkFont(size=13)).grid(
            row=2, column=0, sticky="w", padx=(16, 8), pady=10)

        interval_frame = ctk.CTkFrame(tab, fg_color="transparent")
        interval_frame.grid(row=2, column=1, sticky="w", padx=(0, 16), pady=10)

        self._interval_var = ctk.StringVar(value=str(self._cfg["general"]["interval"]))
        ctk.CTkEntry(
            interval_frame,
            textvariable=self._interval_var,
            width=80,
            font=ctk.CTkFont(size=12),
            justify="center",
        ).pack(side="left")
        ctk.CTkLabel(interval_frame, text=" segundos",
                     font=ctk.CTkFont(size=12)).pack(side="left")

        # Descrição dos modos
        self._mode_desc = ctk.CTkLabel(
            tab, text="",
            font=ctk.CTkFont(size=11, slant="italic"),
            text_color=("gray55", "gray65"),
            wraplength=460,
            justify="left",
        )
        self._mode_desc.grid(row=3, column=0, columnspan=2,
                             sticky="w", padx=16, pady=(8, 20))
        self._on_mode_change(self._mode_var.get())

    # ── Tab: Pastas ───────────────────────────────────────────────────────────
    def _build_tab_paths(self) -> None:
        tab = self._tabview.tab("  Pastas  ")
        tab.grid_columnconfigure(1, weight=1)

        headers = [
            ("Monitor 1",  "monitor_1"),
            ("Monitor 2",  "monitor_2"),
            ("Monitor 3",  "monitor_3"),
            ("Monitor 4",  "monitor_4"),
            ("Aleatório",  "random_folder"),
        ]

        self._path_vars: dict[str, ctk.StringVar] = {}

        for row_idx, (label, key) in enumerate(headers):
            ctk.CTkLabel(tab, text=f"{label}:", anchor="w",
                         font=ctk.CTkFont(size=13)).grid(
                row=row_idx, column=0, sticky="w", padx=(16, 8),
                pady=(16 if row_idx == 0 else 8, 8),
            )

            raw = self._cfg["paths"][key]
            resolved = str(resolve_path(raw))
            var = ctk.StringVar(value=resolved)
            self._path_vars[key] = var

            entry = ctk.CTkEntry(
                tab, textvariable=var,
                font=ctk.CTkFont(size=11),
            )
            entry.grid(row=row_idx, column=1, sticky="ew",
                       padx=(0, 6), pady=(16 if row_idx == 0 else 8, 8))

            btn = ctk.CTkButton(
                tab, text="📁", width=36, height=32,
                font=ctk.CTkFont(size=14),
                fg_color="transparent",
                hover_color=("gray75", "gray30"),
                corner_radius=6,
                command=lambda v=var: self._browse_folder(v),
            )
            btn.grid(row=row_idx, column=2, padx=(0, 16),
                     pady=(16 if row_idx == 0 else 8, 8))

    # ── Barra de ações ────────────────────────────────────────────────────────
    def _build_action_bar(self) -> None:
        bar = ctk.CTkFrame(self, corner_radius=10)
        bar.grid(row=3, column=0, sticky="ew", padx=16, pady=6)
        bar.grid_columnconfigure((0, 1, 2), weight=1)

        self._apply_btn = ctk.CTkButton(
            bar, text="▶  Aplicar Agora",
            font=ctk.CTkFont(size=13, weight="bold"),
            height=40, corner_radius=8,
            command=self._apply_now,
        )
        self._apply_btn.grid(row=0, column=0, padx=(16, 6), pady=12, sticky="ew")

        ctk.CTkButton(
            bar, text="💾  Salvar Config",
            font=ctk.CTkFont(size=13),
            height=40, corner_radius=8,
            fg_color=("gray70", "gray30"),
            hover_color=("gray60", "gray25"),
            command=self._save_config,
        ).grid(row=0, column=1, padx=6, pady=12, sticky="ew")

        self._watch_btn = ctk.CTkButton(
            bar, text="⏱  Iniciar Watch",
            font=ctk.CTkFont(size=13),
            height=40, corner_radius=8,
            fg_color=("#1a6b1a", "#1a4d1a"),
            hover_color=("#145214", "#123a12"),
            command=self._toggle_watch,
        )
        self._watch_btn.grid(row=0, column=2, padx=(6, 16), pady=12, sticky="ew")

    # ── Barra de status ───────────────────────────────────────────────────────
    def _build_status_bar(self) -> None:
        bar = ctk.CTkFrame(self, height=32, corner_radius=0,
                           fg_color=("gray88", "gray17"))
        bar.grid(row=4, column=0, sticky="ew", padx=0, pady=(6, 0))
        bar.grid_columnconfigure(0, weight=1)

        self._status_lbl = ctk.CTkLabel(
            bar, text="  Pronto.",
            font=ctk.CTkFont(size=11),
            text_color=("gray40", "gray70"),
            anchor="w",
        )
        self._status_lbl.grid(row=0, column=0, sticky="w", padx=12)

    # ── Monitor preview ───────────────────────────────────────────────────────
    def _refresh_monitors(self) -> None:
        try:
            self._monitors = get_monitors()
        except Exception as e:
            self._monitors = []
            self._set_status(f"Erro ao detectar monitores: {e}", error=True)
            return

        count = len(self._monitors)
        self._lbl_mon_count.configure(
            text=f"{count} monitor{'es' if count != 1 else ''} detectado{'s' if count != 1 else ''}"
        )
        self._draw_monitors()

    def _draw_monitors(self) -> None:
        c = self._canvas
        c.delete("all")
        if not self._monitors:
            c.create_text(
                c.winfo_width() // 2 or 340,
                80,
                text="Nenhum monitor detectado",
                fill="#666",
                font=("Segoe UI", 12),
            )
            return

        canvas_w = c.winfo_width() or 700
        canvas_h = c.winfo_height() or 160

        # Bounds do virtual desktop
        min_x = min(m.x for m in self._monitors)
        min_y = min(m.y for m in self._monitors)
        max_x = max(m.x + m.width  for m in self._monitors)
        max_y = max(m.y + m.height for m in self._monitors)
        vd_w  = max_x - min_x or 1
        vd_h  = max_y - min_y or 1

        pad    = 20
        scale  = min((canvas_w - pad * 2) / vd_w, (canvas_h - pad * 2) / vd_h)
        off_x  = pad + (canvas_w - pad * 2 - vd_w * scale) / 2
        off_y  = pad + (canvas_h - pad * 2 - vd_h * scale) / 2

        # Fundo
        c.create_rectangle(0, 0, canvas_w, canvas_h,
                           fill=_BG_CANVAS, outline="")

        for m in self._monitors:
            color = _MON_COLORS[m.index % len(_MON_COLORS)]
            rx1 = off_x + (m.x - min_x) * scale
            ry1 = off_y + (m.y - min_y) * scale
            rx2 = rx1 + m.width  * scale
            ry2 = ry1 + m.height * scale

            # Sombra
            c.create_rectangle(rx1 + 3, ry1 + 3, rx2 + 3, ry2 + 3,
                               fill="#000", outline="")

            # Corpo
            c.create_rectangle(rx1, ry1, rx2, ry2,
                               fill=color, outline="#aaaaaa", width=1)

            # Gradiente simulado com retangulo claro no topo
            grad_h = (ry2 - ry1) * 0.35
            # Clareia levemente a cor original para simular gradiente
            r = int(color[1:3], 16)
            g = int(color[3:5], 16)
            b = int(color[5:7], 16)
            light = "#{:02x}{:02x}{:02x}".format(
                min(255, r + 60), min(255, g + 60), min(255, b + 60)
            )
            c.create_rectangle(rx1, ry1, rx2, ry1 + grad_h,
                               fill=light, outline="")

            # Label
            font_size = max(8, int((rx2 - rx1) * 0.14))
            cx = (rx1 + rx2) / 2
            cy = (ry1 + ry2) / 2
            c.create_text(cx, cy - font_size * 0.7,
                         text=f"M{m.index + 1}",
                         fill="white",
                         font=("Segoe UI", font_size, "bold"))
            c.create_text(cx, cy + font_size * 0.7,
                         text=f"{m.width}x{m.height}",
                         fill="#cccccc",
                         font=("Segoe UI", max(7, font_size - 2)))

    # ── Callbacks ─────────────────────────────────────────────────────────────
    def _on_mode_change(self, label: str) -> None:
        descs = {
            list(_MODE_LABELS.values())[0]:
                "Uma única imagem aleatória é esticada por todos os monitores.",
            list(_MODE_LABELS.values())[1]:
                "Cada um dos dois primeiros monitores recebe uma imagem diferente.",
            list(_MODE_LABELS.values())[2]:
                "Os quatro primeiros monitores recebem imagens independentes.",
        }
        self._mode_desc.configure(text=descs.get(label, ""))

    def _browse_folder(self, var: ctk.StringVar) -> None:
        current = Path(var.get())
        initial = str(current) if current.exists() else str(Path.home())
        chosen = filedialog.askdirectory(title="Selecione a pasta de imagens",
                                        initialdir=initial)
        if chosen:
            var.set(chosen)

    # ── Aplicar wallpaper ─────────────────────────────────────────────────────
    def _collect_config(self) -> dict:
        """Monta um dict de config atualizado com os valores da UI."""
        cfg = dict(self._cfg)  # shallow copy

        # Reverse-map label -> key
        inv_mode = {v: k for k, v in _MODE_LABELS.items()}
        inv_fit  = {v: k for k, v in _FIT_LABELS.items()}
        mode = inv_mode.get(self._mode_var.get(), self._mode_var.get())
        fit  = inv_fit.get(self._fit_var.get(),  self._fit_var.get())

        try:
            interval = max(1, int(self._interval_var.get()))
        except ValueError:
            interval = 300

        cfg["general"] = {**cfg.get("general", {}), "mode": mode, "interval": interval}
        cfg["display"] = {**cfg.get("display", {}),  "fit_mode": fit}

        paths = dict(cfg.get("paths", {}))
        for key, var in self._path_vars.items():
            paths[key] = var.get()
        cfg["paths"] = paths

        return cfg

    def _apply_now(self) -> None:
        if not self._monitors:
            self._set_status("Nenhum monitor detectado. Clique em '↻ Detectar Monitores'.",
                             error=True)
            return

        self._apply_btn.configure(state="disabled", text="Aplicando…")
        self._set_status("Aplicando wallpaper…")

        def _worker() -> None:
            try:
                cfg     = self._collect_config()
                out_dir = resolve_path(cfg["paths"]["output_folder"])
                out_dir.mkdir(parents=True, exist_ok=True)
                mode = cfg["general"]["mode"]

                if mode == "random":
                    out = apply_random(cfg, self._monitors, out_dir)
                elif mode == "split2":
                    out = apply_split(cfg, self._monitors, out_dir, splits=2)
                elif mode == "split4":
                    out = apply_split(cfg, self._monitors, out_dir, splits=4)
                else:
                    raise ValueError(f"Modo inválido: {mode}")

                self.after(0, lambda: self._set_status(f"✔ Wallpaper aplicado → {Path(str(out)).name}"))
            except Exception as exc:
                self.after(0, lambda: self._set_status(f"Erro: {exc}", error=True))
            finally:
                self.after(0, lambda: self._apply_btn.configure(
                    state="normal", text="▶  Aplicar Agora"))

        threading.Thread(target=_worker, daemon=True).start()

    # ── Salvar config ─────────────────────────────────────────────────────────
    def _save_config(self) -> None:
        try:
            cfg = self._collect_config()
            save_config(cfg)
            self._cfg = cfg
            self._set_status("✔ Configurações salvas com sucesso.")
        except Exception as exc:
            self._set_status(f"Erro ao salvar: {exc}", error=True)

    # ── Watch mode ────────────────────────────────────────────────────────────
    def _toggle_watch(self) -> None:
        if self._watching:
            self._watching = False
            self._watch_btn.configure(
                text="⏱  Iniciar Watch",
                fg_color=("#1a6b1a", "#1a4d1a"),
                hover_color=("#145214", "#123a12"),
            )
            self._set_status("Watch desativado.")
            schedule.clear()
        else:
            cfg      = self._collect_config()
            interval = cfg["general"]["interval"]
            self._watching = True
            self._watch_btn.configure(
                text="⏹  Parar Watch",
                fg_color=("#8b1a1a", "#5a1010"),
                hover_color=("#6b1212", "#480c0c"),
            )
            self._set_status(f"Watch ativo — trocando a cada {interval}s.")
            schedule.every(interval).seconds.do(self._apply_now)

            def _loop() -> None:
                while self._watching:
                    schedule.run_pending()
                    time.sleep(1)

            self._watch_thr = threading.Thread(target=_loop, daemon=True)
            self._watch_thr.start()

    # ── Status bar helper ─────────────────────────────────────────────────────
    def _set_status(self, msg: str, error: bool = False) -> None:
        color = ("#c0392b", "#e74c3c") if error else ("gray40", "gray70")
        self._status_lbl.configure(text=f"  {msg}", text_color=color)


# ── Entry point ───────────────────────────────────────────────────────────────
def run() -> None:
    """Inicia a interface grafica."""
    app = WallpaperChangerApp()
    app.mainloop()


if __name__ == "__main__":
    run()
