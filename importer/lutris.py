"""
Lutris library importer.
Primary source: ~/.local/share/lutris/pga.db (SQLite)
Fallback: glob *.yml from games dirs.
Deduplication: same slug → keep entry with newest mtime / highest id.
"""

from __future__ import annotations
import os
import re
import sqlite3
import yaml
from pathlib import Path
from typing import Optional

import config
from data.game import Game, Platform


# ---------------------------------------------------------------------------
# Runner → Platform mapping
# ---------------------------------------------------------------------------
_RUNNER_PLATFORM: dict[str, Platform] = {
    "steam":     Platform.STEAM,
    "gog":       Platform.GOG,
    "epic":      Platform.EPIC,
    "battlenet": Platform.BATTLENET,
    "lutris":    Platform.LUTRIS,
    "wine":      Platform.LUTRIS,
    "dosbox":    Platform.LUTRIS,
    "native":    Platform.LUTRIS,
    "scummvm":   Platform.LUTRIS,
    "ppsspp":    Platform.LUTRIS,
}

# Slugs that indicate Lutris-wrapped Steam games
_STEAM_RUNNER = {"steam"}


def _detect_platform(runner: str, slug: str) -> Platform:
    if runner in _RUNNER_PLATFORM:
        return _RUNNER_PLATFORM[runner]
    if "battlenet" in slug or "battlenet" in runner:
        return Platform.BATTLENET
    if "gog" in slug:
        return Platform.GOG
    return Platform.LUTRIS


def _normalise_slug(raw: str) -> str:
    """Strip trailing timestamp from lutris slugs like 'diablo-iv-1751745148'."""
    return re.sub(r"-\d{7,}$", "", raw.strip())


def _find_lutris_art(slug: str) -> Optional[Path]:
    """Check for local cover art by slug."""
    for covers_dir in (config.LUTRIS_COVERS_DIR,):
        if not covers_dir.exists():
            continue
        for ext in (".jpg", ".jpeg", ".png", ".webp"):
            p = covers_dir / f"{slug}{ext}"
            if p.exists():
                return p
    return None


# ---------------------------------------------------------------------------
# pga.db import (primary)
# ---------------------------------------------------------------------------

def _import_from_pga(db_path: Path) -> list[Game]:
    games: list[Game] = []
    try:
        con = sqlite3.connect(db_path, check_same_thread=False)
        con.row_factory = sqlite3.Row
        cur = con.execute(
            """
            SELECT id, name, slug, runner, directory, playtime,
                   lastplayed, year, installed, configpath
            FROM   games
            WHERE  installed = 1
            ORDER  BY id ASC
            """
        )
        rows = cur.fetchall()
        con.close()
    except Exception as e:
        print(f"[lutris] Warning: could not read pga.db: {e}")
        return games

    for row in rows:
        raw_slug  = str(row["slug"] or "")
        norm_slug = _normalise_slug(raw_slug)
        name      = str(row["name"] or "").strip()
        runner    = str(row["runner"] or "").lower()
        if not name:
            continue

        platform = _detect_platform(runner, norm_slug)
        art      = _find_lutris_art(norm_slug) or _find_lutris_art(raw_slug)

        install_dir = None
        d = row["directory"]
        if d:
            install_dir = Path(d)

        # Extract Steam appid from slug like "steam-1091500" or from configpath
        steam_appid: Optional[str] = None
        if runner in _STEAM_RUNNER:
            m = re.search(r"(\d{4,})", norm_slug)
            if m:
                steam_appid = m.group(1)
            # also check configpath
            cfg_path = row["configpath"] or ""
            m2 = re.search(r"(\d{4,})", cfg_path)
            if m2 and not steam_appid:
                steam_appid = m2.group(1)

        game = Game(
            slug             = norm_slug or f"lutris-{row['id']}",
            name             = name,
            platform         = platform,
            steam_appid      = steam_appid,
            lutris_id        = int(row["id"]),
            lutris_slug      = raw_slug,
            runner           = runner,
            art_path         = art,
            install_dir      = install_dir,
            playtime_minutes = int(row["playtime"] or 0),
            last_played      = int(row["lastplayed"] or 0),
            year             = row["year"],
            launch_via_lutris = True,
            lutris_launch_id  = int(row["id"]),
            _mtime            = float(row["id"]),   # higher id = newer entry
        )
        games.append(game)

    print(f"[lutris] pga.db: found {len(games)} installed games.")
    return games


# ---------------------------------------------------------------------------
# YAML fallback
# ---------------------------------------------------------------------------

def _import_from_yaml(games_dirs: list[Path]) -> list[Game]:
    games: list[Game] = []
    for games_dir in games_dirs:
        if not games_dir.exists():
            continue
        for yml_path in sorted(games_dir.glob("*.yml"),
                               key=lambda p: p.stat().st_mtime):
            try:
                data = yaml.safe_load(yml_path.read_text(encoding="utf-8"))
            except Exception:
                continue
            if not isinstance(data, dict):
                continue

            name     = str(data.get("name", "")).strip()
            raw_slug = str(data.get("game_slug") or data.get("slug") or "")
            runner   = str(data.get("runner") or "").lower()
            if not name:
                continue

            norm_slug = _normalise_slug(raw_slug) or _normalise_slug(yml_path.stem)
            platform  = _detect_platform(runner, norm_slug)
            art       = _find_lutris_art(norm_slug)

            game = Game(
                slug             = norm_slug or yml_path.stem,
                name             = name,
                platform         = platform,
                runner           = runner,
                art_path         = art,
                year             = data.get("year"),
                launch_via_lutris = True,
                _mtime            = yml_path.stat().st_mtime,
            )
            games.append(game)

    print(f"[lutris] YAML fallback: found {len(games)} games.")
    return games


# ---------------------------------------------------------------------------
# Deduplication
# ---------------------------------------------------------------------------

def _deduplicate(games: list[Game]) -> list[Game]:
    """
    For each normalised slug, keep the entry with the highest _mtime
    (newest pga.db id or yaml mtime).
    """
    best: dict[str, Game] = {}
    for g in games:
        if g.slug not in best or g._mtime > best[g.slug]._mtime:
            best[g.slug] = g
    return list(best.values())


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------

def import_games() -> list[Game]:
    """
    Return a deduplicated list of Lutris games.
    Uses pga.db if available, otherwise falls back to YAML.
    """
    games: list[Game] = []

    if config.LUTRIS_PGA_DB.exists():
        games = _import_from_pga(config.LUTRIS_PGA_DB)
    else:
        games = _import_from_yaml([
            config.LUTRIS_GAMES_DIR,
            config.LUTRIS_GAMES_DIR2,
        ])

    deduped = _deduplicate(games)
    print(f"[lutris] After dedup: {len(deduped)} unique games.")
    return deduped
