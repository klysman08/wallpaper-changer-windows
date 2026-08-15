"""The library of collages the user has saved as image files.

Saving a collage is an export, not a wallpaper change: nothing on the desktop
moves. What makes the result a *library* rather than a pile of files is this index
— a JSON list beside the settings recording what each file was, which screen it
came from and which pictures went into it, so the gallery can show more than a
filename.

The index is derived data, not user data. It is reconciled against what is still
on disk every time it is read, and losing it costs only the metadata: the images
themselves live in :func:`get_library_dir` or wherever the save dialog pointed.
"""

from __future__ import annotations

import json
import logging
import os
from datetime import datetime
from pathlib import Path

from .config import get_user_config_dir, resolve_saved_dir

log = logging.getLogger(__name__)

INDEX_NAME = "gallery.json"

# Enough to keep years of saves, low enough that the gallery stays a gallery and
# the index stays small enough to rewrite on every save.
_MAX_ENTRIES = 500


def get_library_dir(cfg: dict | None = None) -> Path:
    """Where a save with no explicit destination lands.

    Follows ``paths.saved_folder``, so the user can point the library at their own
    Pictures folder. Left unset it is a subfolder of the local data directory, not
    the roaming config one: these are full-resolution desktop composites, far too
    large to drag through a roaming profile.

    Only new saves move when the setting changes. Entries already in the index carry
    absolute paths, so the gallery keeps listing pictures wherever they were written.
    """
    return resolve_saved_dir(cfg or {})


def get_index_file() -> Path:
    return get_user_config_dir() / INDEX_NAME


def suggest_name(monitor: int | None, extension: str = ".png") -> str:
    """A filename that sorts by time and says what it holds.

    Monitors are numbered from one here, matching every label the UI shows.
    """
    stamp = datetime.now().strftime("%Y-%m-%d_%H-%M-%S")
    which = "all" if monitor is None else f"monitor{monitor + 1}"
    return f"collage_{stamp}_{which}{extension}"


def _key(path: str | Path) -> str:
    """Identity of a saved file.

    Two Windows paths can name one file while differing in case and separators, so
    a plain string compare would let the same picture into the index twice.
    """
    return os.path.normcase(os.path.abspath(str(path)))


def _read() -> list[dict]:
    try:
        raw = json.loads(get_index_file().read_text(encoding="utf-8"))
    except FileNotFoundError:
        return []
    except (OSError, ValueError) as exc:
        # A corrupt index is worth strictly less than the app starting: the files
        # it describes are all still there, and the next save rebuilds it.
        log.warning("Gallery index unreadable, starting from empty: %s", exc)
        return []
    if not isinstance(raw, list):
        return []
    return [e for e in raw if isinstance(e, dict) and e.get("path")]


def _write(entries: list[dict]) -> None:
    target = get_index_file()
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(
        json.dumps(entries, indent=2, ensure_ascii=False), encoding="utf-8"
    )


def entries() -> list[dict]:
    """Saved collages, newest first, limited to the ones still on disk.

    A file deleted or moved from Explorer drops out here rather than lingering as a
    card that cannot be opened. The rewrite that prunes it is best-effort — the list
    this returns is correct either way.
    """
    stored = _read()
    live = [e for e in stored if Path(e["path"]).is_file()]
    if len(live) != len(stored):
        try:
            _write(live)
        except OSError as exc:
            log.warning("Could not prune the gallery index: %s", exc)
    return live


def find(path: str | Path) -> dict | None:
    """The entry describing one saved file, or None if it is not in the library.

    What the caller usually wants from it is ``monitor``: whether the file is one
    screen's worth of collage or the whole desktop, which decides how it should be
    laid back down as a wallpaper.
    """
    wanted = _key(path)
    return next((e for e in _read() if _key(e["path"]) == wanted), None)


def record(
    path: str | Path,
    *,
    monitor: int | None,
    images: list[str],
    width: int,
    height: int,
) -> dict:
    """Add one save to the front of the index and return the entry.

    Saving over an existing file replaces its entry instead of adding a second one
    describing the same picture.
    """
    entry = {
        "path": str(path),
        # Local time with its offset: the gallery shows this to a person, and the
        # offset keeps it unambiguous if the file is looked at from elsewhere.
        "saved_at": datetime.now().astimezone().isoformat(timespec="seconds"),
        "monitor": monitor,
        "images": [str(i) for i in images],
        "width": int(width),
        "height": int(height),
    }
    kept = [e for e in _read() if _key(e["path"]) != _key(path)]
    kept.insert(0, entry)
    del kept[_MAX_ENTRIES:]
    _write(kept)
    return entry


def forget(path: str | Path) -> bool:
    """Drop one entry from the index, leaving the file itself alone.

    Removing a picture from the library is a bookkeeping change; deleting the user's
    image is not something the app does on their behalf. Returns whether anything
    was actually removed.
    """
    stored = _read()
    kept = [e for e in stored if _key(e["path"]) != _key(path)]
    if len(kept) == len(stored):
        return False
    _write(kept)
    return True
