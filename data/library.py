"""
Unified game library — merges Steam + Lutris entries and deduplicates.

Merge rules:
  1. Same slug → merge into single entry.
  2. Steam + Lutris same game → prefer Lutris for launching (user's choice).
  3. Lutris-wrapped Steam entry (runner="steam") with matching steam_appid
     → merge with native Steam entry, mark launch_via_lutris=True.
  4. Sort by name (default), playtime, or last_played.
"""

from __future__ import annotations
import re
from typing import Optional

from data.game import Game, Platform
import importer.steam  as steam_importer
import importer.lutris as lutris_importer
import config


# ---------------------------------------------------------------------------
# Slug normalisation (shared)
# ---------------------------------------------------------------------------

_ARTICLES = re.compile(r"^(the|a|an)\s+", re.IGNORECASE)

def _canonical_slug(name: str) -> str:
    """
    Create a normalised slug from a display name for fuzzy matching.
    Strips leading articles, lowercases, removes punctuation.
    """
    s = _ARTICLES.sub("", name.lower())
    s = re.sub(r"[^a-z0-9]+", "-", s).strip("-")
    return s


# ---------------------------------------------------------------------------
# Merge logic
# ---------------------------------------------------------------------------

def _merge(steam_games: list[Game], lutris_games: list[Game]) -> list[Game]:
    """
    Produce a single deduplicated list, preferring Lutris for launching
    when the same game appears in both sources.
    """
    result: dict[str, Game] = {}

    # Index steam games by slug and by appid
    steam_by_slug:  dict[str, Game] = {}
    steam_by_appid: dict[str, Game] = {}
    for g in steam_games:
        steam_by_slug[g.slug] = g
        if g.steam_appid:
            steam_by_appid[g.steam_appid] = g

    # Index lutris games by slug and by steam_appid (if runner=steam)
    lutris_by_slug:  dict[str, Game] = {}
    lutris_by_appid: dict[str, Game] = {}
    for g in lutris_games:
        lutris_by_slug[g.slug] = g
        if g.steam_appid and g.runner == "steam":
            lutris_by_appid[g.steam_appid] = g

    # Start with Steam games
    for sg in steam_games:
        # Check if Lutris has an entry for the same Steam appid
        lg_appid: Optional[Game] = lutris_by_appid.get(sg.steam_appid or "")
        # Check by slug
        lg_slug:  Optional[Game] = lutris_by_slug.get(sg.slug)
        lg = lg_appid or lg_slug

        if lg:
            # Merge: keep Steam metadata (name, appid, art fallbacks),
            # but route launch through Lutris
            merged = Game(
                slug             = sg.slug,
                name             = sg.name or lg.name,
                platform         = Platform.STEAM,
                steam_appid      = sg.steam_appid,
                lutris_id        = lg.lutris_id,
                lutris_slug      = lg.lutris_slug,
                runner           = sg.runner,
                art_path         = sg.art_path or lg.art_path,
                install_dir      = sg.install_dir or lg.install_dir,
                playtime_minutes = sg.playtime_minutes or lg.playtime_minutes,
                last_played      = sg.last_played,
                year             = sg.year or lg.year,
                launch_via_lutris = True,
                lutris_launch_id  = lg.lutris_id,
                _mtime           = sg._mtime,
            )
            result[merged.slug] = merged
        else:
            result[sg.slug] = sg

    # Add Lutris-only games (not present in Steam)
    for lg in lutris_games:
        # Skip if already merged with a Steam entry
        if lg.slug in result:
            continue
        # Also skip pure lutris-wrapped steam entries already merged by appid
        if lg.steam_appid and lg.steam_appid in steam_by_appid:
            continue
        result[lg.slug] = lg

    return list(result.values())


# ---------------------------------------------------------------------------
# Sorting
# ---------------------------------------------------------------------------

def _sort(games: list[Game]) -> list[Game]:
    sort_by = config.get("general", "sort_by")
    match sort_by:
        case "playtime":
            return sorted(games, key=lambda g: g.playtime_minutes, reverse=True)
        case "last_played":
            return sorted(games, key=lambda g: g.last_played, reverse=True)
        case _:  # "name"
            return sorted(games, key=lambda g: g.name.lower())


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------

_library_cache: Optional[list[Game]] = None

def load(force: bool = False) -> list[Game]:
    """
    Load and merge the complete game library.
    Results are cached in memory after the first call.
    Pass force=True to reimport.
    """
    global _library_cache
    if _library_cache is not None and not force:
        return _library_cache

    print("[library] Importing Steam games...")
    steam_games  = steam_importer.import_games()

    print("[library] Importing Lutris games...")
    lutris_games = lutris_importer.import_games()

    print("[library] Merging libraries...")
    merged = _merge(steam_games, lutris_games)
    merged = _sort(merged)

    print(f"[library] Total unique games: {len(merged)}")
    _library_cache = merged
    return merged


def invalidate():
    """Force reimport on next load()."""
    global _library_cache
    _library_cache = None
