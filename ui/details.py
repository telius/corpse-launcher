"""
Details / action overlay — shown when Triangle is pressed on a selected game.

Shows:
  - Large game art
  - Title, platform, year, playtime
  - Action list: Launch (Cross), Hide/Unhide (Square), Close (Circle/Triangle)

Navigation: D-pad up/down to select action, Cross to confirm.
"""

from __future__ import annotations
from typing import Optional, Callable
import pygame

import config
from data.game import Game, Platform
import data.art as art_module
import data.hidden as hidden_store
import ui.icons as icons
from ui.animations import Tween


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


class DetailsOverlay:
    """
    Full-screen translucent overlay with game details and action menu.
    Call open(game) to show, close() to hide.
    is_open property indicates visibility.
    on_launch callback is invoked when user selects Launch.
    """

    def __init__(self, screen_size: tuple[int, int], on_launch: Callable[[Game], None]):
        self._screen_w, self._screen_h = screen_size
        self._on_launch = on_launch
        self._game: Optional[Game] = None
        self._art_surf: Optional[pygame.Surface] = None
        self._open = False
        self._alpha_tw = Tween(0.0, 255.0, 0.18)
        self._alpha = 0.0
        self._action_idx = 0  # 0=Launch, 1=Hide/Unhide

        self._init_fonts()

    def _init_fonts(self):
        try:
            self._title_font = pygame.font.SysFont(
                "Inter,DejaVuSans,Liberation Sans,sans", 32, bold=True
            )
            self._meta_font = pygame.font.SysFont(
                "Inter,DejaVuSans,Liberation Sans,sans", 18
            )
            self._action_font = pygame.font.SysFont(
                "Inter,DejaVuSans,Liberation Sans,sans", 20, bold=True
            )
            self._hint_font = pygame.font.SysFont(
                "Inter,DejaVuSans,Liberation Sans,sans", 15
            )
        except Exception:
            self._title_font = pygame.font.Font(None, 38)
            self._meta_font = pygame.font.Font(None, 22)
            self._action_font = pygame.font.Font(None, 24)
            self._hint_font = pygame.font.Font(None, 18)

    def resize(self, w: int, h: int):
        self._screen_w, self._screen_h = w, h
        self._art_surf = None
        self._init_fonts()

    # -----------------------------------------------------------------------

    def open(self, game: Game):
        self._game = game
        self._open = True
        self._art_surf = None
        self._action_idx = 0
        self._alpha_tw.reset(0.0, 255.0, 0.18)

    def close(self):
        self._open = False
        self._game = None

    @property
    def is_open(self) -> bool:
        return self._open

    # -----------------------------------------------------------------------
    # Input — called by main loop when overlay is open

    def handle_up(self):
        self._action_idx = (self._action_idx - 1) % self._num_actions()

    def handle_down(self):
        self._action_idx = (self._action_idx + 1) % self._num_actions()

    def handle_confirm(self) -> bool:
        """Execute selected action. Returns True if overlay should close."""
        if not self._game:
            return True
        actions = self._build_actions()
        if 0 <= self._action_idx < len(actions):
            label, fn = actions[self._action_idx]
            fn()
            if label in ("Next Cover Art", "Add Custom Art"):
                return False
        return True  # always close after action

    def _num_actions(self) -> int:
        return len(self._build_actions())

    def _build_actions(self) -> list[tuple[str, Callable]]:
        if not self._game:
            return []
        is_hid = hidden_store.is_hidden(self._game.slug)
        actions = [
            ("Launch game", lambda: self._on_launch(self._game)),
            (
                "Unhide game" if is_hid else "Hide game",
                lambda: hidden_store.toggle(self._game.slug),
            ),
            ("Next Cover Art", self._trigger_swap_art),
            ("Add Custom Art", self._trigger_select_custom_art),
        ]
        return actions

    def _trigger_swap_art(self):
        import threading

        def worker():
            if art_module.swap_to_next_art(self._game):
                self._art_surf = None  # Force reload

        threading.Thread(target=worker, daemon=True).start()

    def _trigger_select_custom_art(self):
        import subprocess
        import shutil
        import threading
        from pathlib import Path

        def file_picker():
            try:
                # Open zenity file selection dialog
                res = subprocess.run(
                    [
                        "zenity",
                        "--file-selection",
                        "--title=Select Custom Art",
                        "--file-filter=Images (png, jpg, jpeg, webp) | *.png *.jpg *.jpeg *.webp",
                    ],
                    capture_output=True,
                    text=True,
                )
                if res.returncode == 0:
                    src_path = Path(res.stdout.strip())
                    if src_path.exists():
                        dest_dir = config.CONFIG_DIR / "custom_art"
                        dest_dir.mkdir(parents=True, exist_ok=True)
                        dest_path = (
                            dest_dir / f"{self._game.slug}{src_path.suffix.lower()}"
                        )

                        # Copy the file
                        shutil.copy2(src_path, dest_path)

                        # Invalidate cache
                        art_module.invalidate(self._game.slug)
                        self._art_surf = None  # Force reload details image
            except Exception as e:
                print(f"[details] Error picking custom art: {e}")

        threading.Thread(target=file_picker, daemon=True).start()

    # -----------------------------------------------------------------------

    def update(self, dt: float):
        if not self._open or not self._game:
            return
        self._alpha_tw.update(dt)
        self._alpha = self._alpha_tw.value

        # Load art
        if self._game and self._art_surf is None:
            art_w = min(300, self._screen_w // 4)
            art_h = int(art_w * 900 / 600)
            self._art_surf = art_module.get_surface(self._game, art_w, art_h)

    def draw(self, surface: pygame.Surface):
        if not self._open or not self._game:
            return

        sw, sh = self._screen_w, self._screen_h
        alpha = int(self._alpha)

        # ── Dark veil ───────────────────────────────────────────────────────
        veil = pygame.Surface((sw, sh), pygame.SRCALPHA)
        veil.fill((0, 0, 0, min(alpha, 200)))
        surface.blit(veil, (0, 0))

        # ── Card panel ──────────────────────────────────────────────────────
        pw, ph = min(720, sw - 80), min(600, sh - 80)

        # Calculate horizontal slide-in transition offset
        progress = alpha / 255.0  # 0.0 -> 1.0
        slide_offset = int((1.0 - progress) * 60)  # slide in by 60px

        px = (sw - pw) // 2 + slide_offset
        py = (sh - ph) // 2

        panel = pygame.Surface((pw, ph), pygame.SRCALPHA)
        panel.fill((*config.BG_PANEL, min(alpha, 245)))

        # Black top border line
        pygame.draw.line(panel, config.BORDER_BG, (0, 0), (pw, 0), 3)

        # ── Art (left column) ───────────────────────────────────────────────
        art_col_w = pw // 3
        art_w = art_col_w - 20
        art_h = int(art_w * 900 / 600)
        art_y = (ph - art_h) // 2

        art_surf = self._art_surf
        if art_surf:
            a = pygame.transform.smoothscale(art_surf, (art_w, art_h))
            panel.blit(a, (10, art_y))
            pygame.draw.rect(
                panel, config.BORDER_BG, pygame.Rect(10, art_y, art_w, art_h), 1
            )

        # ── Info + actions (right column) ────────────────────────────────────
        rx = art_col_w + 10
        rw = pw - rx - 16
        ry = 20

        # Platform badge
        plat = self._game.display_platform_label().upper()
        badge_c = {
            Platform.STEAM: config.BADGE_STEAM,
            Platform.LUTRIS: config.BADGE_LUTRIS,
            Platform.BATTLENET: config.BADGE_BN,
        }.get(self._game.platform, config.GREY)
        bs = self._meta_font.render(plat, True, config.BG_DEEP)
        bw = bs.get_width() + 12
        bh = bs.get_height() + 6
        pygame.draw.rect(panel, badge_c, pygame.Rect(rx, ry, bw, bh))
        panel.blit(bs, (rx + 6, ry + 3))
        ry += bh + 10

        if self._game.launch_via_lutris:
            via = self._meta_font.render("via Lutris", True, config.BADGE_LUTRIS)
            panel.blit(via, (rx, ry))
            ry += via.get_height() + 4

        # Title
        for line in _wrap(self._game.name, self._title_font, rw)[:3]:
            ts = self._title_font.render(line, True, config.WHITE)
            panel.blit(ts, (rx, ry))
            ry += ts.get_height() + 2
        ry += 8

        # Year / Playtime
        if self._game.year:
            ys = self._meta_font.render(str(self._game.year), True, config.GREY)
            panel.blit(ys, (rx, ry))
            ry += ys.get_height() + 4

        if self._game.platform != Platform.LUTRIS:
            pt = self._game.playtime_str()
            if pt:
                pts = self._meta_font.render(f"Playtime: {pt}", True, config.CYAN)
                panel.blit(pts, (rx, ry))
                ry += pts.get_height() + 4

        if hidden_store.is_hidden(self._game.slug):
            hs = self._meta_font.render("HIDDEN", True, config.MAGENTA)
            panel.blit(hs, (rx, ry))
            ry += hs.get_height() + 4

        ry += 16

        # ── Action list ──────────────────────────────────────────────────────
        actions = self._build_actions()
        action_icon_map = ["cross", "square", "triangle", "options"]

        for i, (label, _) in enumerate(actions):
            selected = i == self._action_idx
            row_h = self._action_font.get_height() + 10
            row_rect = pygame.Rect(rx - 4, ry - 4, rw + 8, row_h + 8)

            if selected:
                sel_bg = pygame.Surface(row_rect.size, pygame.SRCALPHA)
                sel_bg.fill((*config.VIOLET, 60))
                panel.blit(sel_bg, row_rect.topleft)
                pygame.draw.rect(panel, config.BORDER_BG, row_rect, 1)

            icon_name = action_icon_map[i] if i < len(action_icon_map) else "cross"
            icons.draw_hint(
                panel,
                rx + 2,
                ry + 2,
                icon_name,
                label,
                self._action_font,
                icon_size=22,
                colour=config.WHITE if selected else config.GREY,
                gap=8,
            )
            ry += row_h + 8

        # ── Bottom hint bar ──────────────────────────────────────────────────
        hint_y = ph - self._hint_font.get_height() - 14
        pygame.draw.line(
            panel, config.BORDER_BG, (rx, hint_y - 8), (pw - 10, hint_y - 8), 1
        )

        cx = rx
        for icon_name, label in [("dpad", "Navigate"), ("circle", "Close")]:
            icon_surf = icons.get(icon_name, 16)
            if icon_surf:
                panel.blit(icon_surf, (cx, hint_y + 2))
                cx += 20
            ts = self._hint_font.render(label, True, config.GREY)
            panel.blit(ts, (cx, hint_y))
            cx += ts.get_width() + 20

        # ── Panel border ─────────────────────────────────────────────────────
        pygame.draw.rect(panel, config.BORDER_BG, panel.get_rect(), 2)

        panel.set_alpha(alpha)
        surface.blit(panel, (px, py))
