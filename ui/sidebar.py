"""
Right-side detail panel for the selected game.
Uses real PS5 button icon PNGs for the hint row.
"""

from __future__ import annotations
from typing import Optional
import pygame

import config
from data.game import Game, Platform
import data.hidden as hidden_store
from ui.animations import Tween


def _colour_for_platform(p: Platform) -> tuple:
    return {
        Platform.STEAM: config.BADGE_STEAM,
        Platform.LUTRIS: config.BADGE_LUTRIS,
        Platform.BATTLENET: config.BADGE_BN,
    }.get(p, config.GREY)


def _wrap(text: str, font: pygame.font.Font, max_w: int) -> list[str]:
    words, lines, cur = text.split(), [], ""
    for w in words:
        test = (cur + " " + w).strip()
        if font.size(test)[0] <= max_w:
            cur = test
        else:
            if cur:
                lines.append(cur)
            cur = w
    if cur:
        lines.append(cur)
    return lines


class Sidebar:
    """
    Right-side panel: art, title, platform, playtime, hint row with PS5 icons.
    """

    HINT_ICON_SIZE = 18

    def __init__(self, rect: pygame.Rect):
        self.rect = rect
        self._game: Optional[Game] = None
        self._art_surf: Optional[pygame.Surface] = None
        self._alpha_tw = Tween(0.0, 255.0, 0.22)
        self._alpha = 0.0
        self._init_fonts()

    def _init_fonts(self):
        w = max(1, self.rect.width)
        try:
            self._title_font = pygame.font.SysFont(
                "Inter,DejaVuSans,Liberation Sans,sans", max(14, w // 12), bold=True
            )
            self._meta_font = pygame.font.SysFont(
                "Inter,DejaVuSans,Liberation Sans,sans", max(11, w // 18)
            )
            self._hint_font = pygame.font.SysFont(
                "Inter,DejaVuSans,Liberation Sans,sans", max(10, w // 22)
            )
            self._badge_font = pygame.font.SysFont(
                "Inter,DejaVuSans,Liberation Sans,sans", max(9, w // 24), bold=True
            )
        except Exception:
            self._title_font = pygame.font.Font(None, max(18, w // 10))
            self._meta_font = pygame.font.Font(None, max(14, w // 16))
            self._hint_font = pygame.font.Font(None, max(12, w // 20))
            self._badge_font = pygame.font.Font(None, max(11, w // 22))

    def set_game(self, game: Optional[Game]):
        if game is self._game:
            return
        self._game = game
        self._art_surf = None
        self._alpha_tw.reset(0.0, 255.0, 0.20)

    def resize(self, rect: pygame.Rect):
        self.rect = rect
        self._art_surf = None
        self._init_fonts()

    def update(self, dt: float):
        self._alpha_tw.update(dt)
        self._alpha = self._alpha_tw.value

        if self._game and self._art_surf is None:
            import data.art as art_mod

            tw = self.rect.width - 20
            th = int(tw * 900 / 600)
            self._art_surf = art_mod.get_surface(self._game, tw, th)

    def draw(self, surface: pygame.Surface, show_hidden: bool = False):
        if self._game is None:
            return

        panel = pygame.Surface(self.rect.size, pygame.SRCALPHA)
        panel.fill((*config.BG_PANEL, 242))

        # Left separator
        pygame.draw.line(
            panel, config.BORDER_BG, (0, 16), (0, self.rect.height - 16), 1
        )

        w = self.rect.width
        y = 12

        # ── Art ─────────────────────────────────────────────────────────────
        art_w = w - 20
        art_h = int(art_w * 900 / 600)
        if self._art_surf:
            a = self._art_surf
            if a.get_size() != (art_w, art_h):
                a = pygame.transform.smoothscale(a, (art_w, art_h))
            panel.blit(a, (10, y))
            pygame.draw.rect(
                panel, config.BORDER_BG, pygame.Rect(10, y, art_w, art_h), 1
            )
        y += art_h + 12

        # ── Platform badge ───────────────────────────────────────────────────
        plat_label = self._game.display_platform_label().upper()
        bs = self._badge_font.render(plat_label, True, config.BG_DEEP)
        bc = _colour_for_platform(self._game.platform)
        bw, bh = bs.get_width() + 12, bs.get_height() + 6
        pygame.draw.rect(panel, bc, pygame.Rect(10, y, bw, bh))
        panel.blit(bs, (16, y + 3))
        y += bh + 6

        # via Lutris label
        if self._game.launch_via_lutris:
            vs = self._badge_font.render("via Lutris", True, config.BADGE_LUTRIS)
            panel.blit(vs, (10, y))
            y += vs.get_height() + 4

        # Hidden badge
        if hidden_store.is_hidden(self._game.slug):
            hs = self._badge_font.render("HIDDEN", True, config.BG_DEEP)
            pygame.draw.rect(
                panel,
                config.MAGENTA,
                pygame.Rect(10, y, hs.get_width() + 10, hs.get_height() + 5),
            )
            panel.blit(hs, (15, y + 2))
            y += hs.get_height() + 8

        y += 4

        # ── Title ────────────────────────────────────────────────────────────
        for line in _wrap(self._game.name, self._title_font, w - 16)[:3]:
            ts = self._title_font.render(line, True, config.WHITE)
            panel.blit(ts, (10, y))
            y += ts.get_height() + 2
        y += 6

        # ── Year ─────────────────────────────────────────────────────────────
        if self._game.year:
            ys = self._meta_font.render(str(self._game.year), True, config.GREY)
            panel.blit(ys, (10, y))
            y += ys.get_height() + 4

        # ── Playtime ─────────────────────────────────────────────────────────
        if self._game.platform != Platform.LUTRIS:
            pt = self._game.playtime_str()
            if pt:
                pts = self._meta_font.render(f"Playtime: {pt}", True, config.CYAN)
                panel.blit(pts, (10, y))
                y += pts.get_height() + 4

        # ── F1 hint ──────────────────────────────────────────────────────────
        f1_label = self._hint_font.render("F1  Keybinds", True, config.GREY)
        panel.blit(f1_label, (10, self.rect.height - f1_label.get_height() - 10))

        panel.set_alpha(int(self._alpha))
        surface.blit(panel, self.rect.topleft)
