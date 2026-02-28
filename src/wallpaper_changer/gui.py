"""Interface grafica (GUI) do WallpaperChanger - ttkbootstrap."""
from __future__ import annotations

import ctypes
import threading
import time
import tkinter as tk
from pathlib import Path
from tkinter import filedialog, messagebox

import pystray
import schedule
import ttkbootstrap as ttk
from PIL import Image as PILImage, ImageDraw
from ttkbootstrap.constants import *

from .config import load_config, save_config, resolve_path
from .hotkeys import HotkeyManager, read_hotkey, is_available as hotkeys_available
from .i18n import t, set_language, get_language, SUPPORTED_LANGUAGES
from .monitor import Monitor, get_monitors
from .startup import is_startup_enabled, is_startup_launch, set_startup_enabled
from .wallpaper import apply_wallpaper, apply_single_wallpaper
from .transparency import (
    get_foreground_window,
    list_visible_windows,
    load_opacity_settings,
    reapply_saved_settings,
    save_opacity_settings,
    set_window_opacity,
)

# ── Paleta ────────────────────────────────────────────────────────────────────
_MON_COLORS = ["#3a7bd5", "#e05252", "#3dba5a", "#d4a027", "#9b59b6"]
_BG_CANVAS  = "#1a1a2e"
_ACCENT     = "#3a7bd5"

# Fit mode keys — labels are resolved via i18n at build time
_FIT_KEYS = ["fill", "fit", "stretch", "center", "span"]


def _fit_label(key: str) -> str:
    return t(f"fit_{key}")


def _fit_desc(key: str) -> str:
    return t(f"fit_{key}_desc")


def _sel_labels() -> dict[str, str]:
    return {"random": t("sel_random"), "sequential": t("sel_sequential")}


# ── Single Instance ───────────────────────────────────────────────────────────
_single_instance_mutex = None


