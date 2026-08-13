"""Native Windows global hotkey management for WallpaperChanger."""

from __future__ import annotations

import ctypes
import logging
import os
import queue
import threading
import time
from collections.abc import Callable, Iterable, Mapping
from ctypes import wintypes
from dataclasses import dataclass, field

log = logging.getLogger(__name__)

_WM_HOTKEY = 0x0312
_WM_COMMAND = 0x8001
_PM_NOREMOVE = 0x0000
_MOD_ALT = 0x0001
_MOD_CONTROL = 0x0002
_MOD_SHIFT = 0x0004
_MOD_WIN = 0x0008
_MOD_NOREPEAT = 0x4000

_MODIFIER_ORDER = ("ctrl", "alt", "shift", "windows")
_MODIFIER_FLAGS = {
    "ctrl": _MOD_CONTROL,
    "alt": _MOD_ALT,
    "shift": _MOD_SHIFT,
    "windows": _MOD_WIN,
}
_ALIASES = {
    "control": "ctrl",
    "ctl": "ctrl",
    "option": "alt",
    "win": "windows",
    "super": "windows",
    "cmd": "windows",
    "escape": "esc",
    "return": "enter",
    "pgup": "pageup",
    "page up": "pageup",
    "pgdn": "pagedown",
    "page down": "pagedown",
    "del": "delete",
    "ins": "insert",
    "spacebar": "space",
}

_NAMED_KEYS = {
    "backspace": 0x08,
    "tab": 0x09,
    "enter": 0x0D,
    "pause": 0x13,
    "capslock": 0x14,
    "esc": 0x1B,
    "space": 0x20,
    "pageup": 0x21,
    "pagedown": 0x22,
    "end": 0x23,
    "home": 0x24,
    "left": 0x25,
    "up": 0x26,
    "right": 0x27,
    "down": 0x28,
    "printscreen": 0x2C,
    "insert": 0x2D,
    "delete": 0x2E,
    "numpad0": 0x60,
    "numpad1": 0x61,
    "numpad2": 0x62,
    "numpad3": 0x63,
    "numpad4": 0x64,
    "numpad5": 0x65,
    "numpad6": 0x66,
    "numpad7": 0x67,
    "numpad8": 0x68,
    "numpad9": 0x69,
    "multiply": 0x6A,
    "add": 0x6B,
    "subtract": 0x6D,
    "decimal": 0x6E,
    "divide": 0x6F,
    "numlock": 0x90,
    "scrolllock": 0x91,
    ";": 0xBA,
    "=": 0xBB,
    ",": 0xBC,
    "-": 0xBD,
    ".": 0xBE,
    "/": 0xBF,
    "`": 0xC0,
    "[": 0xDB,
    "\\": 0xDC,
    "]": 0xDD,
    "'": 0xDE,
}
_KEY_TO_VK = {
    **_NAMED_KEYS,
    **{chr(code).lower(): code for code in range(ord("A"), ord("Z") + 1)},
    **{str(number): ord(str(number)) for number in range(10)},
    **{f"f{number}": 0x6F + number for number in range(1, 25)},
}
_VK_TO_KEY = {vk: key for key, vk in _KEY_TO_VK.items()}

_MODIFIER_VKS = {
    "ctrl": (0x11, 0xA2, 0xA3),
    "alt": (0x12, 0xA4, 0xA5),
    "shift": (0x10, 0xA0, 0xA1),
    "windows": (0x5B, 0x5C),
}

if os.name == "nt":
    _USER32 = ctypes.WinDLL("user32", use_last_error=True)
    _KERNEL32 = ctypes.WinDLL("kernel32", use_last_error=True)
else:  # pragma: no cover - this project only runs on Windows
    _USER32 = None
    _KERNEL32 = None


@dataclass(frozen=True)
class HotkeyRegistration:
    """Result of registering a set of shortcuts."""

    registered: tuple[str, ...]
    errors: tuple[str, ...]


@dataclass
class _BackendCommand:
    action: str
    bindings: dict[str, Callable[[], None]] = field(default_factory=dict)
    done: threading.Event = field(default_factory=threading.Event)
    result: HotkeyRegistration = field(
        default_factory=lambda: HotkeyRegistration((), ())
    )


def is_available() -> bool:
    """Return whether the native Windows hotkey API is available."""
    return _USER32 is not None and _KERNEL32 is not None


