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

    @property
    def GAP(self) -> int:
        return config.grid_gap()

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
        # Cache animation speed — avoid config dict lookup every frame
        self._anim_speed: float = config.get("ui", "animation_speed")
        # Empty-state font — created lazily and reused
        self._empty_font: pygame.font.Font | None = None

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

    def hover_at(self, mx: int, my: int) -> int | None:
        """Return the game index at screen position (mx, my) in O(1) time, or None."""
        if not self.rect.collidepoint(mx, my):
            return None
        
        col_w = self.card_w + self.GAP
        row_h = self.card_h + self.GAP
        
        col = (mx - self.rect.x - self._h_offset - self.GAP) // col_w
        row = (my - self.rect.y - self._v_offset - self.GAP + int(self._scroll_y)) // row_h
        
        if col < 0 or col >= self.cols or row < 0:
            return None
            
        idx = row * self.cols + col
        if idx >= len(self.games):
            return None
            
        # Verify mouse is actually inside the card rect boundaries (and not in the gaps)
        card_r = self._card_rect(idx)
        card_r.y = card_r.y - int(self._scroll_y) + self.rect.y
        if card_r.collidepoint(mx, my):
            return idx
        return None

    def click_at(self, mx: int, my: int) -> int | None:
        """Alias/wrapper for hover_at to maintain backwards compatibility."""
        return self.hover_at(mx, my)

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
        old_scroll = self._scroll_y
        self._scroll_y = lerp(self._scroll_y, self._target_y, self._anim_speed, dt)
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

        # Scale-up selected card (1.06× for more punchy zoom)
        if selected:
            nw = int(cw * 1.06)
            nh = int(ch * 1.06)
            draw_rect = pygame.Rect(
                rect.x - (nw - cw) // 2,
                rect.y - (nh - ch) // 2,
                nw,
                nh,
            )
            
            padding = max(4, nw // 24)
            art_w = nw - padding * 2
            art_h = nh - padding * 2

            # Cache the scaled surface to avoid calling smoothscale every frame
            if (
                not hasattr(self, "_zoom_cache")
                or self._zoom_cache is None
                or getattr(self, "_zoom_cache_slug", None) != game.slug
                or getattr(self, "_zoom_cache_size", None) != (art_w, art_h)
            ):
                self._zoom_cache_slug = game.slug
                self._zoom_cache_size = (art_w, art_h)
                raw_art = art.get_surface(game, cw, ch)
                self._zoom_cache = pygame.transform.smoothscale(raw_art, (art_w, art_h))
            card_surf = self._zoom_cache

            # Pre-render/cache glow surfaces to avoid creating them every frame
            if (
                not hasattr(self, "_glow_surfs")
                or self._glow_surfs is None
                or getattr(self, "_glow_surfs_size", None) != draw_rect.size
            ):
                self._glow_surfs = []
                self._glow_surfs_size = draw_rect.size
                for i in range(8, 0, -2):
                    g_rect = draw_rect.inflate(i, i)
                    glow_surf = pygame.Surface(g_rect.size, pygame.SRCALPHA)
                    alpha = int(45 * (1 - i / 10))
                    pygame.draw.rect(
                        glow_surf,
                        (*config.VIOLET, alpha),
                        glow_surf.get_rect(),
                        border_radius=4,
                    )
                    self._glow_surfs.append((glow_surf, i))

            for glow_surf, i in self._glow_surfs:
                glow_rect = draw_rect.inflate(i, i)
                surface.blit(glow_surf, glow_rect.topleft)
        else:
            draw_rect = rect
            padding = max(4, cw // 24)
            art_w = cw - padding * 2
            art_h = ch - padding * 2
            card_surf = art.get_surface(game, art_w, art_h)

        # Draw the card container background (very dark slate grey)
        pygame.draw.rect(surface, config.BG_CARD, draw_rect, border_radius=6)

        # Draw the card surface inset within the container
        surface.blit(card_surf, (draw_rect.x + padding, draw_rect.y + padding))

        # Dim unselected cards to keep focus on selected card
        if not selected:
            if (
                not hasattr(self, "_dim_surf")
                or self._dim_surf is None
                or self._dim_surf.get_size() != (cw, ch)
            ):
                self._dim_surf = pygame.Surface((cw, ch), pygame.SRCALPHA)
                self._dim_surf.fill((0, 0, 0, 95))
            surface.blit(self._dim_surf, draw_rect.topleft)

        # Border: cyan for selected, border_bg for unselected
        if selected:
            pygame.draw.rect(surface, config.CYAN, draw_rect, 1, border_radius=6)
        else:
            pygame.draw.rect(surface, config.BORDER_BG, draw_rect, 1, border_radius=6)

    def _draw_empty(self, surface: pygame.Surface):
        if self._empty_font is None:
            try:
                self._empty_font = pygame.font.SysFont("Inter,DejaVuSans,sans", 24)
            except Exception:
                self._empty_font = pygame.font.Font(None, 28)
        txt = self._empty_font.render("No games found — loading library...", True, config.GREY)
        cx = self.rect.x + self.rect.width // 2 - txt.get_width() // 2
        cy = self.rect.y + self.rect.height // 2 - txt.get_height() // 2
        surface.blit(txt, (cx, cy))

    @property
    def selected_game(self) -> Game | None:
        if self.games and 0 <= self.selected < len(self.games):
            return self.games[self.selected]
        return None
