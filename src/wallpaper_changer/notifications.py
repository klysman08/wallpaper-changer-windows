"""Windows notification helpers."""
from __future__ import annotations

import base64
import logging
import subprocess
import threading

log = logging.getLogger(__name__)

_APP_ID = "WallpaperChanger"


def _ps_single_quote(text: str) -> str:
    return text.replace("'", "''")


def _build_toast_script(title: str, message: str) -> str:
    title_ps = _ps_single_quote(title)
    message_ps = _ps_single_quote(message)
    return f"""
Add-Type -AssemblyName System.Runtime.WindowsRuntime | Out-Null
[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] > $null
[Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom.XmlDocument, ContentType = WindowsRuntime] > $null
$title = '{title_ps}'
$message = '{message_ps}'
$titleEsc = [System.Security.SecurityElement]::Escape($title)
$messageEsc = [System.Security.SecurityElement]::Escape($message)
$xml = New-Object Windows.Data.Xml.Dom.XmlDocument
$xml.LoadXml("<toast><visual><binding template='ToastGeneric'><text>$titleEsc</text><text>$messageEsc</text></binding></visual></toast>")
$toast = [Windows.UI.Notifications.ToastNotification]::new($xml)
[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('{_APP_ID}').Show($toast)
"""


def send_windows_notification(title: str, message: str) -> None:
    """Send a best-effort Windows toast notification without blocking the UI."""

    def _worker() -> None:
        script = _build_toast_script(title, message)
        encoded = base64.b64encode(script.encode("utf-16le")).decode("ascii")
        try:
            subprocess.run(
                [
                    "powershell.exe",
                    "-NoProfile",
                    "-NonInteractive",
                    "-WindowStyle",
                    "Hidden",
                    "-EncodedCommand",
                    encoded,
                ],
                check=False,
                creationflags=subprocess.CREATE_NO_WINDOW,
                timeout=5,
            )
        except FileNotFoundError:
            log.warning("Cannot send notification because powershell.exe is unavailable")
        except subprocess.TimeoutExpired:
            log.warning("Windows notification timed out")
        except OSError as exc:
            log.warning("Cannot send Windows notification: %s", exc)

    threading.Thread(target=_worker, daemon=True).start()
