"""Interface grafica (GUI) do WallpaperChanger - ttkbootstrap."""
from __future__ import annotations

import threading
import time
import tkinter as tk
from pathlib import Path
from tkinter import filedialog

import pystray
import schedule
import ttkbootstrap as ttk
from PIL import Image as PILImage, ImageDraw
from ttkbootstrap.constants import *

from .config import load_config, save_config, resolve_path
from .monitor import Monitor, get_monitors
from .startup import is_startup_enabled, set_startup_enabled
from .wallpaper import apply_wallpaper

# ── Paleta ────────────────────────────────────────────────────────────────────
_MON_COLORS = ["#3a7bd5", "#e05252", "#3dba5a", "#d4a027", "#9b59b6"]
_BG_CANVAS  = "#1a1a2e"
_ACCENT     = "#3a7bd5"

# Dados dos modos de ajuste
_FIT_INFO: dict[str, tuple[str, str]] = {
    "fill":    ("Preencher",   "Expande para cobrir, corta o excesso"),
    "fit":     ("Ajustar",     "Encaixa sem cortar, adiciona barras pretas"),
    "stretch": ("Ampliar",     "Distorce para preencher exatamente"),
    "center":  ("Centralizar", "Sem redimensionar, centraliza na tela"),
    "span":    ("Estender",    "Imagem distribuida por todo o espaco"),
}

_SEL_LABELS = {"random": "Aleatorio", "sequential": "Sequencial"}


