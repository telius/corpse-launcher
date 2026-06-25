"""
PS5 button icon loader.
Loads PNG icons from the local icon pack and returns pre-scaled pygame Surfaces.
Icons are cached after first load.
"""

from __future__ import annotations
from pathlib import Path
from typing import Optional
import pygame

# Path to the white outline 256w icon set
ICON_DIR = Path("/home/james/projects/PS5 Button Icons and Controls/Buttons Outline/White/256w")

# Mapping from logical name → filename
_FILES: dict[str, str] = {
    "cross":     "Cross.png",
    "circle":    "Circle.png",
    "triangle":  "Triangle.png",
    "square":    "Square.png",
    "l1":        "L1.png",
    "r1":        "R1.png",
    "l2":        "L2.png",
    "r2":        "R2.png",
    "options":   "Options.png",
    "create":    "Create.png",
    "touchpad":  "Touch Pad Press.png",
    "dpad":      "D-Pad.png",
    "dpad_up":   "D-Pad Up.png",
    "dpad_down": "D-Pad Down.png",
    "dpad_left": "D-Pad Left.png",
    "dpad_right":"D-Pad Right.png",
    "lstick":    "Left Stick.png",
}

_cache: dict[tuple[str, int], pygame.Surface] = {}


def get(name: str, size: int = 20) -> Optional[pygame.Surface]:
    """
    Return a pygame Surface for the named PS5 button icon, scaled to `size` px.
    Returns None if the icon file is missing.
    Results are cached by (name, size).
    """
    key = (name, size)
    if key in _cache:
        return _cache[key]

    filename = _FILES.get(name)
    if not filename:
        return None

    path = ICON_DIR / filename
    if not path.exists():
        return None

    try:
        raw  = pygame.image.load(str(path)).convert_alpha()
        surf = pygame.transform.smoothscale(raw, (size, size))
        _cache[key] = surf
        return surf
    except Exception as e:
        print(f"[icons] Failed to load {filename}: {e}")
        return None


def draw_hint(surface: pygame.Surface, x: int, y: int,
              icon_name: str, label: str,
              font: pygame.font.Font,
              icon_size: int = 20,
              colour: tuple = (240, 240, 255),
              gap: int = 6) -> int:
    """
    Draw an icon + label pair at (x, y).
    Returns the total width used so caller can chain horizontally.
    """
    icon = get(icon_name, icon_size)
    cx = x
    if icon:
        iy = y + (font.get_height() - icon_size) // 2
        surface.blit(icon, (cx, iy))
        cx += icon_size + gap
    txt = font.render(label, True, colour)
    ty  = y + (font.get_height() - txt.get_height()) // 2 + 1
    surface.blit(txt, (cx, ty))
    return cx + txt.get_width()


def draw_hint_vertical(panel: pygame.Surface, x: int, y: int,
                       icon_name: str, label: str,
                       font: pygame.font.Font,
                       icon_size: int = 18,
                       colour: tuple = (240, 240, 255),
                       gap: int = 5) -> int:
    """
    Draw icon + label horizontally, return the height of one row.
    """
    row_h = max(icon_size, font.get_height()) + 4
    icon  = get(icon_name, icon_size)
    cx    = x
    if icon:
        iy = y + (row_h - icon_size) // 2
        panel.blit(icon, (cx, iy))
        cx += icon_size + gap
    txt = font.render(label, True, colour)
    ty  = y + (row_h - txt.get_height()) // 2
    panel.blit(txt, (cx, ty))
    return row_h
