"""
IGDB API Client.
Fetches publisher/developer, genres, and a brief description/summary.
Caches metadata locally in json format under ~/.cache/corpse-launcher/igdb_cache.json
"""

from __future__ import annotations
import json
import time
import requests
from pathlib import Path
from typing import Optional, TypedDict, List

import config

_TOKEN_URL = "https://id.twitch.tv/oauth2/token"
_API_URL = "https://api.igdb.com/v4"
_CACHE_PATH = config.CACHE_DIR / "igdb_cache.json"

class IGDBMetadata(TypedDict):
    genres: List[str]
    companies: List[str]  # Developers/Publishers
    summary: str
    rating: Optional[float]
    release_year: Optional[int]


# In-memory metadata cache loaded from disk
_metadata_cache: dict[str, IGDBMetadata] = {}
_oauth_token: Optional[str] = None
_token_expires_at: float = 0.0


def _load_cache():
    global _metadata_cache
    if _CACHE_PATH.exists():
        try:
            with open(_CACHE_PATH, "r", encoding="utf-8") as f:
                _metadata_cache = json.load(f)
        except Exception as e:
            print(f"[igdb] Warning: failed to load cache: {e}")


def _save_cache():
    try:
        _CACHE_PATH.parent.mkdir(parents=True, exist_ok=True)
        with open(_CACHE_PATH, "w", encoding="utf-8") as f:
            json.dump(_metadata_cache, f, indent=2)
    except Exception as e:
        print(f"[igdb] Warning: failed to save cache: {e}")


def _get_access_token() -> Optional[str]:
    global _oauth_token, _token_expires_at
    
    # Check if we already have a valid unexpired token
    now = time.time()
    if _oauth_token and now < _token_expires_at:
        return _oauth_token

    client_id, client_secret = config.igdb_credentials()
    if not client_id or not client_secret:
        return None

    try:
        params = {
            "client_id": client_id,
            "client_secret": client_secret,
            "grant_type": "client_credentials",
        }
        res = requests.post(_TOKEN_URL, params=params, timeout=10)
        if res.status_code == 200:
            data = res.json()
            _oauth_token = data.get("access_token")
            # Set expiration with a 60-second buffer
            expires_in = data.get("expires_in", 3600)
            _token_expires_at = now + expires_in - 60
            return _oauth_token
        else:
            print(f"[igdb] Token request failed (HTTP {res.status_code}): {res.text}")
    except Exception as e:
        print(f"[igdb] Failed to obtain access token: {e}")
    return None


def _headers() -> Optional[dict]:
    client_id, _ = config.igdb_credentials()
    token = _get_access_token()
    if not client_id or not token:
        return None
    return {
        "Client-ID": client_id,
        "Authorization": f"Bearer {token}",
        "Content-Type": "text/plain"
    }


def fetch_metadata(game_name: str, slug: str) -> Optional[IGDBMetadata]:
    """
    Search IGDB by name and retrieve genres, companies, summary, rating.
    Saves to the local cache and returns the dict.
    """
    if not _metadata_cache:
        _load_cache()

    # Return cached data if present
    if slug in _metadata_cache:
        return _metadata_cache[slug]

    headers = _headers()
    if not headers:
        return None

    # Sanitize special trademark characters (®, ™) from query string as IGDB does not index them
    clean_name = game_name.replace("®", "").replace("™", "").replace("©", "")
    clean_name = " ".join(clean_name.split())

    try:
        # Query the games endpoint directly (rather than /search which returns mods/bundles/dlcs first).
        # We search by name and exclude items with colon (mods/dlcs usually contain ':') or filter by exact name match first if possible.
        # Specifically, we rank by rating/release or do a search.
        query = f'search "{clean_name}"; fields name, genres.name, involved_companies.company.name, involved_companies.developer, involved_companies.publisher, summary, total_rating, first_release_date; limit 5;'
        res = requests.post(f"{_API_URL}/games", headers=headers, data=query, timeout=10)
        
        if res.status_code == 200:
            games_list = res.json()
            if not games_list:
                return None
            
            # Find the best candidate. If there's an exact case-insensitive match on name, prefer it.
            # Otherwise, avoid names that look like DLCs/Mods (contain ":" or "Season" or "Pack" or "Expansion"), or default to first.
            game_data = games_list[0]
            for g in games_list:
                g_name = g.get("name", "")
                if g_name.lower() == game_name.lower():
                    game_data = g
                    break
            
            # If no exact match, try to choose one without a colon to avoid mods/DLCs
            if game_data.get("name", "").lower() != game_name.lower():
                for g in games_list:
                    if ":" not in g.get("name", ""):
                        game_data = g
                        break
            
            # Extract genres
            genres = []
            if "genres" in game_data:
                genres = [g["name"] for g in game_data["genres"] if "name" in g]

            # Extract companies (Prefer developers, fall back to publishers)
            companies = []
            if "involved_companies" in game_data:
                devs = []
                pubs = []
                for ic in game_data["involved_companies"]:
                    company_name = ic.get("company", {}).get("name")
                    if company_name:
                        if ic.get("developer"):
                            devs.append(company_name)
                        elif ic.get("publisher"):
                            pubs.append(company_name)
                companies = devs if devs else pubs

            # Extract release year
            rel_year = None
            if "first_release_date" in game_data:
                try:
                    rel_timestamp = int(game_data["first_release_date"])
                    rel_year = time.gmtime(rel_timestamp).tm_year
                except Exception:
                    pass

            meta: IGDBMetadata = {
                "genres": genres[:3],  # Keep up to 3 genres
                "companies": companies[:2],  # Keep up to 2 companies
                "summary": game_data.get("summary", ""),
                "rating": game_data.get("total_rating"),
                "release_year": rel_year,
            }
            
            # Store in cache
            _metadata_cache[slug] = meta
            _save_cache()
            return meta
            
        else:
            print(f"[igdb] API request failed (HTTP {res.status_code}): {res.text}")
    except Exception as e:
        print(f"[igdb] Failed to fetch game metadata from IGDB: {e}")
        
    return None


def get_cached_metadata(slug: str) -> Optional[IGDBMetadata]:
    """Retrieve metadata from local cache without doing network requests."""
    if not _metadata_cache:
        _load_cache()
    return _metadata_cache.get(slug)
