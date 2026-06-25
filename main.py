#!/usr/bin/env python3
"""
corpse-launcher — main entry point.

Usage:
  python main.py
  python main.py --dry-run
"""

from __future__ import annotations
import os
import sys
import time
import threading

# ── Display detection before pygame ─────────────────────────────────────────
if "SDL_VIDEODRIVER" not in os.environ:
    if os.environ.get("WAYLAND_DISPLAY"):
        os.environ["SDL_VIDEODRIVER"] = "wayland"
    elif os.environ.get("DISPLAY"):
        os.environ["SDL_VIDEODRIVER"] = "x11"

os.environ.setdefault("SDL_RENDER_DRIVER",              "opengl")
os.environ.setdefault("SDL_JOYSTICK_HIDAPI_PS5",        "1")
os.environ.setdefault("SDL_JOYSTICK_HIDAPI_PS5_RUMBLE", "1")
os.environ.setdefault("SDL_VIDEO_ALLOW_SCREENSAVER",    "0")

import pygame

import config
import data.library as library
import data.hidden  as hidden_store
import launcher as game_launcher
from controller.input import InputHandler, Action
from ui.grid    import Grid
from ui.sidebar import Sidebar
from ui.details import DetailsOverlay
from ui.keybinds import KeybindsOverlay


def _sidebar_w(win_w: int) -> int:
    return max(220, min(380, int(win_w * 0.22)))


# ---------------------------------------------------------------------------
# DRY RUN
# ---------------------------------------------------------------------------

def dry_run():
    games = library.load()
    print(f"\n{'─'*60}")
    print(f"  corpse-launcher — dry run  ({len(games)} games)")
    print(f"{'─'*60}")
    for g in games:
        via     = " [Lutris]" if g.launch_via_lutris else ""
        has_art = " [art]"    if g.art_path else ""
        hid     = " [hidden]" if hidden_store.is_hidden(g.slug) else ""
        print(f"  {g.name:<40} {g.platform.value:<10}{via}{has_art}{hid}")
    print(f"{'─'*60}\n")


# ---------------------------------------------------------------------------
# App
# ---------------------------------------------------------------------------