def normalize_hotkey(combo: str) -> str:
    """Return the canonical ``modifiers+key`` representation of a shortcut."""
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
    if len(regular) != 1:
        raise ValueError("shortcut must contain exactly one non-modifier key")
    if regular[0] not in _KEY_TO_VK:
        raise ValueError(f"shortcut key {regular[0]!r} is not supported")
    return "+".join((*modifiers, regular[0]))


def _to_windows_hotkey(combo: str) -> tuple[int, int]:
    """Convert a normalized shortcut to Windows modifier flags and a VK code."""
    keys = combo.split("+")
    modifiers = _MOD_NOREPEAT
    for key in keys[:-1]:
        modifiers |= _MODIFIER_FLAGS[key]
    return modifiers, _KEY_TO_VK[keys[-1]]


def _key_is_down(vk: int) -> bool:
    return bool(_USER32.GetAsyncKeyState(vk) & 0x8000)


def read_hotkey() -> str:
    """Record one shortcut using native Windows key-state polling.

    This is only used while the user is in the shortcut editor. Normal shortcut
    delivery uses ``RegisterHotKey`` and does not install a keyboard hook.
    """
    if not is_available():
        return ""

    all_vks = (*_VK_TO_KEY, *(vk for values in _MODIFIER_VKS.values() for vk in values))
    while any(_key_is_down(vk) for vk in all_vks):
        time.sleep(0.01)

    while True:
        pressed = next((vk for vk in _VK_TO_KEY if _key_is_down(vk)), None)
        if pressed is None:
            time.sleep(0.01)
            continue
        modifiers = [
            name
            for name in _MODIFIER_ORDER
            if any(_key_is_down(vk) for vk in _MODIFIER_VKS[name])
        ]
        while _key_is_down(pressed):
            time.sleep(0.01)
        return normalize_hotkey("+".join((*modifiers, _VK_TO_KEY[pressed])))


class _NativeHotkeyBackend:
    """Own a Windows message loop on a dedicated thread."""

    def __init__(self) -> None:
        if not is_available():
            raise RuntimeError("native Windows hotkeys are unavailable")
        self._commands: queue.SimpleQueue[_BackendCommand] = queue.SimpleQueue()
        self._callbacks: dict[int, Callable[[], None]] = {}
        self._thread_id = 0
        self._ready = threading.Event()
        self._startup_error: Exception | None = None
        self._closed = False
        self._thread = threading.Thread(
            target=self._message_loop,
            name="WallpaperChangerHotkeys",
            daemon=True,
        )
        self._thread.start()
        if not self._ready.wait(timeout=3):
            raise RuntimeError("native hotkey message loop did not start")
        if self._startup_error is not None:
            raise RuntimeError(
                f"native hotkey message loop failed: {self._startup_error}"
            )

    def replace(self, bindings: dict[str, Callable[[], None]]) -> HotkeyRegistration:
        return self._submit(_BackendCommand("replace", bindings))

    def unregister_all(self) -> None:
        if not self._closed:
            self._submit(_BackendCommand("replace"))

    def close(self) -> None:
        if self._closed:
            return
        self._submit(_BackendCommand("close"))
        self._closed = True
        self._thread.join(timeout=3)

    def _submit(self, command: _BackendCommand) -> HotkeyRegistration:
        if self._closed:
            return HotkeyRegistration((), ("Native hotkey manager is closed.",))
        self._commands.put(command)
        if not _USER32.PostThreadMessageW(self._thread_id, _WM_COMMAND, 0, 0):
            error = ctypes.get_last_error()
            detail = ctypes.FormatError(error).strip()
            return HotkeyRegistration(
                (), (f"Could not contact the hotkey message loop ({detail}).",)
            )
        if not command.done.wait(timeout=3):
            return HotkeyRegistration((), ("Native hotkey operation timed out.",))
        return command.result

    def _message_loop(self) -> None:
        try:
            self._run_message_loop()
        except Exception as exc:  # pragma: no cover - defensive native API guard
            self._startup_error = exc
            log.exception("Native hotkey message loop failed")
        finally:
            self._ready.set()

    def _run_message_loop(self) -> None:
        self._thread_id = _KERNEL32.GetCurrentThreadId()
        msg = wintypes.MSG()
        # Calling PeekMessage creates this thread's message queue before callers
        # are allowed to use PostThreadMessage.
        _USER32.PeekMessageW(ctypes.byref(msg), None, 0, 0, _PM_NOREMOVE)
        self._ready.set()
        while _USER32.GetMessageW(ctypes.byref(msg), None, 0, 0) > 0:
            if msg.message == _WM_COMMAND:
                self._drain_commands()
            elif msg.message == _WM_HOTKEY:
                self._invoke_callback(int(msg.wParam))

    def _invoke_callback(self, hotkey_id: int) -> None:
        callback = self._callbacks.get(hotkey_id)
        if callback is None:
            return
        try:
            callback()
        except Exception:
            log.exception("Global hotkey callback failed")

    def _drain_commands(self) -> None:
        while True:
            try:
                command = self._commands.get_nowait()
            except queue.Empty:
                return
            try:
                if command.action == "replace":
                    command.result = self._replace_on_thread(command.bindings)
                elif command.action == "close":
                    self._unregister_on_thread()
                    _USER32.PostQuitMessage(0)
            except Exception as exc:  # pragma: no cover - defensive native API guard
                command.result = HotkeyRegistration((), (str(exc),))
                log.exception("Native hotkey command failed")
            finally:
                command.done.set()

    def _replace_on_thread(
        self, bindings: dict[str, Callable[[], None]]
    ) -> HotkeyRegistration:
        self._unregister_on_thread()
        registered: list[str] = []
        errors: list[str] = []
        for hotkey_id, (combo, callback) in enumerate(bindings.items(), start=1):
            modifiers, vk = _to_windows_hotkey(combo)
            ctypes.set_last_error(0)
            if _USER32.RegisterHotKey(None, hotkey_id, modifiers, vk):
                self._callbacks[hotkey_id] = callback
                registered.append(combo)
                continue
            error = ctypes.get_last_error()
            detail = (
                ctypes.FormatError(error).strip()
                if error
                else "shortcut is already in use"
            )
            errors.append(f"{combo}: {detail}")
            log.warning("Cannot register hotkey %r: %s", combo, detail)
        return HotkeyRegistration(tuple(registered), tuple(errors))

    def _unregister_on_thread(self) -> None:
        for hotkey_id in tuple(self._callbacks):
            if not _USER32.UnregisterHotKey(None, hotkey_id):
                log.debug("Could not unregister native hotkey id %s", hotkey_id)
        self._callbacks.clear()