class WallpaperChangerApp(ttk.Window):
    def __init__(self) -> None:
        super().__init__(
            title="WallpaperChanger",
            themename="darkly",
            size=(760, 860),
            minsize=(680, 720),
            resizable=(True, True),
        )

        self._cfg = load_config()
        self._monitors: list[Monitor] = []
        self._watching = False
        self._watch_thr: threading.Thread | None = None
        self._tray_icon: pystray.Icon | None = None

        # ── Variaveis de estado ───────────────────────────────────────────────
        self._fit_var = tk.StringVar(value=self._cfg["display"]["fit_mode"])
        self._sel_var = tk.StringVar(value=self._cfg["general"].get("selection", "random"))
        self._interval_var = tk.StringVar(value=str(self._cfg["general"]["interval"]))
        self._collage_count_var = tk.IntVar(
            value=self._cfg["general"].get("collage_count", 4)
        )
        self._collage_same_var = tk.BooleanVar(
            value=bool(self._cfg["general"].get("collage_same_for_all", False))
        )
        self._startup_var = tk.BooleanVar(value=is_startup_enabled())

        self._fit_btns: dict[str, ttk.Button] = {}
        self._collage_btns: dict[int, ttk.Button] = {}
        self._draw_after_id: str | None = None  # debounce for monitor redraw

        # ── Construcao da UI ──────────────────────────────────────────────────
        self._build_ui()
        self._setup_tray()
        self._refresh_monitors()
        self.after(200, self._draw_monitors)

    # ══════════════════════════════════════════════════════════════════════════
    #   UI Construction
    # ══════════════════════════════════════════════════════════════════════════

    def _build_ui(self) -> None:
        # Main scrollable container
        container = ttk.Frame(self, padding=0)
        container.pack(fill=BOTH, expand=True)

        # Canvas + scrollbar for scrolling
        self._scroll_canvas = tk.Canvas(container, highlightthickness=0)
        scrollbar = ttk.Scrollbar(container, orient=VERTICAL, command=self._scroll_canvas.yview)
        self._scroll_frame = ttk.Frame(self._scroll_canvas)

        self._scroll_frame.bind(
            "<Configure>",
            lambda _: self._scroll_canvas.configure(scrollregion=self._scroll_canvas.bbox("all")),
        )
        self._scroll_win = self._scroll_canvas.create_window(
            (0, 0), window=self._scroll_frame, anchor="nw",
        )
        self._scroll_canvas.configure(yscrollcommand=scrollbar.set)

        # Keep inner frame width in sync with the canvas
        self._scroll_canvas.bind(
            "<Configure>",
            self._on_canvas_configure,
        )

        self._scroll_canvas.pack(side=LEFT, fill=BOTH, expand=True)
        scrollbar.pack(side=RIGHT, fill=Y)

        # Mouse wheel — only when hovering over the scroll canvas, not the treeview
        self._scroll_canvas.bind("<Enter>", lambda _: self._bind_mousewheel(True))
        self._scroll_canvas.bind("<Leave>", lambda _: self._bind_mousewheel(False))

        main = self._scroll_frame
        main.columnconfigure(0, weight=1)

        self._build_header(main)
        self._build_monitor_panel(main)
        self._build_collage_section(main)
        self._build_selection_section(main)
        self._build_fit_section(main)
        self._build_rotation_section(main)
        self._build_folder_section(main)
        self._build_action_bar(main)
        self._build_status_bar()

    # ── Scroll helpers ────────────────────────────────────────────────────────
    def _on_canvas_configure(self, event: tk.Event) -> None:
        """Stretch the inner frame to fill the canvas width."""
        self._scroll_canvas.itemconfigure(self._scroll_win, width=event.width)

    def _bind_mousewheel(self, bind: bool) -> None:
        if bind:
            self._scroll_canvas.bind_all(
                "<MouseWheel>",
                lambda e: self._scroll_canvas.yview_scroll(int(-1 * (e.delta / 120)), "units"),
            )
        else:
            self._scroll_canvas.unbind_all("<MouseWheel>")

    # ── Header ────────────────────────────────────────────────────────────────
    def _build_header(self, parent: ttk.Frame) -> None:
        hdr = ttk.Frame(parent, padding=(16, 14))
        hdr.grid(row=0, column=0, sticky=EW, padx=12, pady=(10, 4))
        hdr.columnconfigure(1, weight=1)

        ttk.Label(
            hdr, text="WP", font=("Segoe UI", 20, "bold"),
            foreground=_ACCENT, anchor=W,
        ).grid(row=0, column=0, rowspan=2, padx=(0, 12))

        ttk.Label(
            hdr, text="WallpaperChanger",
            font=("Segoe UI", 18, "bold"), anchor=W,
        ).grid(row=0, column=1, sticky=W)

        ttk.Label(
            hdr, text="Painel de controle  |  Windows",
            font=("Segoe UI", 10), foreground="gray", anchor=W,
        ).grid(row=1, column=1, sticky=W)

        self._lbl_mon_count = ttk.Label(
            hdr, text="detectando...", font=("Segoe UI", 10), foreground="gray",
        )
        self._lbl_mon_count.grid(row=0, column=2, rowspan=2, padx=(12, 0))

    # ── Monitor Preview ───────────────────────────────────────────────────────
    def _build_monitor_panel(self, parent: ttk.Frame) -> None:
        frame = ttk.Labelframe(parent, text="Monitores", padding=8)
        frame.grid(row=1, column=0, sticky=EW, padx=12, pady=4)
        frame.columnconfigure(0, weight=1)

        bar = ttk.Frame(frame)
        bar.grid(row=0, column=0, sticky=EW)
        bar.columnconfigure(0, weight=1)

        ttk.Button(
            bar, text="Detectar", style="Outline.TButton",
            command=self._refresh_monitors, width=12,
        ).grid(row=0, column=1, padx=4)

        self._mon_canvas = tk.Canvas(
            frame, height=130, bg=_BG_CANVAS, highlightthickness=0, bd=0,
        )
        self._mon_canvas.grid(row=1, column=0, sticky=EW, pady=(6, 0))
        self._mon_canvas.bind("<Configure>", lambda _: self._schedule_draw_monitors())

    # ── Collage Settings ──────────────────────────────────────────────────────
    def _build_collage_section(self, parent: ttk.Frame) -> None:
        frame = ttk.Labelframe(parent, text="Collage — Imagens por Monitor", padding=10)
        frame.grid(row=2, column=0, sticky=EW, padx=12, pady=4)
        frame.columnconfigure(0, weight=1)

        # Number buttons row
        btn_row = ttk.Frame(frame)
        btn_row.grid(row=0, column=0, sticky=W, pady=(0, 6))

        for i, n in enumerate(range(1, 9)):
            btn = ttk.Button(
                btn_row, text=str(n), width=4,
                style="Outline.TButton",
                command=lambda k=n: self._select_collage_count(k),
            )
            btn.grid(row=0, column=i, padx=2)
            self._collage_btns[n] = btn

        self._select_collage_count(self._collage_count_var.get())

        # Same images checkbox
        ttk.Checkbutton(
            frame, text="Mesmas imagens em todos os monitores",
            variable=self._collage_same_var,
            style="Roundtoggle.Toolbutton",
        ).grid(row=1, column=0, sticky=W, pady=(4, 0))

    # ── Image Selection ───────────────────────────────────────────────────────
    def _build_selection_section(self, parent: ttk.Frame) -> None:
        frame = ttk.Labelframe(parent, text="Selecao de Imagens", padding=10)
        frame.grid(row=3, column=0, sticky=EW, padx=12, pady=4)

        btn_row = ttk.Frame(frame)
        btn_row.grid(row=0, column=0, sticky=W)

        self._sel_btns: dict[str, ttk.Radiobutton] = {}
        for key, label in _SEL_LABELS.items():
            rb = ttk.Radiobutton(
                btn_row, text=label, variable=self._sel_var, value=key,
                style="Toolbutton",
            )
            rb.pack(side=LEFT, padx=(0, 8))
            self._sel_btns[key] = rb

    # ── Fit Mode ──────────────────────────────────────────────────────────────
    def _build_fit_section(self, parent: ttk.Frame) -> None:
        frame = ttk.Labelframe(parent, text="Ajuste na Tela", padding=10)
        frame.grid(row=4, column=0, sticky=EW, padx=12, pady=4)
        frame.columnconfigure(tuple(range(len(_FIT_INFO))), weight=1)

        for ci, (fkey, (flabel, _)) in enumerate(_FIT_INFO.items()):
            btn = ttk.Button(
                frame, text=flabel, style="Outline.TButton",
                command=lambda k=fkey: self._select_fit(k),
            )
            btn.grid(row=0, column=ci, padx=2, pady=2, sticky=EW)
            self._fit_btns[fkey] = btn

        self._fit_desc = ttk.Label(
            frame, text="", font=("Segoe UI", 9), foreground="gray",
        )
        self._fit_desc.grid(row=1, column=0, columnspan=len(_FIT_INFO), sticky=W, pady=(6, 0))

        self._select_fit(self._fit_var.get())

    # ── Rotation / Timer ──────────────────────────────────────────────────────
    def _build_rotation_section(self, parent: ttk.Frame) -> None:
        frame = ttk.Labelframe(parent, text="Rotacao Automatica", padding=10)
        frame.grid(row=5, column=0, sticky=EW, padx=12, pady=4)
        frame.columnconfigure(0, weight=1)

        row1 = ttk.Frame(frame)
        row1.grid(row=0, column=0, sticky=W)

        ttk.Label(row1, text="Intervalo:").pack(side=LEFT, padx=(0, 6))
        ttk.Entry(
            row1, textvariable=self._interval_var, width=8, justify=CENTER,
        ).pack(side=LEFT)
        ttk.Label(row1, text=" segundos").pack(side=LEFT)

        # Startup option
        ttk.Checkbutton(
            frame, text="Iniciar com o Windows",
            variable=self._startup_var,
            command=self._on_startup_toggle,
            style="Roundtoggle.Toolbutton",
        ).grid(row=1, column=0, sticky=W, pady=(8, 0))

    # ── Folder Section ────────────────────────────────────────────────────────
    def _build_folder_section(self, parent: ttk.Frame) -> None:
        frame = ttk.Labelframe(parent, text="Pasta de Wallpapers", padding=10)
        frame.grid(row=6, column=0, sticky=EW, padx=12, pady=4)
        frame.columnconfigure(0, weight=1)

        ttk.Label(
            frame,
            text="Formatos suportados: jpg  jpeg  png  bmp  webp",
            font=("Segoe UI", 9), foreground="gray",
        ).grid(row=0, column=0, columnspan=2, sticky=W, pady=(0, 6))

        raw = self._cfg["paths"]["wallpapers_folder"]
        self._folder_var = tk.StringVar(value=str(resolve_path(raw)))

        entry = ttk.Entry(frame, textvariable=self._folder_var)
        entry.grid(row=1, column=0, sticky=EW, padx=(0, 6))
        entry.bind("<FocusOut>", lambda _: self._update_folder_info())

        ttk.Button(
            frame, text="...", width=4, style="Outline.TButton",
            command=self._browse_folder,
        ).grid(row=1, column=1)

        self._folder_info = ttk.Label(
            frame, text="", font=("Segoe UI", 9), foreground="gray",
        )
        self._folder_info.grid(row=2, column=0, columnspan=2, sticky=W, pady=(6, 0))

        # Image list (compact)
        self._img_tree = ttk.Treeview(
            frame, columns=("name",), show="headings", height=5,
            selectmode="none",
        )
        self._img_tree.heading("name", text="Imagens encontradas", anchor=W)
        self._img_tree.column("name", anchor=W)
        self._img_tree.grid(row=3, column=0, columnspan=2, sticky=EW, pady=(6, 0))

        tree_scroll = ttk.Scrollbar(frame, orient=VERTICAL, command=self._img_tree.yview)
        tree_scroll.grid(row=3, column=2, sticky="ns", pady=(6, 0))
        self._img_tree.configure(yscrollcommand=tree_scroll.set)

        self._update_folder_info()

    # ── Action Bar ────────────────────────────────────────────────────────────
    def _build_action_bar(self, parent: ttk.Frame) -> None:
        bar = ttk.Frame(parent, padding=(12, 8))
        bar.grid(row=7, column=0, sticky=EW, padx=12, pady=(8, 4))
        bar.columnconfigure((0, 1, 2), weight=1)

        self._apply_btn = ttk.Button(
            bar, text="Aplicar Agora", style="success.TButton",
            command=self._apply_now,
        )
        self._apply_btn.grid(row=0, column=0, padx=4, sticky=EW)

        ttk.Button(
            bar, text="Salvar Config", style="secondary.TButton",
            command=self._save_config,
        ).grid(row=0, column=1, padx=4, sticky=EW)

        self._watch_btn = ttk.Button(
            bar, text="Iniciar Watch", style="info.TButton",
            command=self._toggle_watch,
        )
        self._watch_btn.grid(row=0, column=2, padx=4, sticky=EW)

        ttk.Button(
            bar, text="Bandeja", style="dark.TButton",
            command=self._minimize_to_tray, width=10,
        ).grid(row=0, column=3, padx=(4, 0))

    # ── Status Bar ────────────────────────────────────────────────────────────
    def _build_status_bar(self) -> None:
        bar = ttk.Frame(self, padding=(12, 4))
        bar.pack(side=BOTTOM, fill=X)

        self._status_lbl = ttk.Label(
            bar, text="Pronto.", font=("Segoe UI", 9), foreground="gray",
        )
        self._status_lbl.pack(side=LEFT)

    # ══════════════════════════════════════════════════════════════════════════
    #   UI Interaction Logic
    # ══════════════════════════════════════════════════════════════════════════

    def _select_collage_count(self, n: int) -> None:
        self._collage_count_var.set(n)
        for k, btn in self._collage_btns.items():
            if k == n:
                btn.configure(style="primary.TButton")
            else:
                btn.configure(style="Outline.TButton")

    def _select_fit(self, key: str) -> None:
        self._fit_var.set(key)
        for k, btn in self._fit_btns.items():
            if k == key:
                btn.configure(style="primary.TButton")
            else:
                btn.configure(style="Outline.TButton")
        desc = _FIT_INFO.get(key, ("", ""))[1]
        self._fit_desc.configure(text=desc)

    def _on_startup_toggle(self) -> None:
        try:
            set_startup_enabled(self._startup_var.get())
            state = "ativado" if self._startup_var.get() else "desativado"
            self._set_status(f"Inicio automatico {state}.")
        except Exception as exc:
            self._set_status(f"Erro ao configurar inicio automatico: {exc}", error=True)
            self._startup_var.set(not self._startup_var.get())

    # ── Folder ────────────────────────────────────────────────────────────────
    def _browse_folder(self) -> None:
        current = Path(self._folder_var.get())
        initial = str(current) if current.exists() else str(Path.home())
        chosen = filedialog.askdirectory(
            title="Selecione a pasta de wallpapers", initialdir=initial,
        )
        if chosen:
            self._folder_var.set(chosen)
            self._update_folder_info()

    def _update_folder_info(self) -> None:
        """Kick off a background scan of the wallpaper folder."""
        # Clear treeview immediately for responsiveness
        for item in self._img_tree.get_children():
            self._img_tree.delete(item)

        folder = Path(self._folder_var.get())
        if not folder.exists():
            self._folder_info.configure(text="Pasta nao encontrada.", foreground="#e74c3c")
            return

        self._folder_info.configure(text="Escaneando...", foreground="gray")

        def _scan() -> None:
            from .image_utils import list_images_sorted_by_date
            images = list_images_sorted_by_date(folder)
            # Schedule UI update back on the main thread
            self.after(0, lambda: self._populate_folder_tree(images))

        threading.Thread(target=_scan, daemon=True).start()

    def _populate_folder_tree(self, images: list[Path]) -> None:
        """Populate the folder treeview with scan results (runs on main thread)."""
        for item in self._img_tree.get_children():
            self._img_tree.delete(item)

        count = len(images)
        plural = "s" if count != 1 else ""
        self._folder_info.configure(
            text=f"{count} imagem{plural} encontrada{plural}", foreground="gray",
        )

        for i, img_path in enumerate(images[:100]):
            self._img_tree.insert("", END, values=(f"{i+1:03d}  {img_path.name}",))

        if count > 100:
            self._img_tree.insert("", END, values=(f"... e mais {count - 100} imagens",))

    # ── Monitor Preview ───────────────────────────────────────────────────────
    def _schedule_draw_monitors(self) -> None:
        """Debounce: delay the monitor redraw so rapid Configure events coalesce."""
        if self._draw_after_id is not None:
            self.after_cancel(self._draw_after_id)
        self._draw_after_id = self.after(50, self._draw_monitors)

    def _refresh_monitors(self) -> None:
        try:
            self._monitors = get_monitors()
        except Exception as e:
            self._monitors = []
            self._set_status(f"Erro ao detectar monitores: {e}", error=True)
            return
        n = len(self._monitors)
        plural = "es" if n != 1 else ""
        self._lbl_mon_count.configure(text=f"{n} monitor{plural}")
        self._draw_monitors()

    def _draw_monitors(self) -> None:
        c = self._mon_canvas
        try:
            c.delete("all")
            cw = c.winfo_width() or 720
            ch = c.winfo_height() or 130
            c.create_rectangle(0, 0, cw, ch, fill=_BG_CANVAS, outline="")
        except tk.TclError:
            return

        if not self._monitors:
            c.create_text(
                cw // 2, ch // 2, text="Nenhum monitor detectado",
                fill="#555", font=("Segoe UI", 11),
            )
            return

        min_x = min(m.x for m in self._monitors)
        min_y = min(m.y for m in self._monitors)
        max_x = max(m.x + m.width for m in self._monitors)
        max_y = max(m.y + m.height for m in self._monitors)
        vd_w = max_x - min_x or 1
        vd_h = max_y - min_y or 1
        pad = 14
        scale = min((cw - pad * 2) / vd_w, (ch - pad * 2) / vd_h)
        ox = pad + (cw - pad * 2 - vd_w * scale) / 2
        oy = pad + (ch - pad * 2 - vd_h * scale) / 2

        for m in self._monitors:
            col = _MON_COLORS[m.index % len(_MON_COLORS)]
            x1 = ox + (m.x - min_x) * scale
            y1 = oy + (m.y - min_y) * scale
            x2 = x1 + m.width * scale
            y2 = y1 + m.height * scale

            # Shadow
            c.create_rectangle(x1 + 3, y1 + 3, x2 + 3, y2 + 3, fill="#000", outline="")
            # Background
            c.create_rectangle(x1, y1, x2, y2, fill=col, outline="#666", width=1)
            # Top bar highlight
            ri, gi, bi = int(col[1:3], 16), int(col[3:5], 16), int(col[5:7], 16)
            light = "#{:02x}{:02x}{:02x}".format(
                min(255, ri + 55), min(255, gi + 55), min(255, bi + 55),
            )
            c.create_rectangle(x1, y1, x2, y1 + (y2 - y1) * 0.3, fill=light, outline="")

            fs = max(8, int((x2 - x1) * 0.14))
            cx_m = (x1 + x2) / 2
            cy_m = (y1 + y2) / 2
            c.create_text(
                cx_m, cy_m - fs, text=f"M{m.index + 1}",
                fill="white", font=("Segoe UI", fs, "bold"),
            )
            c.create_text(
                cx_m, cy_m + fs * 0.6, text=f"{m.width}x{m.height}",
                fill="#ccc", font=("Segoe UI", max(7, fs - 2)),
            )

    # ── Config collect ────────────────────────────────────────────────────────
    def _collect_config(self) -> dict:
        try:
            interval = max(1, int(self._interval_var.get() or "300"))
        except ValueError:
            interval = 300

        return {
            "_config_path": self._cfg.get("_config_path", ""),
            "general": {
                "mode": "collage",
                "selection": self._sel_var.get(),
                "interval": interval,
                "collage_count": int(self._collage_count_var.get()),
                "collage_same_for_all": bool(self._collage_same_var.get()),
            },
            "paths": {
                "wallpapers_folder": self._folder_var.get(),
                "output_folder": self._cfg["paths"].get("output_folder", "assets/output"),
            },
            "display": {
                "fit_mode": self._fit_var.get(),
            },
        }

    # ── Actions ───────────────────────────────────────────────────────────────
    def _apply_now(self) -> None:
        if not self._monitors:
            self._set_status("Nenhum monitor. Clique em Detectar.", error=True)
            return
        self._apply_btn.configure(state=DISABLED, text="Aplicando...")
        self._set_status("Aplicando wallpaper...")

        def _work() -> None:
            try:
                cfg = self._collect_config()
                out_dir = resolve_path(cfg["paths"]["output_folder"])
                out_dir.mkdir(parents=True, exist_ok=True)
                out = apply_wallpaper(cfg, self._monitors, out_dir)
                self.after(0, lambda: self._set_status(
                    f"Wallpaper aplicado: {Path(str(out)).name}",
                ))
            except Exception as exc:
                self.after(0, lambda: self._set_status(f"Erro: {exc}", error=True))
            finally:
                self.after(0, lambda: self._apply_btn.configure(
                    state=NORMAL, text="Aplicar Agora",
                ))

        threading.Thread(target=_work, daemon=True).start()

    def _save_config(self) -> None:
        try:
            cfg = self._collect_config()
            save_config(cfg)
            self._cfg = cfg
            self._set_status("Configuracoes salvas.")
        except Exception as exc:
            self._set_status(f"Erro ao salvar: {exc}", error=True)

    def _toggle_watch(self) -> None:
        if self._watching:
            self._watching = False
            schedule.clear()
            self._watch_btn.configure(text="Iniciar Watch", style="info.TButton")
            self._set_status("Watch desativado.")
        else:
            cfg = self._collect_config()
            interval = cfg["general"]["interval"]
            self._watching = True
            self._watch_btn.configure(text="Parar Watch", style="danger.TButton")
            self._set_status(f"Watch ativo — trocando a cada {interval}s.")
            schedule.every(interval).seconds.do(self._apply_now)

            def _loop() -> None:
                while self._watching:
                    schedule.run_pending()
                    time.sleep(1)

            self._watch_thr = threading.Thread(target=_loop, daemon=True)
            self._watch_thr.start()

    def _set_status(self, msg: str, error: bool = False) -> None:
        color = "#e74c3c" if error else "gray"
        self._status_lbl.configure(text=msg, foreground=color)

    # ── System Tray ───────────────────────────────────────────────────────────
    def _setup_tray(self) -> None:
        self.protocol("WM_DELETE_WINDOW", self._minimize_to_tray)
        self.bind("<Unmap>", self._on_iconify)

    def _on_iconify(self, event: tk.Event) -> None:
        if event.widget is self and self.wm_state() == "iconic":
            self._minimize_to_tray()

    @staticmethod
    def _make_tray_image() -> PILImage.Image:
        size = 64
        img = PILImage.new("RGB", (size, size), color="#3a7bd5")
        draw = ImageDraw.Draw(img)
        draw.rectangle([4, 4, 60, 60], fill="#3a7bd5", outline="#ffffff", width=2)
        draw.text((12, 18), "WP", fill="#ffffff")
        return img

    def _minimize_to_tray(self) -> None:
        if self._tray_icon is not None:
            self.withdraw()
            return

        self.withdraw()

        menu = pystray.Menu(
            pystray.MenuItem("Mostrar", lambda: self.after(0, self._show_from_tray), default=True),
            pystray.MenuItem("Aplicar Agora", lambda: self.after(0, self._apply_now)),
            pystray.Menu.SEPARATOR,
            pystray.MenuItem("Sair", lambda: self.after(0, self._quit_app)),
        )

        self._tray_icon = pystray.Icon(
            "WallpaperChanger", self._make_tray_image(), "WallpaperChanger", menu,
        )

        threading.Thread(target=self._tray_icon.run, daemon=True).start()

    def _show_from_tray(self) -> None:
        self.deiconify()
        self.lift()
        self.focus_force()
        if self._tray_icon is not None:
            self._tray_icon.stop()
            self._tray_icon = None

    def _quit_app(self) -> None:
        self._watching = False
        schedule.clear()
        if self._tray_icon is not None:
            self._tray_icon.stop()
            self._tray_icon = None
        self.destroy()


# ── Entry point ───────────────────────────────────────────────────────────────

def run() -> None:
    """Inicia a interface grafica."""
    app = WallpaperChangerApp()
    app.mainloop()


if __name__ == "__main__":
    run()
