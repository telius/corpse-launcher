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
            ("F", "Toggle FPS Limit"),
            ("N", "Toggle Animations"),
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
    """Full-screen translucent overlay listing all keybinds + settings toggles."""

    def __init__(self, screen_size: tuple[int, int]):
        self._sw, self._sh = screen_size
        self._open = False
        # OLED pixel-shift toggle — loaded from configuration file
        self._pixel_shift_enabled = config.get("ui", "pixel_shift")
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
        if self._open:
            self.close()
        else:
            self._init_fonts()
            self._open = True

    def close(self):
        self._open = False
        # Strategy B: Release fonts on close to minimize memory footprint
        self._heading_font = None
        self._key_font = None
        self._desc_font = None
        self._title_font = None
        self._hint_font = None
        import gc
        gc.collect()

    @property
    def is_open(self) -> bool:
        return self._open

    @property
    def pixel_shift_enabled(self) -> bool:
        return self._pixel_shift_enabled

    def toggle_pixel_shift(self) -> None:
        self._pixel_shift_enabled = not self._pixel_shift_enabled
        config.set_val("ui", "pixel_shift", self._pixel_shift_enabled)

    def toggle_fps(self, app) -> None:
        vsync_enabled = config.get("general", "vsync")
        fps_limit = config.get("general", "fps")

        # Toggle cycle: VSync -> 30 FPS -> 60 FPS -> 144 FPS
        if vsync_enabled:
            # Transition to 30 FPS, turn off VSync
            next_vsync = False
            next_fps = 30
        elif fps_limit == 30:
            next_vsync = False
            next_fps = 60
        elif fps_limit == 60:
            next_vsync = False
            next_fps = 144
        else:
            # From 144 (or other), cycle back to VSync
            next_vsync = True
            next_fps = 144

        config.set_val("general", "vsync", next_vsync)
        config.set_val("general", "fps", next_fps)

        # Apply to app
        app.TARGET_FPS = next_fps
        vsync_val = 1 if next_vsync else 0
        
        # Recreate display surface to change VSync mode in SDL
        w, h = app._win_size
        app.screen = pygame.display.set_mode(
            (w, h), pygame.RESIZABLE | pygame.DOUBLEBUF, vsync=vsync_val
        )
        app._layout()

    def toggle_animations(self, app) -> None:
        current_anim = config.get("ui", "animations")
        config.set_val("ui", "animations", not current_anim)
        # Instantly sync grid scroll if animations are being disabled
        if current_anim:  # was True, now transitioning to False
            app.grid._scroll_y = app.grid._target_y
            app.grid.dirty = True
        app._needs_draw = True

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
        hint = self._hint_font.render(
            "Press F1 or Esc to close", True, config.GREY
        )
        panel.blit(hint, (pw // 2 - hint.get_width() // 2, ph - hint.get_height() - 12))

        # ── Settings strip ────────────────────────────────────────────────
        strip_h = self._key_font.get_height() + 14
        strip_y = ph - strip_h - self._hint_font.get_height() - 22
        pygame.draw.line(panel, config.BORDER_BG, (20, strip_y - 8), (pw - 20, strip_y - 8), 1)

        # Section label
        settings_label = self._heading_font.render("Settings", True, config.VIOLET)
        panel.blit(settings_label, (20, strip_y))

        # Pixel shift toggle pill
        pill_x = 20 + settings_label.get_width() + 12
        pill_y = strip_y
        enabled = self._pixel_shift_enabled
        pill_color = config.CYAN if enabled else config.GREY
        status_text = "ON" if enabled else "OFF"
        key_text = "[P]"
        label_text = f"Pixel Shift: {status_text}"

        key_s = self._key_font.render(key_text, True, config.CYAN)
        label_s = self._key_font.render(label_text, True, pill_color)

        # Highlight pill background
        pill_w = key_s.get_width() + label_s.get_width() + 16
        pill_rect = pygame.Rect(pill_x - 6, pill_y - 3, pill_w, strip_h)
        pill_bg = pygame.Surface(pill_rect.size, pygame.SRCALPHA)
        pill_bg.fill((*config.VIOLET, 30) if enabled else (30, 30, 30, 80))
        panel.blit(pill_bg, pill_rect.topleft)
        pygame.draw.rect(panel, pill_color, pill_rect, 1)

        tx = pill_x
        panel.blit(key_s, (tx, pill_y + 4))
        tx += key_s.get_width() + 8
        panel.blit(label_s, (tx, pill_y + 4))

        # Target FPS / VSync toggle pill
        fps_pill_x = pill_x + pill_w + 16
        vsync_enabled = config.get("general", "vsync")
        fps_limit = config.get("general", "fps")
        
        if vsync_enabled:
            fps_status = "VSYNC"
        else:
            fps_status = f"{fps_limit} FPS"
            
        fps_key_text = "[F]"
        fps_label_text = f"Frame Limit: {fps_status}"
        
        fps_key_s = self._key_font.render(fps_key_text, True, config.CYAN)
        fps_label_s = self._key_font.render(fps_label_text, True, config.CYAN)
        
        fps_pill_w = fps_key_s.get_width() + fps_label_s.get_width() + 16
        fps_pill_rect = pygame.Rect(fps_pill_x - 6, pill_y - 3, fps_pill_w, strip_h)
        fps_pill_bg = pygame.Surface(fps_pill_rect.size, pygame.SRCALPHA)
        fps_pill_bg.fill((*config.VIOLET, 30))
        panel.blit(fps_pill_bg, fps_pill_rect.topleft)
        pygame.draw.rect(panel, config.CYAN, fps_pill_rect, 1)
        
        fx = fps_pill_x
        panel.blit(fps_key_s, (fx, pill_y + 4))
        fx += fps_key_s.get_width() + 8
        panel.blit(fps_label_s, (fx, pill_y + 4))

        # Animations toggle pill
        anim_pill_x = fps_pill_x + fps_pill_w + 16
        anim_enabled = config.get("ui", "animations")
        anim_color = config.CYAN if anim_enabled else config.GREY
        anim_status = "ON" if anim_enabled else "OFF"
        
        anim_key_text = "[N]"
        anim_label_text = f"Animations: {anim_status}"
        
        anim_key_s = self._key_font.render(anim_key_text, True, config.CYAN)
        anim_label_s = self._key_font.render(anim_label_text, True, anim_color)
        
        anim_pill_w = anim_key_s.get_width() + anim_label_s.get_width() + 16
        anim_pill_rect = pygame.Rect(anim_pill_x - 6, pill_y - 3, anim_pill_w, strip_h)
        anim_pill_bg = pygame.Surface(anim_pill_rect.size, pygame.SRCALPHA)
        anim_pill_bg.fill((*config.VIOLET, 30) if anim_enabled else (30, 30, 30, 80))
        panel.blit(anim_pill_bg, anim_pill_rect.topleft)
        pygame.draw.rect(panel, anim_color, anim_pill_rect, 1)
        
        ax = anim_pill_x
        panel.blit(anim_key_s, (ax, pill_y + 4))
        ax += anim_key_s.get_width() + 8
        panel.blit(anim_label_s, (ax, pill_y + 4))

        # Border
        pygame.draw.rect(panel, config.BORDER_BG, panel.get_rect(), 2)

        surface.blit(panel, (px, py))
