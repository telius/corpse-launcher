"""
SteamGridDB API client.
Fetches 600×900 portrait grid art, preferring style=white_logo.
Caches downloads to config.ART_CACHE / sgdb/.

API docs: https://www.steamgriddb.com/api/v2
"""

from __future__ import annotations
import hashlib
import time
from pathlib import Path
from typing import Optional

import requests

import config

_BASE = "https://www.steamgriddb.com/api/v2"
_TIMEOUT = 10

# ---------------------------------------------------------------------------
# Internal helpers
# ---------------------------------------------------------------------------

def _headers() -> dict:
    key = config.sgdb_api_key()
    if not key:
        raise ValueError(
            "SteamGridDB API key not set.\n"
            "Add sgdb_api_key to ~/.config/corpse-launcher/config.toml\n"
            "Get a free key at: https://www.steamgriddb.com/profile/preferences/api"
        )
    return {"Authorization": f"Bearer {key}"}


def _get(endpoint: str, params: dict = {}) -> Optional[dict]:
    try:
        r = requests.get(f"{_BASE}/{endpoint}", headers=_headers(),
                         params=params, timeout=_TIMEOUT)
        if r.status_code == 200:
            return r.json()
        elif r.status_code == 401:
            print("[sgdb] ❌ Invalid API key — check config.toml")
        elif r.status_code == 404:
            pass  # not found, normal
        else:
            print(f"[sgdb] HTTP {r.status_code} for /{endpoint} params={params}")
    except requests.RequestException as e:
        print(f"[sgdb] Network error: {e}")
    return None


def _cache_path(game_slug: str) -> Path:
    """Return the local cache path for a game's art."""
    safe = hashlib.md5(game_slug.encode()).hexdigest()[:12]
    return config.ART_CACHE / f"{safe}_{game_slug[:32]}.jpg"


def _download(url: str, dest: Path) -> bool:
    """Download a file with progress-less streaming write."""
    try:
        r = requests.get(url, timeout=30, stream=True)
        if r.status_code != 200:
            return False
        dest.parent.mkdir(parents=True, exist_ok=True)
        with open(dest, "wb") as f:
            for chunk in r.iter_content(chunk_size=65536):
                f.write(chunk)
        return True
    except Exception as e:
        print(f"[sgdb] Download failed ({url}): {e}")
        return False


# ---------------------------------------------------------------------------
# Search / lookup
# ---------------------------------------------------------------------------

def _get_game_id_by_steam_appid(appid: str) -> Optional[int]:
    """Get SGDB game id from a Steam appid."""
    data = _get(f"games/steam/{appid}")
    if data and data.get("success") and data.get("data"):
        return data["data"]["id"]
    return None


def _get_game_id_by_name(name: str) -> Optional[int]:
    """Search SGDB by game name, return first result id."""
    data = _get("search/autocomplete/" + requests.utils.quote(name))
    if data and data.get("success") and data.get("data"):
        return data["data"][0]["id"]
    return None


def _fetch_grid_url(sgdb_id: int, style: str, dimensions: str) -> Optional[str]:
    """
    Fetch the best matching grid URL from SGDB for a given game id.
    Tries requested style first, then falls back to any style.
    """
    for styles in ([style], []):  # exact style first, then any
        params: dict = {
            "dimensions": dimensions,
            "mimes":      "image/png,image/jpeg,image/webp",
        }
        if styles:
            params["styles"] = styles[0]

        data = _get(f"grids/game/{sgdb_id}", params)
        if data and data.get("success") and data.get("data"):
            # Prefer non-animated, non-NSFW
            for item in data["data"]:
                if not item.get("nsfw") and not item.get("humor"):
                    url = item.get("url")
                    if url:
                        return url
            # Take first available if nothing clean found
            first = data["data"][0].get("url")
            if first:
                return first
    return None


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------

def fetch_art(game_slug: str,
              steam_appid: Optional[str] = None,
              game_name: str = "") -> Optional[Path]:
    """
    Fetch portrait grid art for a game.
    Returns path to locally cached file, or None if unavailable.

    Lookup order:
      1. Return cached file if already downloaded
      2. Lookup SGDB game id via Steam appid (if available)
      3. Lookup SGDB game id via name search
      4. Download best matching 600×900 white_logo (or fallback style) grid
    """
    if not config.sgdb_api_key():
        return None   # silently skip if no key configured

    dest = _cache_path(game_slug)
    if dest.exists() and dest.stat().st_size > 0:
        return dest

    sgdb_id: Optional[int] = None

    # Try steam appid first (faster, more accurate)
    if steam_appid:
        sgdb_id = _get_game_id_by_steam_appid(steam_appid)

    # Fall back to name search
    if sgdb_id is None and game_name:
        sgdb_id = _get_game_id_by_name(game_name)

    if sgdb_id is None:
        return None

    style      = config.get("art", "sgdb_style")        # "white_logo"
    dimensions = config.get("art", "sgdb_dimensions")   # "600x900"

    url = _fetch_grid_url(sgdb_id, style, dimensions)
    if url is None:
        return None

    if _download(url, dest):
        return dest

    return None


def has_key() -> bool:
    """Return True if an API key is configured."""
    return bool(config.sgdb_api_key())


def get_sgdb_id(name: str, steam_appid: Optional[str] = None) -> Optional[int]:
    """Resolve steam_appid or game name to SGDB game ID."""
    sgdb_id: Optional[int] = None
    if steam_appid:
        sgdb_id = _get_game_id_by_steam_appid(steam_appid)
    if sgdb_id is None and name:
        sgdb_id = _get_game_id_by_name(name)
    return sgdb_id


def fetch_all_grid_urls(sgdb_id: int, style: str, dimensions: str) -> list[str]:
    """Fetch all matching grid URLs from SGDB for a given game ID and style/dimensions."""
    params: dict = {
        "dimensions": dimensions,
        "mimes":      "image/png,image/jpeg,image/webp",
    }
    if style:
        params["styles"] = style

    data = _get(f"grids/game/{sgdb_id}", params)
    urls = []
    if data and data.get("success") and data.get("data"):
        for item in data["data"]:
            if not item.get("nsfw") and not item.get("humor"):
                url = item.get("url")
                if url:
                    urls.append(url)
    return urls


def download_url_to_path(url: str, dest: Path) -> bool:
    """Download the specified URL to dest path."""
    return _download(url, dest)
