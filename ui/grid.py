"""
Scrollable portrait game grid — fully dynamic sizing.
Cols and card dimensions recompute from the live rect every frame.
"""

from __future__ import annotations
import pygame

import config
from data.game import Game
import data.art as art
from ui.animations import lerp


class Grid:
    """
    Resizable scrollable grid of portrait game cards.
    Columns auto-adjust to fill available width using a target card width.
    """

    # Target card width — actual width may be wider to fill evenly
    TARGET_CARD_W = 190
    MIN_COLS = 2
    MAX_COLS = 8
    GAP = 20  # increased from 12 for more breathing room

    def __init__(self, games: list[Game], rect: pygame.Rect):
        self.games = games
        self.rect = rect
        self.selected = 0
        self._scroll_y = 0.0
        self._target_y = 0.0

        self._font_size = 0
        self._title_font = None
        self._last_card_w = 0  # detect card size changes to re-cache fonts
        self.dirty = True  # flag for dirty rendering

    # -----------------------------------------------------------------------
    # Dynamic layout — all derived from self.rect live
    # -----------------------------------------------------------------------

    @property
    def cols(self) -> int:
        """Auto column count: fit as many target-width cards as possible."""
        avail = self.rect.width - self.GAP
        cols = avail // (self.TARGET_CARD_W + self.GAP)
        return max(self.MIN_COLS, min(self.MAX_COLS, cols))

    @property
    def card_w(self) -> int:
        """Actual card width: fill available space evenly across cols."""
        avail = self.rect.width - self.GAP * (self.cols + 1)
        return max(80, avail // self.cols)

    @property
    def card_h(self) -> int:
        return int(self.card_w * 900 / 600)

    @property
    def rows(self) -> int:
        return max(1, (len(self.games) + self.cols - 1) // self.cols)

    @property
    def total_height(self) -> int:
        return self.rows * (self.card_h + self.GAP) + self.GAP

    def _ensure_fonts(self):
        cw = self.card_w
        if cw == self._last_card_w:
            return
        self._last_card_w = cw
        size = max(10, cw // 14)
        try:
            self._title_font = pygame.font.SysFont(
                "Inter,DejaVuSans,Liberation Sans,sans", size
            )
        except Exception:
            self._title_font = pygame.font.Font(None, size + 4)

    @property
    def _h_offset(self) -> int:
        """Horizontal offset to center all columns within the rect."""
        used = self.cols * self.card_w + (self.cols + 1) * self.GAP
        return max(0, (self.rect.width - used) // 2)

    @property
    def _v_offset(self) -> int:
        """Vertical offset to center all rows when content fits the viewport."""
        if self.total_height < self.rect.height:
            return max(0, (self.rect.height - self.total_height) // 2)
        return 0

    def _card_rect(self, index: int) -> pygame.Rect:
        """Card rect in grid-local coordinates (before scroll applied)."""
        row = index // self.cols
        col = index % self.cols
        x = self.rect.x + self._h_offset + self.GAP + col * (self.card_w + self.GAP)
        y = self._v_offset + self.GAP + row * (self.card_h + self.GAP)
        return pygame.Rect(x, y, self.card_w, self.card_h)

    # -----------------------------------------------------------------------
    # Selection / scroll
    # -----------------------------------------------------------------------

    def move(self, dx: int, dy: int):
        total = len(self.games)
        if not total:
            return

        # Calculate current row and column
        cur_row = self.selected // self.cols
        cur_col = self.selected % self.cols

        if dx != 0:
            # Horizontal wrap: stays within the same row bounds
            # Row length might be shorter on the last row
            row_start = cur_row * self.cols
            row_end = min(total, row_start + self.cols)
            row_length = row_end - row_start

            # Wrap offset
            col_in_row = self.selected - row_start
            new_col = (col_in_row + dx) % row_length
            new_idx = row_start + new_col
        elif dy != 0:
            # Vertical wrap: keeps the same column index, wraps rows
            # Find the total rows containing this column index
            matching_indices = [i for i in range(total) if (i % self.cols) == cur_col]
            if matching_indices:
                try:
                    cur_v_idx = matching_indices.index(self.selected)
                    new_v_idx = (cur_v_idx + dy) % len(matching_indices)
                    new_idx = matching_indices[new_v_idx]
                except ValueError:
                    new_idx = self.selected
            else:
                new_idx = self.selected
        else:
            new_idx = self.selected

        if new_idx != self.selected:
            self.selected = new_idx
            self._ensure_visible()
            self.dirty = True

    def page(self, direction: int):
        old = self.selected
        visible_rows = max(1, self.rect.height // (self.card_h + self.GAP))
        self.selected = max(
            0,
            min(
                len(self.games) - 1,
                self.selected + direction * self.cols * visible_rows,
            ),
        )
        if self.selected != old:
            self._ensure_visible()
            self.dirty = True

    def scroll_by(self, amount: float):
        """Scroll the grid by pixel amount (for mouse wheel)."""
        self._target_y += amount
        max_scroll = max(0.0, float(self.total_height - self.rect.height))
        self._target_y = max(0.0, min(self._target_y, max_scroll))
        self.dirty = True

    def click_at(self, mx: int, my: int) -> int | None:
        """Return the game index at screen position (mx, my), or None."""
        if not self.rect.collidepoint(mx, my):
            return None
        cw = self.card_w
        ch = self.card_h
        scroll = int(self._scroll_y)
        for idx in range(len(self.games)):
            local = self._card_rect(idx)
            sx = local.x
            sy = local.y - scroll + self.rect.y
            if sy + ch < self.rect.y:
                continue
            if sy > self.rect.y + self.rect.height:
                break
            if pygame.Rect(sx, sy, cw, ch).collidepoint(mx, my):
                return idx
        return None

    def _ensure_visible(self):
        r = self._card_rect(self.selected)
        if r.y - self._target_y < self.GAP:
            self._target_y = r.y - self.GAP
        bottom_gap = self.GAP
        if r.y + self.card_h - self._target_y > self.rect.height - bottom_gap:
            self._target_y = r.y + self.card_h - self.rect.height + bottom_gap
        max_scroll = max(0.0, float(self.total_height - self.rect.height))
        self._target_y = max(0.0, min(self._target_y, max_scroll))

    def set_games(self, games: list[Game]):
        self.games = games
        self.selected = 0
        self._scroll_y = 0.0
        self._target_y = 0.0
        self.dirty = True

    def resize(self, rect: pygame.Rect):
        self.rect = rect
        # Invalidate art cache so cards reload at new size
        art.clear_all()
        self._last_card_w = 0  # force font reinit
        self._ensure_visible()
        self.dirty = True

    # -----------------------------------------------------------------------
    # Update / draw
    # -----------------------------------------------------------------------

    def update(self, dt: float) -> bool:
        """Update scroll animation. Returns True if grid needs redraw."""
        speed = config.get("ui", "animation_speed")
        old_scroll = self._scroll_y
        self._scroll_y = lerp(self._scroll_y, self._target_y, speed, dt)
        self._ensure_fonts()
        if abs(self._scroll_y - old_scroll) > 0.5:
            self.dirty = True
        return self.dirty

    def draw(self, surface: pygame.Surface):
        if not self.games:
            self._draw_empty(surface)
            self.dirty = False
            return

        cw = self.card_w
        ch = self.card_h
        scroll = int(self._scroll_y)

        old_clip = surface.get_clip()
        surface.set_clip(self.rect.inflate(-2, -2))

        for idx, game in enumerate(self.games):
            local = self._card_rect(idx)
            sx = local.x
            sy = local.y - scroll + self.rect.y

            if sy + ch < self.rect.y:
                continue
            if sy > self.rect.y + self.rect.height:
                break

            self._draw_card(
                surface,
                game,
                pygame.Rect(sx, sy, cw, ch),
                selected=(idx == self.selected),
            )

        surface.set_clip(old_clip)
        self.dirty = False

    def _draw_card(
        self, surface: pygame.Surface, game: Game, rect: pygame.Rect, selected: bool
    ):
        cw, ch = self.card_w, self.card_h

        # Fetch surface at current card size
        card_surf = art.get_surface(game, cw, ch)

        # Scale-up selected card (1.06× for more punchy zoom)
        if selected:
            nw = int(cw * 1.06)
            nh = int(ch * 1.06)
            card_surf = pygame.transform.smoothscale(card_surf, (nw, nh))
            draw_rect = pygame.Rect(
                rect.x - (nw - cw) // 2,
                rect.y - (nh - ch) // 2,
                nw,
                nh,
            )

            # Draw multi-layered soft bloom shadow glow
            for i in range(8, 0, -2):
                glow_rect = draw_rect.inflate(i, i)
                glow_surf = pygame.Surface(glow_rect.size, pygame.SRCALPHA)
                # Outer glow layers get more transparent
                alpha = int(45 * (1 - i / 10))
                pygame.draw.rect(
                    glow_surf,
                    (*config.VIOLET, alpha),
                    glow_surf.get_rect(),
                    border_radius=4,
                )
                surface.blit(glow_surf, glow_rect.topleft)
        else:
            draw_rect = rect
            # Dim unselected further for better focus separation
            card_surf = card_surf.copy()
            dim = pygame.Surface(card_surf.get_size(), pygame.SRCALPHA)
            dim.fill((0, 0, 0, 95))
            card_surf.blit(dim, (0, 0))

        surface.blit(card_surf, draw_rect.topleft)

        # Border: violet for selected, black for unselected
        if selected:
            pygame.draw.rect(surface, config.VIOLET, draw_rect, 3, border_radius=2)
            # Add inline cyan highlight line to card top edge
            pygame.draw.line(
                surface,
                config.CYAN,
                (draw_rect.x + 2, draw_rect.y + 1),
                (draw_rect.x + draw_rect.width - 3, draw_rect.y + 1),
                2,
            )
        else:
            pygame.draw.rect(surface, config.BORDER_BG, draw_rect, 1)

    def _draw_empty(self, surface: pygame.Surface):
        try:
            font = pygame.font.SysFont("Inter,DejaVuSans,sans", 24)
        except Exception:
            font = pygame.font.Font(None, 28)
        txt = font.render("No games found — loading library...", True, config.GREY)
        cx = self.rect.x + self.rect.width // 2 - txt.get_width() // 2
        cy = self.rect.y + self.rect.height // 2 - txt.get_height() // 2
        surface.blit(txt, (cx, cy))

    @property
    def selected_game(self) -> Game | None:
        if self.games and 0 <= self.selected < len(self.games):
            return self.games[self.selected]
        return None
