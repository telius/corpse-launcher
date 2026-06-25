"""
Art loading, scaling, caching, and background fetch for game cards.

Key design:
  - Art is stored on disk at native resolution; surfaces are scaled per-request.
  - Two caches: _disk_cache (path known) and _surface_cache (rendered Surface).
  - Placeholder surfaces are NOT cached to _surface_cache so real art
    always replaces them as soon as it arrives from SGDB.
  - Background SGDB fetch runs once per slug; sets game.art_path when done.
  - get_surface(game, w, h) takes explicit dimensions so resize works correctly.
"""

from __future__ import annotations
import json
import threading
from pathlib import Path
from typing import Optional
from collections import OrderedDict

import pygame

import config
from data.game import Game
import data.sgdb as sgdb


# ---------------------------------------------------------------------------
# LRU surface cache  (slug → Surface at a specific size)
# ---------------------------------------------------------------------------


class _LRUCache:
    def __init__(self, maxsize: int = 300):
        self._store: OrderedDict[str, pygame.Surface] = OrderedDict()
        self._max = maxsize
        self._lock = threading.Lock()

    def get(self, key: str) -> Optional[pygame.Surface]:
        with self._lock:
            if key in self._store:
                self._store.move_to_end(key)
                return self._store[key]
            return None

    def put(self, key: str, surf: pygame.Surface):
        with self._lock:
            self._store[key] = surf
            self._store.move_to_end(key)
            if len(self._store) > self._max:
                self._store.popitem(last=False)

    def discard(self, key: str):
        with self._lock:
            self._store.pop(key, None)

    def discard_prefix(self, prefix: str):
        with self._lock:
            for k in list(self._store.keys()):
                if k.startswith(prefix):
                    self._store.pop(k, None)

    def clear(self):
        with self._lock:
            self._store.clear()


# Cache key encodes slug + size so resize produces fresh surfaces
def _cache_key(slug: str, w: int, h: int) -> str:
    return f"{slug}:{w}x{h}"


_surface_cache = _LRUCache(maxsize=400)

# Small placeholder cache — keyed by (slug, w, h) so placeholders aren't re-rendered
# every frame while real art is being fetched. NOT the same as _surface_cache
# (placeholder is evicted as soon as real art arrives via discard_prefix).
_placeholder_cache: dict[str, pygame.Surface] = {}

# Custom art directory — created once at module load, not inside get_surface
_custom_art_dir = config.CONFIG_DIR / "custom_art"
_custom_art_dir.mkdir(parents=True, exist_ok=True)

# Slugs currently being fetched from SGDB in a background thread
_fetching: set[str] = set()
_fetch_lock = threading.Lock()

# Slugs that failed SGDB lookup — don't retry
_sgdb_failed: set[str] = set()


# ---------------------------------------------------------------------------
# Placeholder surface
# ---------------------------------------------------------------------------


