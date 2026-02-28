"""Transparency Controller — standalone ttkbootstrap GUI + global shortcuts."""
from __future__ import annotations

import threading
import tkinter as tk

import ttkbootstrap as ttk
from ttkbootstrap.constants import BOTH, BOTTOM, DISABLED, EW, LEFT, NORMAL, RIGHT, W, X, Y

from .transparency import (
    get_foreground_window,
    list_visible_windows,
    set_window_opacity,
)

# ── Constants ────────────────────────────────────────────────────────────────
_OPACITY_STEP = 5          # delta per scroll tick
_HALF_OPACITY = 128        # value applied by Alt+A
_ACCENT = "#3a7bd5"

# Track per-window opacity so the slider / shortcuts stay in sync
_opacity_map: dict[int, int] = {}


def _get_opacity(hwnd: int) -> int:
    """Return the last-known opacity for *hwnd*, defaulting to 255."""
    return _opacity_map.get(hwnd, 255)


def _record_opacity(hwnd: int, alpha: int) -> None:
    """Store the opacity value and apply it to the window."""
    alpha = max(0, min(255, alpha))
    _opacity_map[hwnd] = alpha
    set_window_opacity(hwnd, alpha)


# ── GUI ──────────────────────────────────────────────────────────────────────

class TransparencyApp(ttk.Window):
    """Standalone transparency controller window."""

    def __init__(self) -> None:
        super().__init__(
            title="Transparency Controller",
            themename="darkly",
            size=(520, 300),
            minsize=(420, 260),
            resizable=(True, False),
        )

        self._windows: list[tuple[int, str]] = []
        self._listener_thread: threading.Thread | None = None
        self._shortcut_thread: threading.Thread | None = None

        self._build_ui()
        self._refresh_window_list()
        self._start_global_shortcuts()

        self.protocol("WM_DELETE_WINDOW", self._on_close)

    # ── UI Construction ──────────────────────────────────────────────────────

    def _build_ui(self) -> None:
        main = ttk.Frame(self, padding=16)
        main.pack(fill=BOTH, expand=True)
        main.columnconfigure(0, weight=1)

        # ── Header ───────────────────────────────────────────────────────────
        hdr = ttk.Frame(main)
        hdr.grid(row=0, column=0, sticky=EW, pady=(0, 10))
        hdr.columnconfigure(1, weight=1)

        ttk.Label(
            hdr, text="🪟", font=("Segoe UI", 18),
        ).grid(row=0, column=0, rowspan=2, padx=(0, 10))

        ttk.Label(
            hdr, text="Transparency Controller",
            font=("Segoe UI", 16, "bold"), anchor=W,
        ).grid(row=0, column=1, sticky=W)

        ttk.Label(
            hdr, text="Control window opacity with shortcuts & slider",
            font=("Segoe UI", 9), foreground="gray", anchor=W,
        ).grid(row=1, column=1, sticky=W)

        # ── Window Selector ──────────────────────────────────────────────────
        sel_frame = ttk.Labelframe(main, text="Select Window", padding=10)
        sel_frame.grid(row=1, column=0, sticky=EW, pady=(0, 8))
        sel_frame.columnconfigure(0, weight=1)

        self._combo_var = tk.StringVar()
        self._combo = ttk.Combobox(
            sel_frame,
            textvariable=self._combo_var,
            state="readonly",
            font=("Segoe UI", 10),
        )
        self._combo.grid(row=0, column=0, sticky=EW, padx=(0, 8))
        self._combo.bind("<<ComboboxSelected>>", self._on_window_selected)

        ttk.Button(
            sel_frame, text="⟳ Refresh", style="Outline.TButton",
            command=self._refresh_window_list, width=12,
        ).grid(row=0, column=1)

        # ── Opacity Slider ───────────────────────────────────────────────────
        slider_frame = ttk.Labelframe(main, text="Opacity", padding=10)
        slider_frame.grid(row=2, column=0, sticky=EW, pady=(0, 8))
        slider_frame.columnconfigure(0, weight=1)

        self._opacity_var = tk.IntVar(value=255)
        self._slider = ttk.Scale(
            slider_frame,
            from_=50,
            to=255,
            variable=self._opacity_var,
            orient="horizontal",
            command=self._on_slider_change,
            style="info.Horizontal.TScale",
        )
        self._slider.grid(row=0, column=0, sticky=EW, padx=(0, 8))

        self._opacity_label = ttk.Label(
            slider_frame, text="255", font=("Segoe UI", 11, "bold"),
            width=4, anchor="center",
        )
        self._opacity_label.grid(row=0, column=1)

        # ── Shortcuts Info ───────────────────────────────────────────────────
        info_frame = ttk.Frame(main)
        info_frame.grid(row=3, column=0, sticky=EW)

        shortcuts = [
            ("Alt + A", "Set focused window to 50% opacity"),
            ("Alt + Scroll ↕", "Adjust focused window opacity gradually"),
        ]
        for sc_key, sc_desc in shortcuts:
            row = ttk.Frame(info_frame)
            row.pack(fill=X, pady=1)
            ttk.Label(
                row, text=sc_key, font=("Segoe UI", 9, "bold"),
                foreground=_ACCENT, width=16, anchor=W,
            ).pack(side=LEFT)
            ttk.Label(
                row, text=sc_desc, font=("Segoe UI", 9),
                foreground="gray", anchor=W,
            ).pack(side=LEFT)

        # ── Status Bar ───────────────────────────────────────────────────────
        status_bar = ttk.Frame(self, padding=(12, 4))
        status_bar.pack(side=BOTTOM, fill=X)

        self._status_lbl = ttk.Label(
            status_bar, text="Ready — shortcuts active",
            font=("Segoe UI", 9), foreground="gray",
        )
        self._status_lbl.pack(side=LEFT)

    # ── Window List ──────────────────────────────────────────────────────────

    def _refresh_window_list(self) -> None:
        self._windows = list_visible_windows()
        titles = [title for _, title in self._windows]
        self._combo["values"] = titles
        if titles:
            self._combo.current(0)
            self._on_window_selected()
        self._set_status(f"{len(titles)} windows found")

    def _selected_hwnd(self) -> int | None:
        idx = self._combo.current()
        if idx < 0 or idx >= len(self._windows):
            return None
        return self._windows[idx][0]

    # ── Event Handlers ───────────────────────────────────────────────────────

    def _on_window_selected(self, _event=None) -> None:
        hwnd = self._selected_hwnd()
        if hwnd is None:
            return
        alpha = _get_opacity(hwnd)
        self._opacity_var.set(alpha)
        self._opacity_label.configure(text=str(alpha))

    def _on_slider_change(self, value: str) -> None:
        alpha = int(float(value))
        self._opacity_label.configure(text=str(alpha))
        hwnd = self._selected_hwnd()
        if hwnd is None:
            return
        _record_opacity(hwnd, alpha)

    # ── Global Shortcuts ─────────────────────────────────────────────────────

    def _start_global_shortcuts(self) -> None:
        """Register Alt+A via *keyboard* and Alt+Scroll via *pynput*."""
        # --- keyboard: Alt+A ---
        self._shortcut_thread = threading.Thread(
            target=self._register_keyboard_shortcuts, daemon=True,
        )
        self._shortcut_thread.start()

        # --- pynput: Alt+Scroll ---
        self._listener_thread = threading.Thread(
            target=self._start_mouse_listener, daemon=True,
        )
        self._listener_thread.start()

    def _register_keyboard_shortcuts(self) -> None:
        try:
            import keyboard
            keyboard.add_hotkey("alt+a", self._hotkey_half_opacity, suppress=False)
        except Exception:
            self.after(0, lambda: self._set_status("⚠ Could not register Alt+A"))

    def _hotkey_half_opacity(self) -> None:
        """Set the currently focused window to 50% opacity."""
        hwnd = get_foreground_window()
        if hwnd:
            _record_opacity(hwnd, _HALF_OPACITY)
            # Update slider if this is the window we have selected
            self.after(0, self._sync_slider_if_match, hwnd)
            self.after(0, lambda: self._set_status(
                f"Applied 50% opacity (Alt+A) — alpha {_HALF_OPACITY}",
            ))

    def _start_mouse_listener(self) -> None:
        try:
            from pynput import mouse, keyboard as pynput_kb

            self._alt_pressed = False

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
                current = _get_opacity(hwnd)
                new_alpha = current + (int(dy) * _OPACITY_STEP)
                new_alpha = max(10, min(255, new_alpha))
                _record_opacity(hwnd, new_alpha)
                self.after(0, self._sync_slider_if_match, hwnd)
                self.after(0, lambda a=new_alpha: self._set_status(
                    f"Scroll opacity → {a}",
                ))

            # Start listeners (blocking, but in daemon thread)
            self._kb_listener = pynput_kb.Listener(
                on_press=on_press, on_release=on_release,
            )
            self._mouse_listener = mouse.Listener(on_scroll=on_scroll)
            self._kb_listener.start()
            self._mouse_listener.start()
            self._kb_listener.join()
            self._mouse_listener.join()
        except ImportError:
            self.after(0, lambda: self._set_status(
                "⚠ pynput not installed — Alt+Scroll disabled",
            ))
        except Exception:
            pass

    def _sync_slider_if_match(self, hwnd: int) -> None:
        """If the given hwnd matches the combo selection, update the slider."""
        selected = self._selected_hwnd()
        if selected == hwnd:
            alpha = _get_opacity(hwnd)
            self._opacity_var.set(alpha)
            self._opacity_label.configure(text=str(alpha))

    # ── Helpers ──────────────────────────────────────────────────────────────

    def _set_status(self, text: str) -> None:
        self._status_lbl.configure(text=text)

    def _on_close(self) -> None:
        """Clean up listeners and close."""
        try:
            import keyboard
            keyboard.remove_hotkey("alt+a")
        except Exception:
            pass
        try:
            if hasattr(self, "_mouse_listener"):
                self._mouse_listener.stop()
            if hasattr(self, "_kb_listener"):
                self._kb_listener.stop()
        except Exception:
            pass
        self.destroy()


# ── Entry Point ──────────────────────────────────────────────────────────────

def run() -> None:
    """Launch the Transparency Controller as a standalone application."""
    app = TransparencyApp()
    app.mainloop()


if __name__ == "__main__":
    run()
