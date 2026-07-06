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
        self._prev_art_surf: Optional[pygame.Surface] = None
        self._alpha_tw = Tween(0.0, 255.0, 0.22)
        self._alpha = 0.0
        self._clicked_url = None
        self._click_timer = 0.0
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
        # Store current art as previous art for cross-fade transition
        if self._art_surf:
            self._prev_art_surf = self._art_surf
        self._game = game
        self._art_surf = None
        self._alpha_tw.reset(0.0, 255.0, 0.20)

    def resize(self, rect: pygame.Rect):
        self.rect = rect
        self._art_surf = None
        self._prev_art_surf = None
        self._init_fonts()

    def update(self, dt: float):
        self._alpha_tw.update(dt)
        self._alpha = self._alpha_tw.value

        if self._click_timer > 0.0:
            self._click_timer = max(0.0, self._click_timer - dt)
            if self._click_timer <= 0.0:
                self._clicked_url = None

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

        # ── Art (Cross-fading) ────────────────────────────────────────────────
        art_w = w - 20
        art_h = int(art_w * 900 / 600)
        
        # We draw a composite block for the art
        art_container = pygame.Surface((art_w, art_h), pygame.SRCALPHA)
        
        # Calculate cross-fade blend percentage (0.0 to 1.0)
        blend_factor = self._alpha / 255.0

        # Draw the previous art first (fading out)
        if self._prev_art_surf and blend_factor < 1.0:
            prev_scaled = self._prev_art_surf
            if prev_scaled.get_size() != (art_w, art_h):
                prev_scaled = pygame.transform.smoothscale(prev_scaled, (art_w, art_h))
                self._prev_art_surf = prev_scaled
            
            # Draw with inverse alpha
            prev_temp = prev_scaled.copy()
            alpha_surf = pygame.Surface((art_w, art_h), pygame.SRCALPHA)
            alpha_surf.fill((255, 255, 255, int((1.0 - blend_factor) * 255)))
            prev_temp.blit(alpha_surf, (0, 0), special_flags=pygame.BLEND_RGBA_MULT)
            art_container.blit(prev_temp, (0, 0))

        # Draw the new art (fading in)
        if self._art_surf:
            curr_scaled = self._art_surf
            if curr_scaled.get_size() != (art_w, art_h):
                curr_scaled = pygame.transform.smoothscale(curr_scaled, (art_w, art_h))
                self._art_surf = curr_scaled
                
            curr_temp = curr_scaled.copy()
            alpha_surf = pygame.Surface((art_w, art_h), pygame.SRCALPHA)
            alpha_surf.fill((255, 255, 255, int(blend_factor * 255)))
            curr_temp.blit(alpha_surf, (0, 0), special_flags=pygame.BLEND_RGBA_MULT)
            art_container.blit(curr_temp, (0, 0))
            
        panel.blit(art_container, (10, y))
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
        # Check for cached IGDB metadata for rich info
        import data.igdb as igdb
        igdb_meta = igdb.get_cached_metadata(self._game.slug)

        display_year = self._game.year
        if igdb_meta and igdb_meta.get("release_year") and (not display_year or display_year == 1996):
            display_year = igdb_meta["release_year"]

        if display_year:
            ys = self._meta_font.render(str(display_year), True, config.GREY)
            panel.blit(ys, (10, y))
            y += ys.get_height() + 4

        # ── Playtime ─────────────────────────────────────────────────────────
        if self._game.platform != Platform.LUTRIS:
            pt = self._game.playtime_str()
            if pt:
                pts = self._meta_font.render(f"Playtime: {pt}", True, config.CYAN)
                panel.blit(pts, (10, y))
                y += pts.get_height() + 4

        # ── IGDB Rich Metadata (Genres / Developer / Summary) ────────────────
        if igdb_meta:
            # 1. Developer / Company
            companies = igdb_meta.get("companies")
            if companies:
                comp_str = f"Developer: {', '.join(companies)}"
                comps = self._meta_font.render(comp_str, True, config.GREY)
                panel.blit(comps, (10, y))
                y += comps.get_height() + 4

            # 2. Genres
            genres = igdb_meta.get("genres")
            if genres:
                genres_str = f"Genres: {', '.join(genres)}"
                gens = self._meta_font.render(genres_str, True, config.GREY)
                panel.blit(gens, (10, y))
                y += gens.get_height() + 4

            # 3. Summary (brief 2-line snippet)
            summary = igdb_meta.get("summary", "").strip()
            if summary:
                # Wrap description inside sidebar width boundaries
                y += 2
                wrapped_lines = _wrap(summary, self._hint_font, w - 20)
                for line in wrapped_lines[:3]:  # Limit to 3 lines to fit static links
                    desc_surf = self._hint_font.render(line, True, config.GREY)
                    panel.blit(desc_surf, (10, y))
                    y += desc_surf.get_height() + 2
                y += 4

        # ── Web Links (Static position near bottom) ──────────────────────────
        self._link_rects = []  # list of (Rect, URL_string)
        links = []
        
        # Standardize checking against lowercased slugs or titles
        slug = self._game.slug.lower()
        title = self._game.name.lower()
        
        if "world of warcraft" in title:
            links.append(("wowhead", "https://www.wowhead.com"))
            links.append(("mmo-champion", "https://www.mmo-champion.com"))
            links.append(("raider.io", "https://raider.io"))
            links.append(("archon", "https://www.archon.gg"))
            links.append(("raidbots", "https://www.raidbots.com"))
        elif "diablo" in title:
            if "ii" in title or "2" in title:
                links.append(("maxroll diablo ii", "https://maxroll.gg/d2"))
            elif "iii" in title or "3" in title:
                links.append(("maxroll diablo iii", "https://maxroll.gg/d3"))
            elif "iv" in title or "4" in title:
                links.append(("maxroll diablo iv", "https://maxroll.gg/d4"))
            else:
                links.append(("maxroll", "https://maxroll.gg"))
        elif "path of exile 2" in title or "poe2" in slug:
            links.append(("path of exile 2", "https://www.pathofexile.com/poe2"))
            links.append(("maxroll poe 2", "https://maxroll.gg/poe2"))
        elif "path of exile" in title or "poe" in slug:
            links.append(("path of exile", "https://www.pathofexile.com"))
            links.append(("maxroll poe", "https://maxroll.gg/poe"))
        elif "satisfactory" in title:
            links.append(("satisfactory calculator", "https://www.satisfactory-calculator.com"))
            links.append(("satisfactory tools", "https://www.satisfactorytools.com"))

        # Add Steam app store/community links if it's a Steam game (independent of title matches)
        if self._game.platform == Platform.STEAM and self._game.steam_appid:
            appid = self._game.steam_appid
            links.append(("steam store", f"https://store.steampowered.com/app/{appid}"))
            links.append(("steam community", f"https://steamcommunity.com/app/{appid}"))
        elif not links:  # Only fall back to generic Lutris / PCGW if no specific game links were added
            if self._game.platform == Platform.LUTRIS:
                lutris_slug = self._game.lutris_slug or self._game.slug
                links.append(("lutris page", f"https://lutris.net/games/{lutris_slug}"))
                links.append(("pcgamingwiki", f"https://www.pcgamingwiki.com/api/appid.php?appid={self._game.steam_appid}" if self._game.steam_appid else f"https://www.pcgamingwiki.com/w/index.php?search={self._game.name}"))

        if links:
            # We place the links at a static position above the F1 hint (which is at height - 30)
            # Let's count backwards so it is strictly static and unaffected by title length wraps.
            default_color = (102, 0, 255)  # #6600ff less prominent
            hover_color = (0, 255, 255)    # #00FFFF
            active_color = (255, 0, 255)   # #FF00FF
            
            f1_height = 30
            spacing = 24  # Increased vertical spacing
            # Calculate start Y going upwards based on link count
            total_link_h = len(links) * spacing
            link_start_y = self.rect.height - f1_height - total_link_h - 15
            
            # Fetch mouse pos relative to absolute screen to check hover bounds
            mx, my = pygame.mouse.get_pos()
            
            for i, (name, url) in enumerate(links):
                lnk_y = link_start_y + (i * spacing)
                
                # Check absolute bounding box of link to set hover/active color state
                click_rect = pygame.Rect(
                    self.rect.x + 10,
                    self.rect.y + lnk_y,
                    0,  # dynamic width will be assigned after font sizing
                    20  # approx height
                )
                
                # Determine current rendering color based on hover/active states
                color = default_color
                if self._clicked_url == url and self._click_timer > 0.0:
                    color = active_color
                elif click_rect.copy().inflate(200, 0).collidepoint(mx, my):
                    # Compute actual width first for precise hover collision
                    test_surf = self._meta_font.render(name, True, default_color)
                    click_rect.width = test_surf.get_width()
                    click_rect.height = test_surf.get_height()
                    if click_rect.collidepoint(mx, my):
                        color = hover_color
                
                lnk_surf = self._meta_font.render(name, True, color)
                click_rect.width = lnk_surf.get_width()
                click_rect.height = lnk_surf.get_height()
                
                panel.blit(lnk_surf, (10, lnk_y))
                self._link_rects.append((click_rect, url))

        # ── F1 hint ──────────────────────────────────────────────────────────
        f1_label = self._hint_font.render("F1  Keybinds", True, config.GREY)
        panel.blit(f1_label, (10, self.rect.height - f1_label.get_height() - 10))

        panel.set_alpha(int(self._alpha))
        surface.blit(panel, self.rect.topleft)

    def handle_click(self, mx: int, my: int) -> bool:
        """Checks if a link was clicked. If so, opens it in a browser and returns True."""
        if not hasattr(self, "_link_rects") or not self._link_rects:
            return False
        for rect, url in self._link_rects:
            if rect.collidepoint(mx, my):
                # Set active clicked state
                self._clicked_url = url
                self._click_timer = 0.15  # highlight active color for 150ms
                import webbrowser
                try:
                    webbrowser.open(url)
                    return True
                except Exception as e:
                    print(f"[sidebar] Failed to open link {url}: {e}")
        return False

