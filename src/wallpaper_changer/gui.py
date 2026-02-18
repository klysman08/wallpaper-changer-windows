"""Interface grafica (GUI) do WallpaperChanger - CustomTkinter."""
from __future__ import annotations

import threading
import time
import tkinter as tk
from pathlib import Path
from tkinter import filedialog

import customtkinter as ctk
import schedule

from .config import load_config, save_config, resolve_path
from .monitor import Monitor, get_monitors
from .wallpaper import apply_wallpaper, MODES

# ── Tema ──────────────────────────────────────────────────────────────────────
ctk.set_appearance_mode("dark")
ctk.set_default_color_theme("blue")

# Paleta
_MON_COLORS = ["#3a7bd5", "#e05252", "#3dba5a", "#d4a027", "#9b59b6"]
_BG_CANVAS  = "#12121e"
_ACCENT     = "#3a7bd5"
_TEXT_DIM   = ("gray60", "gray55")

# Dados dos modos de imagem
_MODE_INFO: dict[str, tuple[str, str, str]] = {
    "clone":   ("Duplicar",   "x2",  "Mesma imagem (adaptada) em todos os monitores"),
    "split1":  ("1 Imagem",   "[ ]", "Uma imagem cobre todo o desktop virtual"),
    "split2":  ("2 Imagens",  "[|]", "Imagem diferente nos 2 primeiros monitores"),
    "split3":  ("3 Imagens",  "[3]", "Imagem diferente nos 3 primeiros monitores"),
    "split4":  ("4 Imagens",  "[4]", "Imagem diferente nos 4 primeiros monitores"),
    "quad":    ("4x Monitor", "[#]", "Cada monitor dividido em 4 quadrantes — 4 imagens por tela"),
    "collage": ("Collage",    "[N]", "Grade personalizada: voce escolhe quantas imagens por monitor (2 a 9)"),
}

# Dados dos modos de ajuste
_FIT_INFO: dict[str, tuple[str, str]] = {
    "fill":    ("Preencher",   "Expande para cobrir, corta o excesso"),
    "fit":     ("Ajustar",     "Encaixa sem cortar, adiciona barras pretas"),
    "stretch": ("Ampliar",     "Distorce para preencher exatamente"),
    "center":  ("Centralizar", "Sem redimensionar, centraliza na tela"),
    "span":    ("Estender",    "Imagem distribuida por todo o espaco"),
}

_SEL_LABELS = {"random": "Aleatorio", "sequential": "Sequencial (recente -> antigo)"}

# Cores de card selecionado / padrao
_BTN_ON  = ("#1e3a6e", "#162858")
_BTN_OFF = ("#252535", "#1a1a2a")
_HOV_OFF = ("#303050", "#222240")
_BDR_ON  = _ACCENT
_BDR_OFF = ("#252535", "#1a1a2a")


# ── App principal ─────────────────────────────────────────────────────────────

