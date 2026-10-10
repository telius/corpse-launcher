"""
Steam library importer.
Parses libraryfolders.vdf → appmanifest_*.acf to enumerate installed games.
Also reads userdata/<steamid>/config/localconfig.vdf for playtime.
"""

from __future__ import annotations
import re
import zlib
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
        Path.home() / ".var" / "app" / "com.valvesoftware.Steam" / ".local" / "share" / "Steam",
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
            data = vdf.load(
                open(lc, encoding="utf-8", errors="replace"), mapper=vdf.VDFDict
            )
            apps = (
                data.get("UserLocalConfigStore", {})
                .get("Software", {})
                .get("Valve", {})
                .get("Steam", {})
                .get("apps", {})
            )
            for appid, info in apps.items():
                if isinstance(info, dict):
                    pt = int(info.get("Playtime", 0) or 0)
                    lp = int(info.get("LastPlayed", 0) or 0)
                    if pt or lp:
                        playtimes[str(appid)] = pt
        except Exception:
            pass
    return playtimes


def _find_steam_art(
    appid: str,
    steam_root: Optional[Path] = None,
    icon: str = "",
) -> Optional[Path]:
    """
    Look for portrait (600×900) grid art in Steam's user grid or art cache.
    Steam stores covers as:
      - userdata/<steamid>/config/grid/<appid>p.png / .jpg (custom portrait covers)
      - appcache/librarycache/<appid>_library_600x900.jpg / .png
      - userdata/<steamid>/config/grid/<appid>.png / .jpg (legacy grid)
    For shortcuts, appid may be 64-bit rungameid or 32-bit shortcut appid.
    """
    cids = [appid]
    try:
        val = int(appid)
        if val > 0xFFFFFFFF:
            cids.append(str(val >> 32))
        elif val > 0:
            cids.append(str((val << 32) | 0x02000000))
    except ValueError:
        pass

    if steam_root is None:
        steam_root = _steam_root()

    # 1. Check custom portrait / standard covers in userdata grid dirs (highest user fidelity)
    if steam_root:
        userdata = steam_root / "userdata"
        if userdata.exists():
            for uid_dir in userdata.iterdir():
                grid = uid_dir / "config" / "grid"
                if not grid.exists():
                    continue
                for cid in cids:
                    for suffix in ("p.png", "p.jpg", ".png", ".jpg"):
                        p = grid / f"{cid}{suffix}"
                        if p.exists():
                            return p

    # 2. Check standard steam librarycache
    art_dir = config.STEAM_ART_CACHE
    if art_dir.exists():
        for cid in cids:
            for suffix in (
                f"{cid}_library_600x900.jpg",
                f"{cid}_library_600x900.png",
                f"{cid}_library_hero.jpg",
                f"{cid}_header.jpg",
            ):
                p = art_dir / suffix
                if p.exists():
                    return p

    # 3. Check hero/header fallbacks in userdata grid dirs
    if steam_root:
        userdata = steam_root / "userdata"
        if userdata.exists():
            for uid_dir in userdata.iterdir():
                grid = uid_dir / "config" / "grid"
                if not grid.exists():
                    continue
                for cid in cids:
                    for suffix in ("_hero.png", "_hero.jpg", "_header.png", "_header.jpg"):
                        p = grid / f"{cid}{suffix}"
                        if p.exists():
                            return p

    # 4. Optional icon path specified in shortcut
    if icon:
        ip = Path(icon.strip(' "\''))
        if ip.exists() and ip.is_file():
            return ip

    return None


def _make_slug(name: str, appid: str) -> str:
    """Create a dedup slug from the game name."""
    slug = name.lower()
    slug = re.sub(r"[^a-z0-9]+", "-", slug)
    slug = slug.strip("-")
    return slug or f"steam-{appid}"