class App:
    TARGET_FPS = config.get("general", "fps")
    IDLE_FPS   = 15   # throttle when nothing is animating

    def __init__(self):
        try:
            pygame.mixer.pre_init(22050, -16, 2, 512)
        except Exception:
            pass
        pygame.init()
        pygame.display.set_caption("☠  corpse-launcher")

        # Compile procedural navigation tick sound
        self.nav_sound = None
        try:
            import numpy as np
            sample_rate = 22050
            duration = 0.03
            t = np.linspace(0, duration, int(sample_rate * duration), False)
            snd_arr = np.sin(2 * np.pi * 320 * t) * 0.12
            snd_arr[-int(len(t)*0.5):] *= np.linspace(1.0, 0.0, int(len(t)*0.5))
            self.nav_sound = pygame.sndarray.make_sound((snd_arr * 32767).astype(np.int16))
        except Exception:
            pass

        w, h = config.window_size()
        self.screen = pygame.display.set_mode(
            (w, h), pygame.RESIZABLE | pygame.DOUBLEBUF)
        self.clock    = pygame.time.Clock()
        self.running  = True

        self._all_games:  list = []   # full library (including hidden)
        self._loading     = True
        self._status      = "Loading..."
        self._show_hidden = False     # toggle: show hidden entries in grid
        self._needs_draw  = True      # dirty flag for rendering

        self._win_size = self.screen.get_size()

        sw = _sidebar_w(w)
        self.grid    = Grid([],  pygame.Rect(0,      0, w - sw, h))
        self.sidebar = Sidebar(  pygame.Rect(w - sw, 0, sw,     h))
        self.details = DetailsOverlay(
            screen_size=(w, h),
            on_launch=self._do_launch,
        )
        self.keybinds = KeybindsOverlay(screen_size=(w, h))
        self.input = InputHandler()

        threading.Thread(target=self._load_library, daemon=True).start()

    # -----------------------------------------------------------------------

    def _load_library(self):
        games = library.load()
        self._all_games = games
        self._loading   = False
        self._status    = ""
        self._pending_refresh = True
        self._needs_draw = True

    def _visible_games(self):
        """Return games filtered by show_hidden toggle."""
        if self._show_hidden:
            return [g for g in self._all_games
                    if hidden_store.is_hidden(g.slug)]
        return [g for g in self._all_games
                if not hidden_store.is_hidden(g.slug)]

    def _refresh_grid(self):
        visible = self._visible_games()
        self.grid.set_games(visible)
        self.sidebar.set_game(self.grid.selected_game)
        self._needs_draw = True

    # -----------------------------------------------------------------------

    def _layout(self):
        w, h = self.screen.get_size()
        sw   = _sidebar_w(w)
        self.grid.resize(   pygame.Rect(0,      0, w - sw, h))
        self.sidebar.resize(pygame.Rect(w - sw, 0, sw,     h))
        self.details.resize(w, h)
        self.keybinds.resize(w, h)
        self._needs_draw = True

    # -----------------------------------------------------------------------

    def _do_launch(self, game):
        # Play confirm synth sound
        try:
            if pygame.mixer.get_init():
                # Procedural positive confirm sound (rising tone)
                import numpy as np
                sample_rate = 22050
                duration = 0.25
                t = np.linspace(0, duration, int(sample_rate * duration), False)
                # Frequency sweep from 440Hz to 880Hz
                frequencies = np.linspace(440, 880, len(t))
                snd_arr = np.sin(2 * np.pi * frequencies * t) * 0.18
                # Fade out last 30%
                fade_len = int(len(t) * 0.3)
                snd_arr[-fade_len:] *= np.linspace(1.0, 0.0, fade_len)
                audio_data = (snd_arr * 32767).astype(np.int16)
                sound = pygame.sndarray.make_sound(audio_data)
                sound.play()
        except Exception:
            pass
        game_launcher.launch(game)

    # -----------------------------------------------------------------------

    def _handle_action(self, action: Action):
        self._needs_draw = True   # any action probably causes a visual change

        # ── Keybinds overlay absorbs everything when open ─────────────────
        if self.keybinds.is_open:
            if action in (Action.HELP, Action.BACK, Action.QUIT):
                self.keybinds.close()
            return

        # ── HELP toggles keybinds overlay from anywhere ───────────────────
        if action == Action.HELP:
            self.keybinds.toggle()
            return

        # ── Details overlay absorbs navigation when open ──────────────────
        if self.details.is_open:
            match action:
                case Action.NAV_UP:
                    self.details.handle_up()
                    if self.nav_sound: self.nav_sound.play()
                case Action.NAV_DOWN:
                    self.details.handle_down()
                    if self.nav_sound: self.nav_sound.play()
                case Action.CONFIRM:
                    should_close = self.details.handle_confirm()
                    if should_close:
                        self.details.close()
                    # Play back thud on confirm/close
                    try:
                        if pygame.mixer.get_init():
                            import numpy as np
                            t = np.linspace(0, 0.1, int(22050 * 0.1), False)
                            snd_arr = np.sin(2 * np.pi * 180 * t) * 0.12
                            snd_arr[-int(len(t)*0.5):] *= np.linspace(1.0, 0.0, int(len(t)*0.5))
                            pygame.sndarray.make_sound((snd_arr * 32767).astype(np.int16)).play()
                    except Exception:
                        pass
                    self._refresh_grid()   # visibility may have changed (hide/unhide)
                case Action.BACK | Action.DETAILS | Action.QUIT:
                    self.details.close()
                    try:
                        if pygame.mixer.get_init():
                            import numpy as np
                            t = np.linspace(0, 0.15, int(22050 * 0.15), False)
                            snd_arr = np.sin(2 * np.pi * 150 * t) * 0.15
                            snd_arr[-int(len(t)*0.6):] *= np.linspace(1.0, 0.0, int(len(t)*0.6))
                            pygame.sndarray.make_sound((snd_arr * 32767).astype(np.int16)).play()
                    except Exception:
                        pass
                case Action.MOUSE_CLICK | Action.MOUSE_DBLCLICK:
                    pass  # ignore mouse clicks inside overlay for now
            return

        # ── Normal grid navigation ─────────────────────────────────────────
        prev = self.grid.selected

        match action:
            case Action.QUIT:
                self.running = False

            case Action.BACK:
                try:
                    if pygame.mixer.get_init():
                        # Play procedural "thud" sound for backing out
                        import numpy as np
                        t = np.linspace(0, 0.15, int(22050 * 0.15), False)
                        snd_arr = np.sin(2 * np.pi * 150 * t) * 0.15
                        snd_arr[-int(len(t)*0.6):] *= np.linspace(1.0, 0.0, int(len(t)*0.6))
                        pygame.sndarray.make_sound((snd_arr * 32767).astype(np.int16)).play()
                except Exception:
                    pass

            case Action.CONFIRM:
                g = self.grid.selected_game
                if g:
                    self._do_launch(g)

            case Action.DETAILS:
                g = self.grid.selected_game
                if g:
                    self.details.open(g)

            case Action.HIDE:
                g = self.grid.selected_game
                if g:
                    now_hidden = hidden_store.toggle(g.slug)
                    self._refresh_grid()
                    # Status message
                    self._status = f"{'Hidden' if now_hidden else 'Unhidden'}: {g.name}"
                    threading.Timer(2.0, self._clear_status).start()

            case Action.SHOW_HIDDEN:
                self._show_hidden = not self._show_hidden
                self._refresh_grid()
                label = "Showing hidden games" if self._show_hidden else "Hiding hidden games"
                self._status = label
                threading.Timer(2.0, self._clear_status).start()

            case Action.NAV_UP:
                self.grid.move(0, -1)
                if self.nav_sound: self.nav_sound.play()
            case Action.NAV_DOWN:
                self.grid.move(0,  1)
                if self.nav_sound: self.nav_sound.play()
            case Action.NAV_LEFT:
                self.grid.move(-1, 0)
                if self.nav_sound: self.nav_sound.play()
            case Action.NAV_RIGHT:
                self.grid.move( 1, 0)
                if self.nav_sound: self.nav_sound.play()

            case Action.PAGE_LEFT:
                self.grid.page(-1)
                if self.nav_sound: self.nav_sound.play()
            case Action.PAGE_RIGHT:
                self.grid.page( 1)
                if self.nav_sound: self.nav_sound.play()

            case Action.SCROLL_UP:
                self.grid.scroll_by(-60)
                if self.nav_sound: self.nav_sound.play()

            case Action.SCROLL_DOWN:
                self.grid.scroll_by(60)
                if self.nav_sound: self.nav_sound.play()

            case Action.MOUSE_CLICK:
                mx, my = self.input.mouse_pos
                idx = self.grid.click_at(mx, my)
                if idx is not None:
                    if idx == self.grid.selected:
                        # Second click on same = open details
                        g = self.grid.selected_game
                        if g:
                            self.details.open(g)
                    else:
                        self.grid.selected = idx
                        self.grid.dirty = True
                        self.sidebar.set_game(self.grid.selected_game)

            case Action.MOUSE_DBLCLICK:
                mx, my = self.input.mouse_pos
                idx = self.grid.click_at(mx, my)
                if idx is not None:
                    self.grid.selected = idx
                    self.grid.dirty = True
                    g = self.grid.selected_game
                    if g:
                        self._do_launch(g)

            case Action.REFRESH:
                self._loading = True
                self._status  = "Refreshing..."
                library.invalidate()
                threading.Thread(target=self._load_library, daemon=True).start()

        if self.grid.selected != prev:
            self.sidebar.set_game(self.grid.selected_game)

    def _clear_status(self):
        self._status = ""
        self._needs_draw = True

    # -----------------------------------------------------------------------

    def run(self):
        self.IDLE_FPS = 5  # Lower idle throttle to 5 FPS to reduce CPU usage
        while self.running:
            # Determine FPS — throttle to IDLE_FPS when nothing is animating
            sidebar_anim = (self.sidebar._alpha_tw and not self.sidebar._alpha_tw.done)
            details_anim = (self.details.is_open and self.details._alpha_tw and not self.details._alpha_tw.done)
            grid_scroll_anim = (abs(self.grid._scroll_y - self.grid._target_y) > 0.5)

            animating = (self._needs_draw or self._loading
                         or self.details.is_open
                         or self.keybinds.is_open
                         or sidebar_anim
                         or details_anim
                         or grid_scroll_anim)
            fps = self.TARGET_FPS if animating else self.IDLE_FPS
            dt = min(self.clock.tick(fps) / 1000.0, 0.05)

            # Resize detection
            cur = self.screen.get_size()
            if cur != self._win_size:
                self._win_size = cur
                self._layout()

            # Pending library load
            if getattr(self, "_pending_refresh", False):
                del self._pending_refresh
                self._refresh_grid()

            # Input
            actions = []
            try:
                actions = self.input.poll(dt)
            except Exception as e:
                print(f"[input] Poll warning (ignoring during transition): {e}")
            for action in actions:
                self._handle_action(action)

            # Update
            grid_dirty = self.grid.update(dt)
            self.sidebar.update(dt)
            self.details.update(dt)

            # Draw only when needed
            sidebar_fading = (self.sidebar._alpha_tw and not self.sidebar._alpha_tw.done)
            details_fading = (self.details.is_open and self.details._alpha_tw and not self.details._alpha_tw.done)
            if self._needs_draw or grid_dirty or sidebar_fading or details_fading:
                self.screen.fill(config.BG)
                self.grid.draw(self.screen)
                self.sidebar.draw(self.screen, show_hidden=self._show_hidden)
                self.details.draw(self.screen)
                self.keybinds.draw(self.screen)

                if self._loading:
                    _draw_loading(self.screen, self._status)
                elif self._status:
                    _draw_status(self.screen, self._status)

                # Optimised updates: use dirty rect updates for grid and sidebar if no fullscreen overlays are active
                if self.details.is_open or self.keybinds.is_open or self._loading:
                    pygame.display.flip()
                else:
                    # Update only grid and sidebar regions
                    pygame.display.update([self.grid.rect, self.sidebar.rect])
                self._needs_draw = False

        pygame.quit()


