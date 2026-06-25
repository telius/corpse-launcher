"""
Steam library importer.
Parses libraryfolders.vdf → appmanifest_*.acf to enumerate installed games.
Also reads userdata/<steamid>/config/localconfig.vdf for playtime.
"""

from __future__ import annotations
import re
from pathlib import Path
from typing import Optional

import vdf

import config
from data.game import Game, Platform


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _steam_root() -> Optional[Path]:
    candidates = [
        config.STEAM_ROOT,
        Path.home() / ".local" / "share" / "Steam",
        Path.home() / ".steam" / "steam",
    ]
    for p in candidates:
        if p.exists():
            return p
    return None


def _load_library_folders(steam_root: Path) -> list[Path]:
    """Return list of steamapps directories from libraryfolders.vdf."""
    lf_path = steam_root / "config" / "libraryfolders.vdf"
    if not lf_path.exists():
        return [steam_root / "steamapps"]

    try:
        data = vdf.load(open(lf_path, encoding="utf-8", errors="replace"))
        root_key = data.get("libraryfolders") or data.get("LibraryFolders") or {}
        dirs: list[Path] = []
        for key, val in root_key.items():
            if not key.isdigit():
                continue
            if isinstance(val, dict):
                path_str = val.get("path", "")
            elif isinstance(val, str):
                path_str = val
            else:
                continue
            p = Path(path_str) / "steamapps"
            if p.exists():
                dirs.append(p)
        if not dirs:
            dirs.append(steam_root / "steamapps")
        return dirs
    except Exception as e:
        print(f"[steam] Warning: could not parse libraryfolders.vdf: {e}")
        return [steam_root / "steamapps"]


def _read_acf(acf_path: Path) -> Optional[dict]:
    """Parse a single appmanifest_*.acf file into a dict."""
    try:
        data = vdf.load(open(acf_path, encoding="utf-8", errors="replace"))
        return data.get("AppState") or data.get("appstate")
    except Exception as e:
        print(f"[steam] Warning: could not parse {acf_path.name}: {e}")
        return None


def _load_playtimes(steam_root: Path) -> dict[str, int]:
    """Return {appid: playtime_minutes} from userdata localconfig.vdf files."""
    playtimes: dict[str, int] = {}
    userdata = steam_root / "userdata"
    if not userdata.exists():
        return playtimes

    for uid_dir in userdata.iterdir():
        lc = uid_dir / "config" / "localconfig.vdf"
        if not lc.exists():
            continue
        try:
            data = vdf.load(open(lc, encoding="utf-8", errors="replace"),
                            mapper=vdf.VDFDict)
            apps = (data
                    .get("UserLocalConfigStore", {})
                    .get("Software", {})
                    .get("Valve", {})
                    .get("Steam", {})
                    .get("apps", {}))
            for appid, info in apps.items():
                if isinstance(info, dict):
                    pt = int(info.get("Playtime", 0) or 0)
                    lp = int(info.get("LastPlayed", 0) or 0)
                    if pt or lp:
                        playtimes[str(appid)] = pt
        except Exception:
            pass
    return playtimes


def _find_steam_art(appid: str) -> Optional[Path]:
    """
    Look for portrait (600×900) grid art in the Steam art cache.
    Steam stores covers as  <hash>_library_600x900.jpg / .png  or
    <appid>_library_600x900.jpg depending on version.
    """
    art_dir = config.STEAM_ART_CACHE
    if not art_dir.exists():
        return None

    # Try the modern naming scheme first
    for suffix in (
        f"{appid}_library_600x900.jpg",
        f"{appid}_library_600x900.png",
        f"{appid}_library_hero.jpg",
        f"{appid}_header.jpg",
    ):
        p = art_dir / suffix
        if p.exists():
            return p

    # Older: hash-named files — can't map without extra metadata, skip
    return None


def _make_slug(name: str, appid: str) -> str:
    """Create a dedup slug from the game name."""
    slug = name.lower()
    slug = re.sub(r"[^a-z0-9]+", "-", slug)
    slug = slug.strip("-")
    return slug or f"steam-{appid}"


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------

def import_games() -> list[Game]:
    """
    Return a list of Game objects for every installed Steam game.
    """
    steam_root = _steam_root()
    if steam_root is None:
        print("[steam] Steam installation not found.")
        return []

    libraries   = _load_library_folders(steam_root)
    playtimes   = _load_playtimes(steam_root)
    games: list[Game] = []

    for steamapps in libraries:
        for acf_path in sorted(steamapps.glob("appmanifest_*.acf")):
            data = _read_acf(acf_path)
            if data is None:
                continue

            appid   = str(data.get("appid", ""))
            name    = str(data.get("name", "")).strip()
            state   = int(data.get("StateFlags", 0) or 0)

            # Skip non-installed, tools, soundtracks, etc.
            if not name or appid in ("228980", "1070560"):   # Steamworks common
                continue
            # StateFlags: 4 = fully installed
            if state not in (4, 6, 1030):
                continue
            # Skip Proton, Steam Runtime, and other tools
            _SKIP_PREFIXES = ("Proton", "Steam Linux Runtime", "Steamworks",
                              "Steam VR", "SteamVR")
            if any(name.startswith(p) for p in _SKIP_PREFIXES):
                continue


            install_dir_str = data.get("installdir", "")
            install_dir = steamapps / "common" / install_dir_str if install_dir_str else None

            slug = _make_slug(name, appid)
            art  = _find_steam_art(appid)

            game = Game(
                slug             = slug,
                name             = name,
                platform         = Platform.STEAM,
                steam_appid      = appid,
                runner           = "steam",
                art_path         = art,
                install_dir      = install_dir,
                playtime_minutes = playtimes.get(appid, 0),
                last_played      = 0,  # could read from localconfig too
                _mtime           = acf_path.stat().st_mtime,
            )
            games.append(game)

    print(f"[steam] Imported {len(games)} installed games.")
    return games
