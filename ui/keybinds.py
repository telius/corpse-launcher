"""
Keybinds help overlay — shown on F1 press.
Displays all keyboard, mouse, and controller bindings.
"""

from __future__ import annotations
import pygame

import config
import ui.icons as icons


_KEYBINDS = [
    (
        "Navigation",
        [
            ("Arrow Keys / WASD", "Move selection"),
            ("Mouse Click", "Select game"),
            ("Mouse Wheel", "Scroll grid"),
            ("Page Up / Page Down", "Scroll by page"),
        ],
    ),
    (
        "Actions",
        [
            ("Enter / Space", "Launch game"),
            ("Double-Click", "Launch game"),
            ("I", "Details / Actions"),
            ("H", "Hide / Unhide game"),
            ("Tab", "Toggle show hidden"),
            ("F5", "Refresh library"),
        ],
    ),
    (
        "Controller",
        [
            ("✕  Cross", "Launch"),
            ("△  Triangle", "Details / Actions"),
            ("□  Square", "Details / Actions"),
            ("○  Circle", "Back / Close overlay"),
            ("D-Pad Up", "Hide / Unhide game"),
            ("D-Pad / Left Stick", "Navigate"),
            ("L1 / R1", "Page scroll"),
            ("Create", "Toggle show hidden"),
            ("Options", "Refresh library"),
        ],
    ),
]


class KeybindsOverlay:
    """Full-screen translucent overlay listing all keybinds."""

    def __init__(self, screen_size: tuple[int, int]):
        self._sw, self._sh = screen_size
        self._open = False
        self._init_fonts()

    def _init_fonts(self):
        try:
            self._heading_font = pygame.font.SysFont(
                "Inter,DejaVuSans,Liberation Sans,sans", 20, bold=True
            )
            self._key_font = pygame.font.SysFont(
                "Inter,DejaVuSans,Liberation Sans,sans", 14, bold=True
            )
            self._desc_font = pygame.font.SysFont(
                "Inter,DejaVuSans,Liberation Sans,sans", 14
            )
            self._title_font = pygame.font.SysFont(
                "Inter,DejaVuSans,Liberation Sans,sans", 28, bold=True
            )
            self._hint_font = pygame.font.SysFont(
                "Inter,DejaVuSans,Liberation Sans,sans", 12
            )
        except Exception:
            self._heading_font = pygame.font.Font(None, 24)
            self._key_font = pygame.font.Font(None, 18)
            self._desc_font = pygame.font.Font(None, 18)
            self._title_font = pygame.font.Font(None, 32)
            self._hint_font = pygame.font.Font(None, 16)

    def resize(self, w: int, h: int):
        self._sw, self._sh = w, h
        self._init_fonts()

    def toggle(self):
        self._open = not self._open

    def close(self):
        self._open = False

    @property
    def is_open(self) -> bool:
        return self._open

    def draw(self, surface: pygame.Surface):
        if not self._open:
            return

        sw, sh = self._sw, self._sh

        # Dark veil
        veil = pygame.Surface((sw, sh), pygame.SRCALPHA)
        veil.fill((0, 0, 0, 210))
        surface.blit(veil, (0, 0))

        # Panel
        pw = min(680, sw - 60)
        ph = min(560, sh - 60)
        px = (sw - pw) // 2
        py = (sh - ph) // 2

        panel = pygame.Surface((pw, ph), pygame.SRCALPHA)
        panel.fill((*config.BG_PANEL, 250))

        # Title
        title = self._title_font.render("Keybinds", True, config.WHITE)
        panel.blit(title, (pw // 2 - title.get_width() // 2, 16))

        # Columns: lay out sections side by side
        col_w = (pw - 40) // len(_KEYBINDS)
        cx = 20
        top_y = 60

        for section_name, bindings in _KEYBINDS:
            y = top_y

            # Section heading
            heading = self._heading_font.render(section_name, True, config.VIOLET)
            panel.blit(heading, (cx, y))
            y += heading.get_height() + 8

            # Underline
            pygame.draw.line(panel, config.BORDER_BG, (cx, y), (cx + col_w - 20, y), 1)
            y += 6

            for key_label, desc_label in bindings:
                # If this is the Controller section, try drawing real PNG icons
                icon_surf = None
                if section_name == "Controller":
                    # Extract Logical mapping name for icons
                    lower_label = key_label.lower()
                    mapped_name = None
                    if "cross" in lower_label:
                        mapped_name = "cross"
                    elif "circle" in lower_label:
                        mapped_name = "circle"
                    elif "triangle" in lower_label:
                        mapped_name = "triangle"
                    elif "square" in lower_label:
                        mapped_name = "square"
                    elif "d-pad up" in lower_label:
                        mapped_name = "dpad_up"
                    elif "d-pad" in lower_label:
                        mapped_name = "dpad"
                    elif "l1" in lower_label:
                        mapped_name = "l1"
                    elif "r1" in lower_label:
                        mapped_name = "r1"
                    elif "create" in lower_label:
                        mapped_name = "create"
                    elif "options" in lower_label:
                        mapped_name = "options"

                    if mapped_name:
                        icon_surf = icons.get(mapped_name, 18)

                if icon_surf:
                    # Blit icon and draw label text next to it
                    iy = y + (self._key_font.get_height() - 18) // 2
                    panel.blit(icon_surf, (cx, iy))
                    ks = self._key_font.render(
                        key_label.split(None, 1)[-1], True, config.CYAN
                    )
                    panel.blit(ks, (cx + 24, y))
                else:
                    ks = self._key_font.render(key_label, True, config.CYAN)
                    panel.blit(ks, (cx, y))

                y += ks.get_height() + 1
                ds = self._desc_font.render(desc_label, True, config.GREY)
                panel.blit(ds, (cx + 8, y))
                y += ds.get_height() + 8

            cx += col_w

        # Bottom hint
        hint = self._hint_font.render("Press F1 or Esc to close", True, config.GREY)
        panel.blit(hint, (pw // 2 - hint.get_width() // 2, ph - hint.get_height() - 12))

        # Border
        pygame.draw.rect(panel, config.BORDER_BG, panel.get_rect(), 2)

        surface.blit(panel, (px, py))