def _make_placeholder(game: Game, w: int, h: int) -> pygame.Surface:
    surf = pygame.Surface((w, h), pygame.SRCALPHA)
    surf.fill(config.BG_CARD)

    # Violet gradient overlay (top half)
    for y in range(h // 2):
        alpha = int(90 * (1 - y / max(1, h / 2)))
        line = pygame.Surface((w, 1), pygame.SRCALPHA)
        line.fill((*config.VIOLET, alpha))
        surf.blit(line, (0, y))

    # Border
    pygame.draw.rect(surf, config.BORDER_BG, surf.get_rect(), 2)

    # Diagonal accent line
    pygame.draw.line(surf, (*config.VIOLET, 40), (0, 0), (w, h), 1)

    # Title text
    font_size = max(10, w // 12)
    try:
        font = pygame.font.SysFont(
            "Inter,DejaVuSans,Liberation Sans,sans", font_size, bold=True
        )
    except Exception:
        font = pygame.font.Font(None, font_size + 4)

    words = game.name.split()
    lines: list[str] = []
    current = ""
    for word in words:
        test = (current + " " + word).strip()
        if font.size(test)[0] <= w - 16:
            current = test
        else:
            if current:
                lines.append(current)
            current = word
    if current:
        lines.append(current)

    text_y = h // 2
    for line in lines[:4]:
        rendered = font.render(line, True, config.WHITE)
        surf.blit(rendered, (w // 2 - rendered.get_width() // 2, text_y))
        text_y += rendered.get_height() + 3

    # Platform badge bottom
    badge_size = max(9, w // 16)
    try:
        bfont = pygame.font.SysFont("Inter,DejaVuSans,Liberation Sans,sans", badge_size)
    except Exception:
        bfont = pygame.font.Font(None, badge_size + 3)
    blabel = bfont.render(game.display_platform_label().upper(), True, config.GREY)
    surf.blit(blabel, (w // 2 - blabel.get_width() // 2, h - blabel.get_height() - 8))

    return surf


# ---------------------------------------------------------------------------
# Disk image loader
# ---------------------------------------------------------------------------


def _load_surface(path: Path, w: int, h: int) -> Optional[pygame.Surface]:
    try:
        raw = pygame.image.load(str(path))
        scaled_raw = pygame.transform.smoothscale(raw, (w, h))
        scaled = scaled_raw.convert_alpha()
        return scaled
    except Exception as e:
        print(f"[art] Failed to load {path.name}: {e}")
        return None


# ---------------------------------------------------------------------------
# Background SGDB fetch thread
# ---------------------------------------------------------------------------


def _fetch_in_background(game: Game):
    """Download art from SGDB in a background thread."""
    slug = game.slug
    try:
        print(f"[art] Fetching SGDB art: {game.name!r}")
        path = sgdb.fetch_art(slug, game.steam_appid, game.name)
    except Exception as e:
        print(f"[art] SGDB exception for {game.name!r}: {e}")
        path = None

    with _fetch_lock:
        _fetching.discard(slug)
        if path is None:
            _sgdb_failed.add(slug)
            print(f"[art] SGDB no result: {game.name!r}")
        else:
            print(f"[art] SGDB done: {game.name!r} → {path.name}")

    if path and path.exists():
        # Invalidate any placeholder/stale surfaces for this slug
        _surface_cache.discard_prefix(f"{slug}:")
        game.art_path = path
        game.art_loaded = True


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------


def get_surface(game: Game, w: int, h: int) -> pygame.Surface:
    """
    Return a Surface for game art at size (w, h).
    Always returns immediately — SGDB fetches happen in a background thread.

    Priority:
      1. Cached surface at this exact size  →  return it
      2. Known art_path on disk             →  load, cache, return
      3. SGDB key available + not failed    →  trigger background fetch, return placeholder
      4. Return placeholder (not cached, so real art replaces it as soon as it lands)
    """
    key = _cache_key(game.slug, w, h)

    # 1. Cached surface (real art)
    cached = _surface_cache.get(key)
    if cached is not None:
        return cached

    # 1.5 Custom art directory override (no mkdir — ensured at module load)
    for ext in (".png", ".jpg", ".jpeg", ".webp"):
        p = _custom_art_dir / f"{game.slug}{ext}"
        if p.exists():
            surf = _load_surface(p, w, h)
            if surf:
                _surface_cache.put(key, surf)
                game.art_loaded = True
                return surf
            break  # file exists but failed to load — fall through

    # 2. Known art path on disk
    if game.art_path and game.art_path.exists():
        surf = _load_surface(game.art_path, w, h)
        if surf:
            _surface_cache.put(key, surf)
            game.art_loaded = True
            return surf

    # 3. Trigger SGDB background fetch (once per slug)
    slug = game.slug
    with _fetch_lock:
        if slug not in _fetching and slug not in _sgdb_failed and sgdb.has_key():
            _fetching.add(slug)
            threading.Thread(
                target=_fetch_in_background, args=(game,), daemon=True
            ).start()

    # 4. Return cached placeholder so we don't re-render it every frame
    pk = _cache_key(game.slug, w, h)
    ph = _placeholder_cache.get(pk)
    if ph is None:
        ph = _make_placeholder(game, w, h)
        if len(_placeholder_cache) > 200:
            # Simple eviction: drop the oldest entry
            _placeholder_cache.pop(next(iter(_placeholder_cache)))
        _placeholder_cache[pk] = ph
    return ph


def clear_all():
    """Flush entire surface cache and fetch state (e.g. on window resize)."""
    _surface_cache.clear()
    _placeholder_cache.clear()
    with _fetch_lock:
        _fetching.clear()
        _sgdb_failed.clear()


def invalidate(slug: str):
    """Remove all cached surfaces for a slug (real art + placeholder)."""
    _surface_cache.discard_prefix(f"{slug}:")
    # Also evict placeholder so the refreshed art is picked up immediately
    for k in [k for k in _placeholder_cache if k.startswith(slug + ":")]:
        del _placeholder_cache[k]


def swap_to_next_art(game: Game) -> bool:
    """Fetch next available grid from SGDB, download and overwrite local cache."""

    if not sgdb.has_key():
        return False

    sgdb_id = sgdb.get_sgdb_id(game.name, game.steam_appid)
    if not sgdb_id:
        print(f"[art] Could not resolve SGDB ID for {game.name}")
        return False

    style = config.get("art", "sgdb_style")
    dimensions = config.get("art", "sgdb_dimensions")
    urls = sgdb.fetch_all_grid_urls(sgdb_id, style, dimensions)
    if not urls:
        print(
            f"[art] No grids found matching style={style} dimensions={dimensions} for {game.name}"
        )
        return False

    art_selections_file = config.CONFIG_DIR / "art_selections.json"
    selections = {}
    if art_selections_file.exists():
        try:
            selections = json.loads(art_selections_file.read_text(encoding="utf-8"))
        except Exception:
            pass

    current_url = selections.get(game.slug, "")
    try:
        idx = urls.index(current_url)
    except ValueError:
        idx = 0

    next_idx = (idx + 1) % len(urls)
    next_url = urls[next_idx]

    dest = sgdb._cache_path(game.slug)
    print(
        f"[art] Swapping art for {game.name} from index {idx} to {next_idx} (total {len(urls)})"
    )
    if sgdb.download_url_to_path(next_url, dest):
        selections[game.slug] = next_url
        try:
            art_selections_file.write_text(
                json.dumps(selections, indent=2), encoding="utf-8"
            )
        except Exception as e:
            print(f"[art] Failed to save art selections: {e}")

        # Invalidate cached surfaces
        invalidate(game.slug)
        game.art_path = dest
        game.art_loaded = True
        return True

    return False