class HotkeyManager:
    """Validate shortcuts and manage their native Windows registrations."""

    def __init__(self, backend: _NativeHotkeyBackend | None = None) -> None:
        self._backend: _NativeHotkeyBackend | None = backend
        self._backend_error: str | None = None
        if self._backend is None and is_available():
            try:
                self._backend = _NativeHotkeyBackend()
            except Exception as exc:
                self._backend_error = str(exc)
                log.exception("Could not initialize native global hotkeys")

    def unregister_all(self) -> None:
        """Remove every hotkey registered through this manager."""
        if self._backend is not None:
            self._backend.unregister_all()

    def close(self) -> None:
        """Remove all hotkeys and stop the native message-loop thread."""
        if self._backend is not None:
            self._backend.close()
            self._backend = None

    def update(
        self,
        bindings: Mapping[str, Callable[[], None]]
        | Iterable[tuple[str, Callable[[], None]]],
    ) -> HotkeyRegistration:
        """Replace bindings, reporting invalid, duplicate, or unavailable shortcuts."""
        if self._backend is None:
            reason = (
                self._backend_error
                or "Native Windows global hotkeys are unavailable."
            )
            return HotkeyRegistration((), (reason,))

        normalized: dict[str, Callable[[], None]] = {}
        errors: list[str] = []
        items = bindings.items() if isinstance(bindings, Mapping) else bindings
        for raw_combo, callback in items:
            # Blank means "not bound", which is the shipped default for every
            # action — it is not a mistake worth reporting.
            if not raw_combo or not raw_combo.strip():
                continue
            try:
                combo = normalize_hotkey(raw_combo)
            except ValueError as exc:
                errors.append(f"{raw_combo!r}: {exc}")
                continue
            if combo in normalized:
                errors.append(f"{combo}: assigned to more than one action")
                continue
            normalized[combo] = callback

        result = self._backend.replace(normalized)
        return HotkeyRegistration(result.registered, (*errors, *result.errors))