class WallpaperChangerApp(ctk.CTk):
    def __init__(self) -> None:
        super().__init__()
        self.title("WallpaperChanger")
        self.geometry("760x920")
        self.minsize(680, 820)
        self.resizable(True, True)
        self.grid_columnconfigure(0, weight=1)
        self.grid_rowconfigure(2, weight=1)

        self._cfg = load_config()
        self._monitors: list[Monitor] = []
        self._watching  = False
        self._watch_thr: threading.Thread | None = None

        self._mode_var     = tk.StringVar(value=self._cfg["general"]["mode"])
        self._fit_var      = tk.StringVar(value=self._cfg["display"]["fit_mode"])
        self._sel_var      = tk.StringVar(value=self._cfg["general"].get("selection", "random"))
        self._interval_var = tk.StringVar(value=str(self._cfg["general"]["interval"]))

        self._mode_btns:    dict[str, ctk.CTkButton] = {}
        self._fit_btns:     dict[str, ctk.CTkButton] = {}
        self._collage_btns: dict[int,  ctk.CTkButton] = {}
        self._collage_count_var = tk.IntVar(
            value=self._cfg["general"].get("collage_count", 4))

        self._build_header()
        self._build_monitor_panel()
        self._build_tabs()
        self._build_action_bar()
        self._build_status_bar()

        self._refresh_monitors()
        self.after(200, self._draw_monitors)

    # ── Header ────────────────────────────────────────────────────────────────
    def _build_header(self) -> None:
        hdr = ctk.CTkFrame(self, corner_radius=12, fg_color=("#0f2044", "#09152e"))
        hdr.grid(row=0, column=0, sticky="ew", padx=16, pady=(16, 6))
        hdr.grid_columnconfigure(1, weight=1)

        ctk.CTkLabel(hdr, text="WP", font=ctk.CTkFont(size=24, weight="bold"),
                     text_color=_ACCENT,
                     ).grid(row=0, column=0, rowspan=2, padx=(18, 12), pady=14)

        ctk.CTkLabel(hdr, text="WallpaperChanger",
                     font=ctk.CTkFont(size=21, weight="bold"), anchor="w",
                     ).grid(row=0, column=1, sticky="w", pady=(14, 0))

        ctk.CTkLabel(hdr, text="Painel de controle  |  Windows 11",
                     font=ctk.CTkFont(size=11), text_color=_TEXT_DIM, anchor="w",
                     ).grid(row=1, column=1, sticky="w", pady=(0, 14))

        self._lbl_mon_count = ctk.CTkLabel(
            hdr, text="detectando...", font=ctk.CTkFont(size=11), text_color=_TEXT_DIM)
        self._lbl_mon_count.grid(row=0, column=2, rowspan=2, padx=18)

    # ── Monitor preview ───────────────────────────────────────────────────────
    def _build_monitor_panel(self) -> None:
        frame = ctk.CTkFrame(self, corner_radius=10)
        frame.grid(row=1, column=0, sticky="ew", padx=16, pady=6)
        frame.grid_columnconfigure(0, weight=1)

        bar = ctk.CTkFrame(frame, fg_color="transparent")
        bar.grid(row=0, column=0, sticky="ew", padx=12, pady=(10, 4))
        bar.grid_columnconfigure(0, weight=1)

        ctk.CTkLabel(bar, text="  Disposicao dos Monitores",
                     font=ctk.CTkFont(size=13, weight="bold"), anchor="w",
                     ).grid(row=0, column=0, sticky="w")

        ctk.CTkButton(bar, text="Detectar", width=100, height=26,
                      font=ctk.CTkFont(size=11), corner_radius=6,
                      command=self._refresh_monitors,
                      ).grid(row=0, column=1)

        self._mon_canvas = tk.Canvas(
            frame, height=148, bg=_BG_CANVAS, highlightthickness=0, bd=0)
        self._mon_canvas.grid(row=1, column=0, sticky="ew", padx=12, pady=(0, 12))
        self._mon_canvas.bind("<Configure>", lambda _e: self._draw_monitors())

    # ── Tabs ──────────────────────────────────────────────────────────────────
    def _build_tabs(self) -> None:
        self._tabs = ctk.CTkTabview(self, corner_radius=10)
        self._tabs.grid(row=2, column=0, sticky="nsew", padx=16, pady=6)
        self._tabs.add("  Configurar  ")
        self._tabs.add("  Pasta  ")
        self._tabs.set("  Configurar  ")
        self._build_tab_config()
        self._build_tab_folder()

    # ── Tab: Configurar ───────────────────────────────────────────────────────
    def _build_tab_config(self) -> None:
        tab = self._tabs.tab("  Configurar  ")
        tab.grid_columnconfigure(0, weight=1)
        row = 0

        # Secao: Numero de Imagens ───────────────────────────────────────────
        row = self._section(tab, row, "NUMERO DE IMAGENS NA TELA")

        cards = ctk.CTkFrame(tab, fg_color="transparent")
        cards.grid(row=row, column=0, sticky="ew", padx=12, pady=(6, 4))
        for ci, (key, (title, icon, _desc)) in enumerate(_MODE_INFO.items()):
            cards.grid_columnconfigure(ci, weight=1)
            btn = ctk.CTkButton(
                cards,
                text=f"{icon}\n{title}",
                font=ctk.CTkFont(size=11),
                height=68,
                corner_radius=10,
                fg_color=_BTN_OFF,
                hover_color=_HOV_OFF,
                text_color=_TEXT_DIM[1],
                border_width=2,
                border_color=_BDR_OFF,
                command=lambda k=key: self._select_mode(k),
            )
            btn.grid(row=0, column=ci, padx=4, pady=4, sticky="ew")
            self._mode_btns[key] = btn
        row += 1

        self._mode_desc = ctk.CTkLabel(
            tab, text="", font=ctk.CTkFont(size=11),
            text_color=_TEXT_DIM, anchor="w",
        )
        self._mode_desc.grid(row=row, column=0, sticky="w", padx=16, pady=(0, 6))
        row += 1

        # Picker de quantidade do collage (visivel apenas quando mode == "collage")
        self._collage_row = ctk.CTkFrame(tab, fg_color=("gray85", "#1e1e30"), corner_radius=8)
        self._collage_row.grid(row=row, column=0, sticky="ew", padx=12, pady=(0, 8))
        row += 1
        ctk.CTkLabel(self._collage_row, text="Imagens por monitor:",
                     font=ctk.CTkFont(size=12), anchor="w",
                     ).grid(row=0, column=0, padx=(14, 12), pady=10)
        for _i, _n in enumerate(range(2, 10)):
            _btn = ctk.CTkButton(
                self._collage_row,
                text=str(_n),
                font=ctk.CTkFont(size=12, weight="bold"),
                width=36, height=32, corner_radius=7,
                fg_color=_BTN_OFF, hover_color=_HOV_OFF,
                text_color=_TEXT_DIM[1], border_width=2, border_color=_BDR_OFF,
                command=lambda k=_n: self._select_collage_count(k),
            )
            _btn.grid(row=0, column=_i + 1, padx=3, pady=10)
            self._collage_btns[_n] = _btn

        self._select_mode(self._mode_var.get())

        # Secao: Selecao de Imagens ──────────────────────────────────────────
        row = self._section(tab, row, "SELECAO DE IMAGENS")

        sel_frame = ctk.CTkFrame(tab, fg_color="transparent")
        sel_frame.grid(row=row, column=0, sticky="w", padx=12, pady=(6, 14))
        row += 1

        self._sel_seg = ctk.CTkSegmentedButton(
            sel_frame,
            values=list(_SEL_LABELS.values()),
            font=ctk.CTkFont(size=12),
            command=self._on_sel_change,
        )
        self._sel_seg.pack()
        self._sel_seg.set(_SEL_LABELS.get(self._sel_var.get(), "Aleatorio"))

        # Secao: Ajuste na Tela ──────────────────────────────────────────────
        row = self._section(tab, row, "AJUSTE NA TELA")

        fit_frame = ctk.CTkFrame(tab, fg_color="transparent")
        fit_frame.grid(row=row, column=0, sticky="ew", padx=12, pady=(6, 4))
        for ci, (fkey, (flabel, _fdesc)) in enumerate(_FIT_INFO.items()):
            fit_frame.grid_columnconfigure(ci, weight=1)
            btn = ctk.CTkButton(
                fit_frame,
                text=flabel,
                font=ctk.CTkFont(size=11),
                height=34,
                corner_radius=8,
                fg_color=_BTN_OFF,
                hover_color=_HOV_OFF,
                text_color=_TEXT_DIM[1],
                border_width=2,
                border_color=_BDR_OFF,
                command=lambda k=fkey: self._select_fit(k),
            )
            btn.grid(row=0, column=ci, padx=3, pady=4, sticky="ew")
            self._fit_btns[fkey] = btn
        row += 1

        self._fit_desc = ctk.CTkLabel(
            tab, text="", font=ctk.CTkFont(size=11),
            text_color=_TEXT_DIM, anchor="w",
        )
        self._fit_desc.grid(row=row, column=0, sticky="w", padx=16, pady=(0, 6))
        row += 1

        self._select_fit(self._fit_var.get())

        # Secao: Rotacao Automatica ──────────────────────────────────────────
        row = self._section(tab, row, "ROTACAO AUTOMATICA")

        int_frame = ctk.CTkFrame(tab, fg_color="transparent")
        int_frame.grid(row=row, column=0, sticky="w", padx=12, pady=(6, 16))
        ctk.CTkLabel(int_frame, text="Intervalo:", font=ctk.CTkFont(size=12)
                     ).pack(side="left", padx=(0, 8))
        ctk.CTkEntry(int_frame, textvariable=self._interval_var,
                     width=76, font=ctk.CTkFont(size=12), justify="center"
                     ).pack(side="left")
        ctk.CTkLabel(int_frame, text=" segundos", font=ctk.CTkFont(size=12)
                     ).pack(side="left")

    def _section(self, parent, row: int, text: str) -> int:
        """Insere divisor + rotulo de secao. Retorna proximo row disponivel."""
        ctk.CTkFrame(parent, height=1, fg_color=("gray72", "gray25")
                     ).grid(row=row, column=0, sticky="ew", padx=8, pady=(18, 0))
        ctk.CTkLabel(parent, text=text,
                     font=ctk.CTkFont(size=10, weight="bold"),
                     text_color=_TEXT_DIM, anchor="w",
                     ).grid(row=row, column=0, sticky="w", padx=14, pady=(9, 0))
        return row + 1

    def _select_mode(self, key: str) -> None:
        self._mode_var.set(key)
        for k, btn in self._mode_btns.items():
            on = k == key
            btn.configure(
                fg_color=_BTN_ON if on else _BTN_OFF,
                text_color="white" if on else _TEXT_DIM[1],
                border_color=_BDR_ON if on else _BDR_OFF,
            )
        self._mode_desc.configure(
            text="    " + _MODE_INFO.get(key, ("", "", ""))[2])
        # Mostra/oculta o picker de quantidade do collage
        if key == "collage":
            self._collage_row.grid()
            self._select_collage_count(self._collage_count_var.get())
        else:
            self._collage_row.grid_remove()

    def _select_collage_count(self, n: int) -> None:
        self._collage_count_var.set(n)
        for k, btn in self._collage_btns.items():
            on = k == n
            btn.configure(
                fg_color=_BTN_ON if on else _BTN_OFF,
                text_color="white" if on else _TEXT_DIM[1],
                border_color=_BDR_ON if on else _BDR_OFF,
            )

    def _select_fit(self, key: str) -> None:
        self._fit_var.set(key)
        for k, btn in self._fit_btns.items():
            on = k == key
            btn.configure(
                fg_color=_BTN_ON if on else _BTN_OFF,
                text_color="white" if on else _TEXT_DIM[1],
                border_color=_BDR_ON if on else _BDR_OFF,
            )
        self._fit_desc.configure(
            text="    " + _FIT_INFO.get(key, ("", ""))[1])

    def _on_sel_change(self, label: str) -> None:
        inv = {v: k for k, v in _SEL_LABELS.items()}
        self._sel_var.set(inv.get(label, "random"))

    # ── Tab: Pasta ────────────────────────────────────────────────────────────
    def _build_tab_folder(self) -> None:
        tab = self._tabs.tab("  Pasta  ")
        tab.grid_columnconfigure(0, weight=1)

        ctk.CTkLabel(tab, text="Pasta de wallpapers",
                     font=ctk.CTkFont(size=13, weight="bold"), anchor="w",
                     ).grid(row=0, column=0, sticky="w", padx=16, pady=(18, 4))

        ctk.CTkLabel(
            tab,
            text="Todas as imagens serao buscadas nesta pasta.\n"
                 "Formatos suportados: jpg  jpeg  png  bmp  webp",
            font=ctk.CTkFont(size=11), text_color=_TEXT_DIM,
            anchor="w", justify="left",
        ).grid(row=1, column=0, sticky="w", padx=16, pady=(0, 10))

        picker = ctk.CTkFrame(tab, fg_color="transparent")
        picker.grid(row=2, column=0, sticky="ew", padx=12, pady=4)
        picker.grid_columnconfigure(0, weight=1)

        raw = self._cfg["paths"]["wallpapers_folder"]
        self._folder_var = ctk.StringVar(value=str(resolve_path(raw)))

        entry = ctk.CTkEntry(picker, textvariable=self._folder_var,
                             font=ctk.CTkFont(size=12), height=36)
        entry.grid(row=0, column=0, sticky="ew", padx=(0, 6))
        entry.bind("<FocusOut>", lambda _e: self._update_folder_info())

        ctk.CTkButton(
            picker, text="...", width=44, height=36,
            font=ctk.CTkFont(size=13),
            fg_color=("gray62", "gray30"), hover_color=("gray52", "gray22"),
            corner_radius=8, command=self._browse_folder,
        ).grid(row=0, column=1)

        self._folder_info = ctk.CTkLabel(
            tab, text="", font=ctk.CTkFont(size=11),
            text_color=_TEXT_DIM, anchor="w", justify="left",
        )
        self._folder_info.grid(row=3, column=0, sticky="w", padx=16, pady=(8, 0))

        ctk.CTkLabel(tab, text="Imagens encontradas:",
                     font=ctk.CTkFont(size=12, weight="bold"), anchor="w",
                     ).grid(row=4, column=0, sticky="w", padx=16, pady=(14, 4))

        self._img_list = ctk.CTkScrollableFrame(tab, height=140, corner_radius=8)
        self._img_list.grid(row=5, column=0, sticky="ew", padx=12, pady=(0, 16))
        self._img_list.grid_columnconfigure(0, weight=1)

        self._update_folder_info()

    def _browse_folder(self) -> None:
        current = Path(self._folder_var.get())
        initial = str(current) if current.exists() else str(Path.home())
        chosen  = filedialog.askdirectory(title="Selecione a pasta de wallpapers",
                                          initialdir=initial)
        if chosen:
            self._folder_var.set(chosen)
            self._update_folder_info()

    def _update_folder_info(self) -> None:
        from .image_utils import list_images_sorted_by_date

        for w in self._img_list.winfo_children():
            w.destroy()

        folder = Path(self._folder_var.get())
        if not folder.exists():
            self._folder_info.configure(
                text="Pasta nao encontrada.",
                text_color=("#c0392b", "#e74c3c"))
            return

        images = list_images_sorted_by_date(folder)
        count  = len(images)
        self._folder_info.configure(
            text=f"{count} imagem{'ns' if count != 1 else ''} encontrada{'s' if count != 1 else ''}",
            text_color=_TEXT_DIM,
        )

        for i, img_path in enumerate(images[:80]):
            rf = ctk.CTkFrame(self._img_list, fg_color="transparent")
            rf.grid(row=i, column=0, sticky="ew", pady=1)
            rf.grid_columnconfigure(1, weight=1)
            ctk.CTkLabel(rf, text=f"{i + 1:03d}",
                         font=ctk.CTkFont(size=10), text_color=_TEXT_DIM,
                         width=28, anchor="e",
                         ).grid(row=0, column=0, padx=(4, 6))
            ctk.CTkLabel(rf, text=img_path.name,
                         font=ctk.CTkFont(size=11), anchor="w",
                         ).grid(row=0, column=1, sticky="w")

        if count > 80:
            ctk.CTkLabel(self._img_list,
                         text=f"... e mais {count - 80} imagens",
                         font=ctk.CTkFont(size=10), text_color=_TEXT_DIM,
                         ).grid(row=80, column=0, sticky="w", pady=(4, 0))

    # ── Monitor preview draw ──────────────────────────────────────────────────
    def _refresh_monitors(self) -> None:
        try:
            self._monitors = get_monitors()
        except Exception as e:
            self._monitors = []
            self._set_status(f"Erro ao detectar monitores: {e}", error=True)
            return
        n = len(self._monitors)
        self._lbl_mon_count.configure(
            text=f"{n} monitor{'es' if n != 1 else ''} detectado{'s' if n != 1 else ''}")
        self._draw_monitors()

    def _draw_monitors(self) -> None:
        c = self._mon_canvas
        try:
            c.delete("all")
            cw = c.winfo_width()  or 720
            ch = c.winfo_height() or 148
            c.create_rectangle(0, 0, cw, ch, fill=_BG_CANVAS, outline="")
        except tk.TclError:
            return

        if not self._monitors:
            c.create_text(cw // 2, ch // 2, text="Nenhum monitor detectado",
                          fill="#555", font=("Segoe UI", 12))
            return

        min_x = min(m.x for m in self._monitors)
        min_y = min(m.y for m in self._monitors)
        max_x = max(m.x + m.width  for m in self._monitors)
        max_y = max(m.y + m.height for m in self._monitors)
        vd_w  = max_x - min_x or 1
        vd_h  = max_y - min_y or 1
        pad   = 16
        scale = min((cw - pad * 2) / vd_w, (ch - pad * 2) / vd_h)
        ox    = pad + (cw - pad * 2 - vd_w * scale) / 2
        oy    = pad + (ch - pad * 2 - vd_h * scale) / 2

        for m in self._monitors:
            col = _MON_COLORS[m.index % len(_MON_COLORS)]
            x1  = ox + (m.x - min_x) * scale
            y1  = oy + (m.y - min_y) * scale
            x2  = x1 + m.width  * scale
            y2  = y1 + m.height * scale
            ri, gi, bi = int(col[1:3], 16), int(col[3:5], 16), int(col[5:7], 16)
            light = "#{:02x}{:02x}{:02x}".format(
                min(255, ri + 55), min(255, gi + 55), min(255, bi + 55))

            c.create_rectangle(x1 + 3, y1 + 3, x2 + 3, y2 + 3,
                               fill="#000000", outline="")
            c.create_rectangle(x1, y1, x2, y2,
                               fill=col, outline="#888888", width=1)
            c.create_rectangle(x1, y1, x2, y1 + (y2 - y1) * 0.35,
                               fill=light, outline="")

            fs   = max(8, int((x2 - x1) * 0.14))
            cx_m = (x1 + x2) / 2
            cy_m = (y1 + y2) / 2
            c.create_text(cx_m, cy_m - fs, text=f"M{m.index + 1}",
                          fill="white", font=("Segoe UI", fs, "bold"))
            c.create_text(cx_m, cy_m + fs * 0.6, text=f"{m.width}x{m.height}",
                          fill="#cccccc", font=("Segoe UI", max(7, fs - 2)))

    # ── Coleta de config da UI ────────────────────────────────────────────────
    def _collect_config(self) -> dict:
        try:
            interval = max(1, int(self._interval_var.get() or "300"))
        except ValueError:
            interval = 300

        return {
            "_config_path": self._cfg.get("_config_path", ""),
            "general": {
                "mode":          self._mode_var.get(),
                "selection":     self._sel_var.get(),
                "interval":      interval,
                "collage_count": int(self._collage_count_var.get()),
            },
            "paths": {
                "wallpapers_folder": self._folder_var.get(),
                "output_folder":     self._cfg["paths"].get("output_folder", "assets/output"),
            },
            "display": {
                "fit_mode": self._fit_var.get(),
            },
        }

    # ── Barra de acoes ────────────────────────────────────────────────────────
    def _build_action_bar(self) -> None:
        bar = ctk.CTkFrame(self, corner_radius=10)
        bar.grid(row=3, column=0, sticky="ew", padx=16, pady=6)
        bar.grid_columnconfigure((0, 1, 2), weight=1)

        self._apply_btn = ctk.CTkButton(
            bar, text="Aplicar Agora",
            font=ctk.CTkFont(size=13, weight="bold"),
            height=44, corner_radius=8,
            command=self._apply_now,
        )
        self._apply_btn.grid(row=0, column=0, padx=(16, 6), pady=12, sticky="ew")

        ctk.CTkButton(
            bar, text="Salvar Config",
            font=ctk.CTkFont(size=13), height=44, corner_radius=8,
            fg_color=("gray62", "gray26"),
            hover_color=("gray52", "gray20"),
            command=self._save_config,
        ).grid(row=0, column=1, padx=6, pady=12, sticky="ew")

        self._watch_btn = ctk.CTkButton(
            bar, text="Iniciar Watch",
            font=ctk.CTkFont(size=13), height=44, corner_radius=8,
            fg_color=("#1a5830", "#144020"),
            hover_color=("#14502a", "#0e3018"),
            command=self._toggle_watch,
        )
        self._watch_btn.grid(row=0, column=2, padx=(6, 16), pady=12, sticky="ew")

    # ── Barra de status ───────────────────────────────────────────────────────
    def _build_status_bar(self) -> None:
        bar = ctk.CTkFrame(self, height=30, corner_radius=0,
                           fg_color=("gray85", "gray17"))
        bar.grid(row=4, column=0, sticky="ew")
        bar.grid_columnconfigure(0, weight=1)
        self._status_lbl = ctk.CTkLabel(
            bar, text="  Pronto.",
            font=ctk.CTkFont(size=11), text_color=_TEXT_DIM, anchor="w",
        )
        self._status_lbl.grid(row=0, column=0, sticky="w", padx=12)

    # ── Acoes ─────────────────────────────────────────────────────────────────
    def _apply_now(self) -> None:
        if not self._monitors:
            self._set_status("Nenhum monitor. Clique em Detectar.", error=True)
            return
        self._apply_btn.configure(state="disabled", text="Aplicando...")
        self._set_status("Aplicando wallpaper...")

        def _work() -> None:
            try:
                cfg     = self._collect_config()
                out_dir = resolve_path(cfg["paths"]["output_folder"])
                out_dir.mkdir(parents=True, exist_ok=True)
                out = apply_wallpaper(cfg, self._monitors, out_dir)
                self.after(0, lambda: self._set_status(
                    f"Wallpaper aplicado: {Path(str(out)).name}"))
            except Exception as exc:
                self.after(0, lambda: self._set_status(f"Erro: {exc}", error=True))
            finally:
                self.after(0, lambda: self._apply_btn.configure(
                    state="normal", text="Aplicar Agora"))

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
            self._watch_btn.configure(
                text="Iniciar Watch",
                fg_color=("#1a5830", "#144020"),
                hover_color=("#14502a", "#0e3018"),
            )
            self._set_status("Watch desativado.")
        else:
            cfg      = self._collect_config()
            interval = cfg["general"]["interval"]
            self._watching = True
            self._watch_btn.configure(
                text="Parar Watch",
                fg_color=("#7a1a1a", "#4e0e0e"),
                hover_color=("#641414", "#3c0a0a"),
            )
            self._set_status(f"Watch ativo - trocando a cada {interval}s.")
            schedule.every(interval).seconds.do(self._apply_now)

            def _loop() -> None:
                while self._watching:
                    schedule.run_pending()
                    time.sleep(1)

            self._watch_thr = threading.Thread(target=_loop, daemon=True)
            self._watch_thr.start()

    def _set_status(self, msg: str, error: bool = False) -> None:
        color = ("#c0392b", "#e74c3c") if error else _TEXT_DIM
        self._status_lbl.configure(text=f"  {msg}", text_color=color)


# ── Entry point ───────────────────────────────────────────────────────────────
def run() -> None:
    """Inicia a interface grafica."""
    app = WallpaperChangerApp()
    app.mainloop()


if __name__ == "__main__":
    run()