def _acquire_single_instance() -> bool:
    """Try to acquire a system-wide named mutex.

    Returns True if this is the only running instance.
    """
    global _single_instance_mutex
    _single_instance_mutex = ctypes.windll.kernel32.CreateMutexW(
        None, False, "WallpaperChangerSingleInstance",
    )
    return ctypes.windll.kernel32.GetLastError() != 183  # ERROR_ALREADY_EXISTS


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

        # ── Initialise i18n from saved config ─────────────────────────────────
        saved_lang = self._cfg["general"].get("language", "en")
        set_language(saved_lang)

        self._monitors: list[Monitor] = []
        self._watching = False
        self._watch_thr: threading.Thread | None = None
        self._tray_icon: pystray.Icon | None = None
        self._startup_launch = is_startup_launch()

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
        self._lang_var = tk.StringVar(value=saved_lang)

        self._fit_btns: dict[str, ttk.Button] = {}
        self._collage_btns: dict[int, ttk.Button] = {}
        self._draw_after_id: str | None = None  # debounce for monitor redraw

        # ── Hotkey variables ──────────────────────────────────────────────────
        hk = self._cfg.get("hotkeys", {})
        self._hk_next_var = tk.StringVar(value=hk.get("next_wallpaper", "ctrl+alt+right"))
        self._hk_prev_var = tk.StringVar(value=hk.get("prev_wallpaper", "ctrl+alt+left"))
        self._hk_stop_var = tk.StringVar(value=hk.get("stop_watch", "ctrl+alt+s"))
        self._hk_default_var = tk.StringVar(value=hk.get("default_wallpaper", "ctrl+alt+d"))
        self._hk_transp_var = tk.StringVar(value=hk.get("toggle_transparency", "alt+a"))
        self._default_wp_var = tk.StringVar(
            value=self._cfg.get("paths", {}).get("default_wallpaper", ""),
        )

        # ── Wallpaper history (in-session) ────────────────────────────────────
        self._wp_history: list[list[str]] = []
        self._wp_hist_idx: int = -1

        # ── Hotkey manager ────────────────────────────────────────────────────
        self._hk_manager = HotkeyManager()

        # ── Transparency state ────────────────────────────────────────────────
        self._transp_windows: list[tuple[int, str]] = []
        self._opacity_map: dict[str, int] = load_opacity_settings()
        self._pynput_mouse_listener = None
        self._pynput_kb_listener = None
        self._alt_pressed = False

        # ── Construcao da UI ──────────────────────────────────────────────────
        self._build_ui()
        self._setup_tray()
        self._refresh_monitors()
        self.after(200, self._draw_monitors)
        self._register_hotkeys()
        self._start_transparency_listeners()

        # ── Restore saved transparency on startup ─────────────────────────────
        restored = reapply_saved_settings()
        if restored:
            self.after(500, lambda: self._set_status(
                t("transp_restored", n=restored),
            ))

        # ── Startup-to-tray: minimise + auto-watch ───────────────────────────
        if self._startup_launch:
            self.after(300, self._startup_to_tray)

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
        self._build_hotkeys_section(main)
        self._build_transparency_section(main)
        self._build_default_wp_section(main)
        self._build_folder_section(main)
        self._build_language_section(main)
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
            hdr, text=t("header_subtitle"),
            font=("Segoe UI", 10), foreground="gray", anchor=W,
        ).grid(row=1, column=1, sticky=W)

        self._lbl_mon_count = ttk.Label(
            hdr, text=t("detecting"), font=("Segoe UI", 10), foreground="gray",
        )
        self._lbl_mon_count.grid(row=0, column=2, rowspan=2, padx=(12, 0))

    # ── Monitor Preview ───────────────────────────────────────────────────────
    def _build_monitor_panel(self, parent: ttk.Frame) -> None:
        frame = ttk.Labelframe(parent, text=t("monitors"), padding=8)
        frame.grid(row=1, column=0, sticky=EW, padx=12, pady=4)
        frame.columnconfigure(0, weight=1)

        bar = ttk.Frame(frame)
        bar.grid(row=0, column=0, sticky=EW)
        bar.columnconfigure(0, weight=1)

        ttk.Button(
            bar, text=t("detect"), style="Outline.TButton",
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
        frame = ttk.Labelframe(parent, text=t("selection_title"), padding=10)
        frame.grid(row=3, column=0, sticky=EW, padx=12, pady=4)

        btn_row = ttk.Frame(frame)
        btn_row.grid(row=0, column=0, sticky=W)

        self._sel_btns: dict[str, ttk.Radiobutton] = {}
        for key, label in _sel_labels().items():
            rb = ttk.Radiobutton(
                btn_row, text=label, variable=self._sel_var, value=key,
                style="Toolbutton",
            )
            rb.pack(side=LEFT, padx=(0, 8))
            self._sel_btns[key] = rb

    # ── Fit Mode ──────────────────────────────────────────────────────────────
    def _build_fit_section(self, parent: ttk.Frame) -> None:
        frame = ttk.Labelframe(parent, text=t("fit_title"), padding=10)
        frame.grid(row=4, column=0, sticky=EW, padx=12, pady=4)
        frame.columnconfigure(tuple(range(len(_FIT_KEYS))), weight=1)

        for ci, fkey in enumerate(_FIT_KEYS):
            btn = ttk.Button(
                frame, text=_fit_label(fkey), style="Outline.TButton",
                command=lambda k=fkey: self._select_fit(k),
            )
            btn.grid(row=0, column=ci, padx=2, pady=2, sticky=EW)
            self._fit_btns[fkey] = btn

        self._fit_desc = ttk.Label(
            frame, text="", font=("Segoe UI", 9), foreground="gray",
        )
        self._fit_desc.grid(row=1, column=0, columnspan=len(_FIT_KEYS), sticky=W, pady=(6, 0))

        self._select_fit(self._fit_var.get())

    # ── Rotation / Timer ──────────────────────────────────────────────────────
    def _build_rotation_section(self, parent: ttk.Frame) -> None:
        frame = ttk.Labelframe(parent, text=t("rotation_title"), padding=10)
        frame.grid(row=5, column=0, sticky=EW, padx=12, pady=4)
        frame.columnconfigure(0, weight=1)

        row1 = ttk.Frame(frame)
        row1.grid(row=0, column=0, sticky=W)

        ttk.Label(row1, text=t("interval_label")).pack(side=LEFT, padx=(0, 6))
        ttk.Entry(
            row1, textvariable=self._interval_var, width=8, justify=CENTER,
        ).pack(side=LEFT)
        ttk.Label(row1, text=" " + t("seconds")).pack(side=LEFT)

        # Startup option
        ttk.Checkbutton(
            frame, text=t("start_with_windows"),
            variable=self._startup_var,
            command=self._on_startup_toggle,
            style="Roundtoggle.Toolbutton",
        ).grid(row=1, column=0, sticky=W, pady=(8, 0))

    # ── Hotkeys Section ───────────────────────────────────────────────────────
    def _build_hotkeys_section(self, parent: ttk.Frame) -> None:
        frame = ttk.Labelframe(parent, text=t("hotkeys_title"), padding=10)
        frame.grid(row=6, column=0, sticky=EW, padx=12, pady=4)
        frame.columnconfigure(1, weight=1)

        labels = [
            (t("hk_next"), self._hk_next_var),
            (t("hk_prev"), self._hk_prev_var),
            (t("hk_stop"), self._hk_stop_var),
            (t("hk_default"), self._hk_default_var),
            (t("hk_transp"), self._hk_transp_var),
        ]

        self._hk_record_btns: list[ttk.Button] = []
        for i, (text, var) in enumerate(labels):
            ttk.Label(frame, text=text).grid(
                row=i, column=0, sticky=W, padx=(0, 8), pady=2,
            )
            entry = ttk.Entry(frame, textvariable=var, width=24)
            entry.grid(row=i, column=1, sticky=EW, padx=(0, 4), pady=2)
            btn = ttk.Button(
                frame, text=t("hk_record"), width=8, style="Outline.TButton",
                command=lambda v=var, b_idx=i: self._record_hotkey(v, b_idx),
            )
            btn.grid(row=i, column=2, pady=2)
            self._hk_record_btns.append(btn)

        if not hotkeys_available():
            ttk.Label(
                frame,
                text=t("hk_disabled_warning"),
                font=("Segoe UI", 9), foreground="#e74c3c",
            ).grid(row=len(labels), column=0, columnspan=3, sticky=W, pady=(6, 0))

    # ── Default Wallpaper Section ─────────────────────────────────────────────
    # ── Transparency Section ────────────────────────────────────────────────
    def _build_transparency_section(self, parent: ttk.Frame) -> None:
        frame = ttk.Labelframe(parent, text=t("transp_title"), padding=10)
        frame.grid(row=7, column=0, sticky=EW, padx=12, pady=4)
        frame.columnconfigure(0, weight=1)

        # ── Window ComboBox + Refresh ─────────────────────────────────────
        sel_row = ttk.Frame(frame)
        sel_row.grid(row=0, column=0, sticky=EW, pady=(0, 6))
        sel_row.columnconfigure(0, weight=1)

        self._transp_combo_var = tk.StringVar()
        self._transp_combo = ttk.Combobox(
            sel_row,
            textvariable=self._transp_combo_var,
            state="readonly",
            font=("Segoe UI", 10),
        )
        self._transp_combo.grid(row=0, column=0, sticky=EW, padx=(0, 8))
        self._transp_combo.bind("<<ComboboxSelected>>", self._on_transp_window_selected)

        ttk.Button(
            sel_row, text=t("transp_refresh"), style="Outline.TButton",
            command=self._refresh_transp_list, width=10,
        ).grid(row=0, column=1)

        # ── Opacity Slider ────────────────────────────────────────────────
        slider_row = ttk.Frame(frame)
        slider_row.grid(row=1, column=0, sticky=EW, pady=(0, 6))
        slider_row.columnconfigure(0, weight=1)

        self._transp_opacity_var = tk.IntVar(value=255)
        self._transp_slider = ttk.Scale(
            slider_row,
            from_=50,
            to=255,
            variable=self._transp_opacity_var,
            orient="horizontal",
            command=self._on_transp_slider_change,
            style="info.Horizontal.TScale",
        )
        self._transp_slider.grid(row=0, column=0, sticky=EW, padx=(0, 8))

        self._transp_opacity_label = ttk.Label(
            slider_row, text="255", font=("Segoe UI", 11, "bold"),
            width=4, anchor="center",
        )
        self._transp_opacity_label.grid(row=0, column=1)

        # ── Shortcut hints ────────────────────────────────────────────────
        ttk.Label(
            frame, text=t("transp_shortcut_info"),
            font=("Segoe UI", 9), foreground="gray",
        ).grid(row=2, column=0, sticky=W)

        # Populate on build
        self.after(300, self._refresh_transp_list)

    # ── Transparency helpers ──────────────────────────────────────────────────

    def _refresh_transp_list(self) -> None:
        self._transp_windows = list_visible_windows()
        titles = [title for _, title in self._transp_windows]
        self._transp_combo["values"] = titles
        if titles:
            self._transp_combo.current(0)
            self._on_transp_window_selected()

    def _transp_selected_hwnd(self) -> int | None:
        idx = self._transp_combo.current()
        if idx < 0 or idx >= len(self._transp_windows):
            return None
        return self._transp_windows[idx][0]

    def _transp_selected_title(self) -> str | None:
        idx = self._transp_combo.current()
        if idx < 0 or idx >= len(self._transp_windows):
            return None
        return self._transp_windows[idx][1]

    def _on_transp_window_selected(self, _event=None) -> None:
        title = self._transp_selected_title()
        if title is None:
            return
        alpha = self._opacity_map.get(title, 255)
        self._transp_opacity_var.set(alpha)
        self._transp_opacity_label.configure(text=str(alpha))

    def _on_transp_slider_change(self, value: str) -> None:
        alpha = int(float(value))
        self._transp_opacity_label.configure(text=str(alpha))
        hwnd = self._transp_selected_hwnd()
        title = self._transp_selected_title()
        if hwnd is None or title is None:
            return
        self._opacity_map[title] = alpha
        set_window_opacity(hwnd, alpha)

    def _save_transparency_settings(self) -> None:
        """Persist the current opacity map to disk."""
        # Only save entries that are not fully opaque
        to_save = {t: a for t, a in self._opacity_map.items() if a < 255}
        save_opacity_settings(to_save)

    # ── Transparency global shortcuts ─────────────────────────────────────────

    def _hotkey_half_opacity(self) -> None:
        """Toggle focused window between 50% opacity and fully opaque."""
        hwnd = get_foreground_window()
        if not hwnd:
            return
        from .transparency import _get_window_title
        title = _get_window_title(hwnd)
        if not title:
            return
        current = self._opacity_map.get(title, 255)
        # Toggle: if already semi-transparent, restore to opaque; otherwise set 50%
        new_alpha = 255 if current < 255 else 128
        self._opacity_map[title] = new_alpha
        set_window_opacity(hwnd, new_alpha)
        self.after(0, lambda: self._set_status(t("transp_applied", alpha=new_alpha)))
        self.after(0, self._sync_transp_slider_if_match, hwnd)

    def _start_transparency_listeners(self) -> None:
        """Start the pynput mouse/keyboard listener for Alt+Scroll."""
        threading.Thread(
            target=self._run_pynput_listeners, daemon=True,
        ).start()

    def _run_pynput_listeners(self) -> None:
        try:
            from pynput import mouse, keyboard as pynput_kb

            def on_press(key):
                try:
                    if key in (pynput_kb.Key.alt_l, pynput_kb.Key.alt_r):
                        self._alt_pressed = True
                except Exception:
                    pass

            def on_release(key):
                try:
                    if key in (pynput_kb.Key.alt_l, pynput_kb.Key.alt_r):
                        self._alt_pressed = False
                except Exception:
                    pass

            def on_scroll(_x, _y, _dx, dy):
                if not self._alt_pressed:
                    return
                hwnd = get_foreground_window()
                if not hwnd:
                    return
                from .transparency import _get_window_title
                title = _get_window_title(hwnd)
                if not title:
                    return
                current = self._opacity_map.get(title, 255)
                new_alpha = max(10, min(255, current + int(dy) * 5))
                self._opacity_map[title] = new_alpha
                set_window_opacity(hwnd, new_alpha)
                self.after(0, lambda a=new_alpha: self._set_status(
                    t("transp_applied", alpha=a),
                ))
                self.after(0, self._sync_transp_slider_if_match, hwnd)

            self._pynput_kb_listener = pynput_kb.Listener(
                on_press=on_press, on_release=on_release,
            )
            self._pynput_mouse_listener = mouse.Listener(on_scroll=on_scroll)
            self._pynput_kb_listener.start()
            self._pynput_mouse_listener.start()
            self._pynput_kb_listener.join()
            self._pynput_mouse_listener.join()
        except ImportError:
            pass
        except Exception:
            pass

    def _sync_transp_slider_if_match(self, hwnd: int) -> None:
        """If the given hwnd matches the combo selection, update the slider."""
        selected = self._transp_selected_hwnd()
        if selected == hwnd:
            title = self._transp_selected_title()
            if title:
                alpha = self._opacity_map.get(title, 255)
                self._transp_opacity_var.set(alpha)
                self._transp_opacity_label.configure(text=str(alpha))

    def _build_default_wp_section(self, parent: ttk.Frame) -> None:
        frame = ttk.Labelframe(parent, text=t("default_wp_title"), padding=10)
        frame.grid(row=8, column=0, sticky=EW, padx=12, pady=4)
        frame.columnconfigure(0, weight=1)

        ttk.Label(
            frame,
            text=t("default_wp_desc"),
            font=("Segoe UI", 9), foreground="gray",
        ).grid(row=0, column=0, columnspan=2, sticky=W, pady=(0, 6))

        entry = ttk.Entry(frame, textvariable=self._default_wp_var)
        entry.grid(row=1, column=0, sticky=EW, padx=(0, 6))

        ttk.Button(
            frame, text="...", width=4, style="Outline.TButton",
            command=self._browse_default_wp,
        ).grid(row=1, column=1)

    # ── Folder Section ────────────────────────────────────────────────────────
    def _build_folder_section(self, parent: ttk.Frame) -> None:
        frame = ttk.Labelframe(parent, text=t("folder_title"), padding=10)
        frame.grid(row=9, column=0, sticky=EW, padx=12, pady=4)
        frame.columnconfigure(0, weight=1)

        ttk.Label(
            frame,
            text=t("folder_formats"),
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
        self._img_tree.heading("name", text=t("images_found_header"), anchor=W)
        self._img_tree.column("name", anchor=W)
        self._img_tree.grid(row=3, column=0, columnspan=2, sticky=EW, pady=(6, 0))

        tree_scroll = ttk.Scrollbar(frame, orient=VERTICAL, command=self._img_tree.yview)
        tree_scroll.grid(row=3, column=2, sticky="ns", pady=(6, 0))
        self._img_tree.configure(yscrollcommand=tree_scroll.set)

        self._update_folder_info()

    # ── Language Section ──────────────────────────────────────────────────────
    def _build_language_section(self, parent: ttk.Frame) -> None:
        frame = ttk.Labelframe(parent, text=t("language_title"), padding=10)
        frame.grid(row=10, column=0, sticky=EW, padx=12, pady=4)
        frame.columnconfigure(1, weight=1)

        btn_row = ttk.Frame(frame)
        btn_row.grid(row=0, column=0, columnspan=2, sticky=W)

        self._lang_btns: dict[str, ttk.Radiobutton] = {}
        for code, label in SUPPORTED_LANGUAGES.items():
            rb = ttk.Radiobutton(
                btn_row, text=label, variable=self._lang_var, value=code,
                style="Toolbutton",
                command=self._on_language_change,
            )
            rb.pack(side=LEFT, padx=(0, 8))
            self._lang_btns[code] = rb

        self._lang_note = ttk.Label(
            frame, text="", font=("Segoe UI", 9), foreground="gray",
        )
        self._lang_note.grid(row=1, column=0, columnspan=2, sticky=W, pady=(6, 0))

    def _on_language_change(self) -> None:
        """Save the new language preference and notify user about restart."""
        new_lang = self._lang_var.get()
        set_language(new_lang)
        # Persist immediately
        try:
            cfg = self._collect_config()
            save_config(cfg)
            self._cfg = cfg
        except Exception:
            pass
        self._lang_note.configure(text=t("language_restart_note"))

    # ── Action Bar ────────────────────────────────────────────────────────────
    def _build_action_bar(self, parent: ttk.Frame) -> None:
        bar = ttk.Frame(parent, padding=(12, 8))
        bar.grid(row=11, column=0, sticky=EW, padx=12, pady=(8, 4))
        bar.columnconfigure((0, 1, 2), weight=1)

        self._apply_btn = ttk.Button(
            bar, text=t("apply_now"), style="success.TButton",
            command=self._apply_now,
        )
        self._apply_btn.grid(row=0, column=0, padx=4, sticky=EW)

        ttk.Button(
            bar, text=t("save_config"), style="secondary.TButton",
            command=self._save_config,
        ).grid(row=0, column=1, padx=4, sticky=EW)

        self._watch_btn = ttk.Button(
            bar, text=t("start_watch"), style="info.TButton",
            command=self._toggle_watch,
        )
        self._watch_btn.grid(row=0, column=2, padx=4, sticky=EW)

        ttk.Button(
            bar, text=t("tray_btn"), style="dark.TButton",
            command=self._minimize_to_tray, width=10,
        ).grid(row=0, column=3, padx=(4, 0))

    # ── Status Bar ────────────────────────────────────────────────────────────
    def _build_status_bar(self) -> None:
        bar = ttk.Frame(self, padding=(12, 4))
        bar.pack(side=BOTTOM, fill=X)

        self._status_lbl = ttk.Label(
            bar, text=t("ready"), font=("Segoe UI", 9), foreground="gray",
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
        desc = _fit_desc(key)
        self._fit_desc.configure(text=desc)

    def _on_startup_toggle(self) -> None:
        try:
            set_startup_enabled(self._startup_var.get())
            if self._startup_var.get():
                self._set_status(t("startup_enabled"))
            else:
                self._set_status(t("startup_disabled"))
        except Exception as exc:
            self._set_status(t("startup_error", msg=exc), error=True)
            self._startup_var.set(not self._startup_var.get())

    # ── Folder ────────────────────────────────────────────────────────────────
    def _browse_folder(self) -> None:
        current = Path(self._folder_var.get())
        initial = str(current) if current.exists() else str(Path.home())
        chosen = filedialog.askdirectory(
            title=t("select_folder"), initialdir=initial,
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
            self._folder_info.configure(text=t("folder_not_found"), foreground="#e74c3c")
            return

        self._folder_info.configure(text=t("folder_scanning"), foreground="gray")

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
        self._folder_info.configure(
            text=t("folder_images_found", n=count), foreground="gray",
        )

        for i, img_path in enumerate(images[:100]):
            self._img_tree.insert("", END, values=(f"{i+1:03d}  {img_path.name}",))

        if count > 100:
            self._img_tree.insert("", END, values=(t("folder_more_images", n=count - 100),))

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
            self._set_status(t("error_prefix", msg=e), error=True)
            return
        n = len(self._monitors)
        self._lbl_mon_count.configure(text=t("monitors_count", n=n))
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
                cw // 2, ch // 2, text=t("no_monitor_detected"),
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
                "language": self._lang_var.get(),
            },
            "paths": {
                "wallpapers_folder": self._folder_var.get(),
                "output_folder": self._cfg["paths"].get("output_folder", "assets/output"),
                "default_wallpaper": self._default_wp_var.get(),
            },
            "display": {
                "fit_mode": self._fit_var.get(),
            },
            "hotkeys": {
                "next_wallpaper": self._hk_next_var.get(),
                "prev_wallpaper": self._hk_prev_var.get(),
                "stop_watch": self._hk_stop_var.get(),
                "default_wallpaper": self._hk_default_var.get(),
                "toggle_transparency": self._hk_transp_var.get(),
            },
        }

    # ── Actions ───────────────────────────────────────────────────────────────
    def _apply_now(self) -> None:
        if not self._monitors:
            self._set_status(t("no_monitor_action"), error=True)
            return
        self._apply_btn.configure(state=DISABLED, text=t("applying"))
        self._set_status(t("applying"))

        def _work() -> None:
            try:
                cfg = self._collect_config()
                out_dir = resolve_path(cfg["paths"]["output_folder"])
                out_dir.mkdir(parents=True, exist_ok=True)
                out, images_used = apply_wallpaper(cfg, self._monitors, out_dir)
                # Track history
                if self._wp_hist_idx < len(self._wp_history) - 1:
                    self._wp_history = self._wp_history[: self._wp_hist_idx + 1]
                self._wp_history.append(images_used)
                self._wp_hist_idx = len(self._wp_history) - 1
                self.after(0, lambda: self._set_status(
                    t("wallpaper_applied", name=Path(str(out)).name),
                ))
            except Exception as exc:
                self.after(0, lambda: self._set_status(t("error_prefix", msg=exc), error=True))
            finally:
                self.after(0, lambda: self._apply_btn.configure(
                    state=NORMAL, text=t("apply_now"),
                ))

        threading.Thread(target=_work, daemon=True).start()

    def _save_config(self) -> None:
        try:
            cfg = self._collect_config()
            save_config(cfg)
            self._cfg = cfg
            self._register_hotkeys()
            self._save_transparency_settings()
            self._set_status(t("config_saved"))
        except Exception as exc:
            self._set_status(t("save_error", msg=exc), error=True)

    def _toggle_watch(self) -> None:
        if self._watching:
            self._watching = False
            schedule.clear()
            self._watch_btn.configure(text=t("start_watch"), style="info.TButton")
            self._set_status(t("watch_disabled"))
        else:
            cfg = self._collect_config()
            interval = cfg["general"]["interval"]
            self._watching = True
            self._watch_btn.configure(text=t("stop_watch"), style="danger.TButton")
            self._set_status(t("watch_active", n=interval))
            schedule.every(interval).seconds.do(self._apply_now)

            def _loop() -> None:
                while self._watching:
                    schedule.run_pending()
                    time.sleep(1)

            self._watch_thr = threading.Thread(target=_loop, daemon=True)
            self._watch_thr.start()

    # ── Hotkey actions ────────────────────────────────────────────────────────

    def _hotkey_next(self) -> None:
        """Hotkey: apply next wallpaper."""
        self._apply_now()

    def _hotkey_prev(self) -> None:
        """Hotkey: go back to the previous wallpaper."""
        if self._wp_hist_idx <= 0:
            self._set_status(t("no_prev_wallpaper"))
            return
        self._wp_hist_idx -= 1
        images = self._wp_history[self._wp_hist_idx]

        def _work() -> None:
            try:
                cfg = self._collect_config()
                out_dir = resolve_path(cfg["paths"]["output_folder"])
                out_dir.mkdir(parents=True, exist_ok=True)
                out, _ = apply_wallpaper(cfg, self._monitors, out_dir, preset_images=images)
                self.after(0, lambda: self._set_status(
                    t("prev_applied", name=Path(str(out)).name),
                ))
            except Exception as exc:
                self.after(0, lambda: self._set_status(t("error_prefix", msg=exc), error=True))

        threading.Thread(target=_work, daemon=True).start()

    def _hotkey_default(self) -> None:
        """Hotkey: apply the configured default wallpaper."""
        path = self._default_wp_var.get()
        if not path or not Path(path).exists():
            self._set_status(t("default_wp_not_found"), error=True)
            return
        if not self._monitors:
            self._set_status(t("no_monitor_error"), error=True)
            return

        def _work() -> None:
            try:
                cfg = self._collect_config()
                out_dir = resolve_path(cfg["paths"]["output_folder"])
                out_dir.mkdir(parents=True, exist_ok=True)
                fit = cfg["display"]["fit_mode"]
                out = apply_single_wallpaper(path, self._monitors, out_dir, fit)
                self.after(0, lambda: self._set_status(
                    t("default_wp_applied", name=Path(str(out)).name),
                ))
            except Exception as exc:
                self.after(0, lambda: self._set_status(t("error_prefix", msg=exc), error=True))

        threading.Thread(target=_work, daemon=True).start()

    # ── Hotkey helpers ────────────────────────────────────────────────────────

    def _register_hotkeys(self) -> None:
        """Register (or re-register) all global hotkeys."""
        self._hk_manager.update({
            self._hk_next_var.get(): lambda: self.after(0, self._hotkey_next),
            self._hk_prev_var.get(): lambda: self.after(0, self._hotkey_prev),
            self._hk_stop_var.get(): lambda: self.after(0, self._toggle_watch),
            self._hk_default_var.get(): lambda: self.after(0, self._hotkey_default),
            self._hk_transp_var.get(): lambda: self.after(0, self._hotkey_half_opacity),
        })

    def _record_hotkey(self, var: tk.StringVar, btn_idx: int) -> None:
        """Start recording a hotkey combo in a background thread."""
        if not hotkeys_available():
            self._set_status(t("hk_lib_unavailable"), error=True)
            return
        btn = self._hk_record_btns[btn_idx]
        btn.configure(text="...", state=DISABLED)
        old_val = var.get()
        var.set(t("hk_recording"))

        def _do_record() -> None:
            try:
                combo = read_hotkey()
            except Exception:
                combo = old_val
            self.after(0, lambda: self._finish_record(var, btn, combo))

        threading.Thread(target=_do_record, daemon=True).start()

    def _finish_record(self, var: tk.StringVar, btn: ttk.Button, combo: str) -> None:
        var.set(combo)
        btn.configure(text=t("hk_record"), state=NORMAL)
        self._register_hotkeys()

    def _browse_default_wp(self) -> None:
        """Open a file dialog to select the default wallpaper image."""
        current = self._default_wp_var.get()
        initial = str(Path(current).parent) if current and Path(current).exists() else str(Path.home())
        chosen = filedialog.askopenfilename(
            title=t("select_default_wp"),
            initialdir=initial,
            filetypes=[("Images", "*.jpg *.jpeg *.png *.bmp *.webp"), ("All", "*.*")],
        )
        if chosen:
            self._default_wp_var.set(chosen)

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
            pystray.MenuItem(t("tray_show"), lambda: self.after(0, self._show_from_tray), default=True),
            pystray.MenuItem(t("tray_apply"), lambda: self.after(0, self._apply_now)),
            pystray.Menu.SEPARATOR,
            pystray.MenuItem(t("tray_quit"), lambda: self.after(0, self._quit_app)),
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
        self._hk_manager.unregister_all()
        # Save transparency before exit
        self._save_transparency_settings()
        # Stop pynput listeners
        try:
            if self._pynput_mouse_listener:
                self._pynput_mouse_listener.stop()
            if self._pynput_kb_listener:
                self._pynput_kb_listener.stop()
        except Exception:
            pass
        if self._tray_icon is not None:
            self._tray_icon.stop()
            self._tray_icon = None
        self.destroy()

    # ── Startup-to-tray ──────────────────────────────────────────────────────
    def _startup_to_tray(self) -> None:
        """Minimise to tray and auto-start watch when launched via Windows startup."""
        self._minimize_to_tray()
        # Auto-start watch if interval > 0
        cfg = self._collect_config()
        interval = cfg["general"]["interval"]
        if interval > 0 and not self._watching:
            self._toggle_watch()


# ── Entry point ───────────────────────────────────────────────────────────────

def run() -> None:
    """Inicia a interface grafica."""
    # Load language early so even the "already running" message is translated
    try:
        cfg = load_config()
        set_language(cfg["general"].get("language", "en"))
    except Exception:
        pass

    if not _acquire_single_instance():
        root = tk.Tk()
        root.withdraw()
        messagebox.showwarning(
            "WallpaperChanger",
            t("already_running"),
        )
        root.destroy()
        return
    app = WallpaperChangerApp()
    app.mainloop()


if __name__ == "__main__":
    run()