# ---------------------------------------------------------------------------
# Overlays
# ---------------------------------------------------------------------------

def _draw_loading(surface: pygame.Surface, msg: str = "Loading..."):
    w, h = surface.get_size()
    veil  = pygame.Surface((w, h), pygame.SRCALPHA)
    veil.fill((0, 0, 0, 160))
    surface.blit(veil, (0, 0))
    try:
        font = pygame.font.SysFont("Inter,DejaVuSans,Liberation Sans,sans", 24, bold=True)
    except Exception:
        font = pygame.font.Font(None, 30)
    dots = "." * (int(time.time() * 2) % 4)
    txt  = font.render(f"{msg}{dots}", True, config.CYAN)
    surface.blit(txt, (w // 2 - txt.get_width() // 2,
                       h // 2 - txt.get_height() // 2))


def _draw_status(surface: pygame.Surface, msg: str):
    """Small status pill at bottom-left."""
    try:
        font = pygame.font.SysFont("Inter,DejaVuSans,Liberation Sans,sans", 14)
    except Exception:
        font = pygame.font.Font(None, 18)
    txt = font.render(msg, True, config.CYAN)
    pad = 8
    w   = txt.get_width()  + pad * 2
    h   = txt.get_height() + pad
    sw, sh = surface.get_size()
    bg = pygame.Surface((w, h), pygame.SRCALPHA)
    bg.fill((*config.BG_PANEL, 200))
    pygame.draw.rect(bg, config.BORDER_BG, bg.get_rect(), 1)
    surface.blit(bg,  (12, sh - h - 12))
    surface.blit(txt, (12 + pad, sh - h - 12 + pad // 2))


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

if __name__ == "__main__":
    if "--dry-run" in sys.argv:
        dry_run()
    else:
        App().run()
