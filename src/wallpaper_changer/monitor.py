"""Deteccao de monitores e resolucao total da area de trabalho."""
from __future__ import annotations
from dataclasses import dataclass

@dataclass
class Monitor:
    index: int
    x: int
    y: int
    width: int
    height: int

def get_monitors() -> list[Monitor]:
    from screeninfo import get_monitors as _get
    return [Monitor(i, m.x, m.y, m.width, m.height) for i, m in enumerate(_get())]

def get_virtual_desktop_size(monitors: list[Monitor]) -> tuple[int, int]:
    right  = max(m.x + m.width  for m in monitors)
    bottom = max(m.y + m.height for m in monitors)
    left   = min(m.x for m in monitors)
    top    = min(m.y for m in monitors)
    return (right - left, bottom - top)