def _load_shortcuts(steam_root: Path, playtimes: dict[str, int]) -> list[Game]:
    """Parse shortcuts.vdf across all userdata directories for non-Steam games."""
    userdata = steam_root / "userdata"
    if not userdata.exists():
        return []

    games: list[Game] = []
    seen_slugs: set[str] = set()

    for uid_dir in sorted(userdata.iterdir()):
        if not uid_dir.is_dir() or not uid_dir.name.isdigit() or uid_dir.name == "0":
            continue

        sc_path = uid_dir / "config" / "shortcuts.vdf"
        if not sc_path.exists():
            continue

        try:
            with open(sc_path, "rb") as f:
                data = (
                    vdf.binary_load(f)
                    if hasattr(vdf, "binary_load")
                    else vdf.binary_loads(f.read())
                )
        except Exception as e:
            print(f"[steam] Warning: could not parse {sc_path}: {e}")
            continue

        sc_dict = data.get("shortcuts") or data.get("Shortcuts") or {}
        items = sc_dict.values() if isinstance(sc_dict, dict) else sc_dict

        for item in items:
            if not isinstance(item, dict):
                continue

            name = str(item.get("AppName") or item.get("appname") or "").strip()
            if not name:
                continue

            exe = str(item.get("Exe") or item.get("exe") or "").strip()

            raw_appid = item.get("appid")
            u_appid = (int(raw_appid) & 0xFFFFFFFF) if raw_appid is not None else 0
            if not u_appid:
                crc = zlib.crc32((exe + name).encode("utf-8")) & 0xFFFFFFFF
                u_appid = crc | 0x80000000

            rungameid = (u_appid << 32) | 0x02000000
            appid_str = str(rungameid)
            u_appid_str = str(u_appid)

            slug = _make_slug(name, u_appid_str)
            if slug in seen_slugs:
                continue

            is_hidden = int(item.get("IsHidden") or item.get("ishidden") or 0)
            if is_hidden:
                import data.hidden as hidden_store
                if not hidden_store.is_hidden(slug):
                    hidden_store.hide(slug)

            start_dir = str(item.get("StartDir") or item.get("startdir") or "").strip(' "\'')
            clean_exe = exe.strip(' "\'')
            install_dir: Optional[Path] = None
            if start_dir and Path(start_dir).exists():
                install_dir = Path(start_dir)
            elif clean_exe and Path(clean_exe).parent.exists():
                install_dir = Path(clean_exe).parent

            icon = str(item.get("icon") or "")
            art = _find_steam_art(appid_str, steam_root, icon=icon)

            last_played = int(item.get("LastPlayTime") or item.get("lastplaytime") or 0)
            playtime = playtimes.get(appid_str) or playtimes.get(u_appid_str) or 0

            from data import igdb
            igdb_meta = igdb.get_cached_metadata(slug)
            year = igdb_meta.get("release_year") if igdb_meta else None

            game = Game(
                slug=slug,
                name=name,
                platform=Platform.STEAM,
                steam_appid=appid_str,
                is_shortcut=True,
                runner="steam",
                art_path=art,
                install_dir=install_dir,
                playtime_minutes=playtime,
                last_played=last_played,
                year=year,
                _mtime=sc_path.stat().st_mtime,
            )
            games.append(game)
            seen_slugs.add(slug)

    return games


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------


def import_games() -> list[Game]:
    """
    Return a list of Game objects for every installed Steam game,
    including non-Steam shortcuts from shortcuts.vdf.
    """
    steam_root = _steam_root()
    if steam_root is None:
        print("[steam] Steam installation not found.")
        return []

    libraries = _load_library_folders(steam_root)
    playtimes = _load_playtimes(steam_root)
    games: list[Game] = []

    for steamapps in libraries:
        for acf_path in sorted(steamapps.glob("appmanifest_*.acf")):
            data = _read_acf(acf_path)
            if data is None:
                continue

            appid = str(data.get("appid", ""))
            name = str(data.get("name", "")).strip()
            state = int(data.get("StateFlags", 0) or 0)

            # Skip non-installed, tools, soundtracks, etc.
            if not name or appid in ("228980", "1070560"):  # Steamworks common
                continue
            # StateFlags: 4 = fully installed
            if state not in (4, 6, 1030):
                continue
            # Skip Proton, Steam Runtime, and other tools
            _SKIP_PREFIXES = (
                "Proton",
                "Steam Linux Runtime",
                "Steamworks",
                "Steam VR",
                "SteamVR",
            )
            if any(name.startswith(p) for p in _SKIP_PREFIXES):
                continue

            install_dir_str = data.get("installdir", "")
            install_dir = (
                steamapps / "common" / install_dir_str if install_dir_str else None
            )

            slug = _make_slug(name, appid)
            art = _find_steam_art(appid, steam_root)

            # Retrieve cached IGDB year if available to populate Steam release years
            import data.igdb as igdb
            igdb_meta = igdb.get_cached_metadata(slug)
            year = igdb_meta.get("release_year") if igdb_meta else None

            game = Game(
                slug=slug,
                name=name,
                platform=Platform.STEAM,
                steam_appid=appid,
                is_shortcut=False,
                runner="steam",
                art_path=art,
                install_dir=install_dir,
                playtime_minutes=playtimes.get(appid, 0),
                last_played=0,  # could read from localconfig too
                year=year,
                _mtime=acf_path.stat().st_mtime,
            )
            games.append(game)

    official_count = len(games)
    shortcuts = _load_shortcuts(steam_root, playtimes)
    existing_slugs = {g.slug for g in games}
    existing_appids = {g.steam_appid for g in games if g.steam_appid}
    shortcut_count = 0
    for sc in shortcuts:
        if sc.slug not in existing_slugs and sc.steam_appid not in existing_appids:
            games.append(sc)
            existing_slugs.add(sc.slug)
            if sc.steam_appid:
                existing_appids.add(sc.steam_appid)
            shortcut_count += 1

    print(f"[steam] Imported {len(games)} games ({official_count} official, {shortcut_count} shortcuts).")
    return games
