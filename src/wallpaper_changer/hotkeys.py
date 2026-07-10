"""Reliable global hotkey management for WallpaperChanger."""

from __future__ import annotations

import logging
import threading
from collections.abc import Callable, Iterable, Mapping
from dataclasses import dataclass

try:
    import keyboard as _kb

    _AVAILABLE = True
except ImportError:
    _kb = None  # type: ignore[assignment]
    _AVAILABLE = False

log = logging.getLogger(__name__)

_MODIFIER_ORDER = ("ctrl", "alt", "shift", "windows")
_ALIASES = {
    "control": "ctrl",
    "ctl": "ctrl",
    "option": "alt",
    "win": "windows",
    "super": "windows",
    "cmd": "windows",
}


@dataclass(frozen=True)
class HotkeyRegistration:
    """Result of registering a set of shortcuts."""

    registered: tuple[str, ...]
    errors: tuple[str, ...]


def is_available() -> bool:
    """Return whether the optional ``keyboard`` package is importable."""
    return _AVAILABLE


def normalize_hotkey(combo: str) -> str:
    """Return a stable representation of a keyboard combination.

    Empty segments and whitespace are removed, common aliases are accepted, and
    modifiers are ordered consistently. A shortcut must include a non-modifier key.
    """
    keys = [
        _ALIASES.get(part.strip().lower(), part.strip().lower())
        for part in combo.split("+")
        if part.strip()
    ]
    if not keys:
        raise ValueError("shortcut is empty")
    if len(keys) != len(set(keys)):
        raise ValueError("shortcut contains the same key more than once")
    modifiers = [key for key in _MODIFIER_ORDER if key in keys]
    regular = [key for key in keys if key not in _MODIFIER_ORDER]
    if not regular:
        raise ValueError("shortcut must include a non-modifier key")
    return "+".join((*modifiers, *regular))


def read_hotkey() -> str:
    """Block until the user presses and releases a hotkey combination."""
    if not _AVAILABLE:
        return ""
    return normalize_hotkey(_kb.read_hotkey(suppress=False))  # type: ignore[union-attr]


class HotkeyManager:
    """Register global hotkeys and safely replace the whole binding set."""

    def __init__(self) -> None:
        self._handles: list[object] = []
        self._lock = threading.RLock()

    def unregister_all(self) -> None:
        """Remove every hotkey registered through this manager."""
        if not _AVAILABLE:
            return
        with self._lock:
            for handle in self._handles:
                try:
                    _kb.remove_hotkey(handle)  # type: ignore[union-attr]
                except Exception:
                    log.debug("Could not remove hotkey handle", exc_info=True)
            self._handles.clear()

    def update(
        self,
        bindings: Mapping[str, Callable[[], None]]
        | Iterable[tuple[str, Callable[[], None]]],
    ) -> HotkeyRegistration:
        """Replace bindings, reporting invalid, duplicate, or unavailable shortcuts."""
        self.unregister_all()
        if not _AVAILABLE:
            return HotkeyRegistration((), ("Global hotkeys are unavailable.",))

        normalized: dict[str, Callable[[], None]] = {}
        errors: list[str] = []
        items = bindings.items() if isinstance(bindings, Mapping) else bindings
        for raw_combo, callback in items:
            try:
                combo = normalize_hotkey(raw_combo)
            except ValueError as exc:
                errors.append(f"{raw_combo!r}: {exc}")
                continue
            if combo in normalized:
                errors.append(f"{combo}: assigned to more than one action")
                continue
            normalized[combo] = callback

        registered: list[str] = []
        with self._lock:
            for combo, callback in normalized.items():
                try:
                    handle = _kb.add_hotkey(  # type: ignore[union-attr]
                        combo, callback, suppress=False, trigger_on_release=True
                    )
                    self._handles.append(handle)
                    registered.append(combo)
                except Exception as exc:
                    errors.append(f"{combo}: {exc}")
                    log.warning("Cannot register hotkey %r: %s", combo, exc)
        return HotkeyRegistration(tuple(registered), tuple(errors))
