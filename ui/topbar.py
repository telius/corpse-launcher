"""
Top bar: launcher title, game count, search status, controller indicator.
"""

from __future__ import annotations
import pygame
import config
from ui.animations import Pulse


class TopBar:
    HEIGHT = 48

    def __init__(self, rect: pygame.Rect):
        self.rect    = rect
        self._pulse  = Pulse(period=3.0, lo=0.6, hi=1.0)
        self._pval   = 1.0

        try:
            self._title_font  = pygame.font.SysFont(
                "Inter,DejaVuSans,Liberation Sans,sans", 22, bold=True)
            self._meta_font   = pygame.font.SysFont(
                "Inter,DejaVuSans,Liberation Sans,sans", 13)
        except Exception:
            self._title_font  = pygame.font.Font(None, 28)
            self._meta_font   = pygame.font.Font(None, 16)

    def update(self, dt: float):
        self._pval = self._pulse.update(dt)

    def draw(self, surface: pygame.Surface, game_count: int,
             has_controller: bool, status: str = ""):
        # Background bar
        bar = pygame.Surface(self.rect.size, pygame.SRCALPHA)
        bar.fill((*config.BG_PANEL, 230))

        # Bottom border gradient
        for x in range(self.rect.width):
            t = x / self.rect.width
            # violet → magenta → cyan
            if t < 0.5:
                c = _lerp_colour(config.VIOLET, config.MAGENTA, t * 2)
            else:
                c = _lerp_colour(config.MAGENTA, config.CYAN, (t - 0.5) * 2)
            pygame.draw.line(bar, c, (x, self.rect.height - 1),
                             (x, self.rect.height - 1))

        # Title
        pv = self._pval
        title_colour = (
            int(config.VIOLET[0] + (config.CYAN[0] - config.VIOLET[0]) * (1 - pv)),
            int(config.VIOLET[1] + (config.CYAN[1] - config.VIOLET[1]) * (1 - pv)),
            int(config.VIOLET[2] + (config.CYAN[2] - config.VIOLET[2]) * (1 - pv)),
        )
        title = self._title_font.render("☠  CORPSE LAUNCHER", True, title_colour)
        bar.blit(title, (16, (self.HEIGHT - title.get_height()) // 2))

        # Game count
        count_s = self._meta_font.render(f"{game_count} games", True, config.GREY)
        cx = self.rect.width // 2 - count_s.get_width() // 2
        bar.blit(count_s, (cx, (self.HEIGHT - count_s.get_height()) // 2))

        # Right: controller + status
        right_x = self.rect.width - 16
        if status:
            st = self._meta_font.render(status, True, config.CYAN)
            right_x -= st.get_width()
            bar.blit(st, (right_x, (self.HEIGHT - st.get_height()) // 2))
            right_x -= 16

        ctrl_label = "🎮" if has_controller else "⌨"
        ctrl_colour = config.CYAN if has_controller else config.GREY
        cs = self._meta_font.render(ctrl_label, True, ctrl_colour)
        right_x -= cs.get_width()
        bar.blit(cs, (right_x, (self.HEIGHT - cs.get_height()) // 2))

        surface.blit(bar, self.rect.topleft)

    def resize(self, rect: pygame.Rect):
        self.rect = rect


def _lerp_colour(a: tuple, b: tuple, t: float) -> tuple:
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))
