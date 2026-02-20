"""Global hotkey management for WallpaperChanger."""
from __future__ import annotations

import logging
from typing import Callable

try:
    import keyboard as _kb

    _AVAILABLE = True
except ImportError:
    _kb = None  # type: ignore[assignment]
    _AVAILABLE = False

log = logging.getLogger(__name__)


def is_available() -> bool:
    """Return True if the *keyboard* library is importable."""
    return _AVAILABLE


def read_hotkey() -> str:
    """Block until the user presses and releases a hotkey combo.

    Returns the combo string (e.g. ``ctrl+alt+right``).
    """
    if not _AVAILABLE:
        return ""
    return _kb.read_hotkey(suppress=False)  # type: ignore[union-attr]


class HotkeyManager:
    """Register / unregister global hotkeys."""

    def __init__(self) -> None:
        self._registered: list[str] = []

    def register(self, combo: str, callback: Callable[[], None]) -> None:
        """Register a single global hotkey."""
        if not _AVAILABLE or not combo.strip():
            return
        try:
            _kb.add_hotkey(combo, callback, suppress=False)  # type: ignore[union-attr]
            self._registered.append(combo)
        except Exception as exc:
            log.warning("Cannot register hotkey %r: %s", combo, exc)

    def unregister_all(self) -> None:
        """Remove every hotkey registered through this manager."""
        if not _AVAILABLE:
            return
        for hk in self._registered:
            try:
                _kb.remove_hotkey(hk)  # type: ignore[union-attr]
            except Exception:
                pass
        self._registered.clear()

    def update(self, bindings: dict[str, Callable[[], None]]) -> None:
        """Replace all current hotkeys with *bindings*."""
        self.unregister_all()
        for combo, cb in bindings.items():
            self.register(combo, cb)
