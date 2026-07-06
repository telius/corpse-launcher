"""
corpse-launcher — global configuration and constants.
Edit ~/.config/corpse-launcher/config.toml to override.
"""

import tomllib  # stdlib Python 3.11+
from pathlib import Path

# ---------------------------------------------------------------------------
# Paths
# ---------------------------------------------------------------------------
HOME = Path.home()
CONFIG_DIR = HOME / ".config" / "corpse-launcher"
ART_CACHE = CONFIG_DIR / "art"
CACHE_DIR = HOME / ".cache" / "corpse-launcher"
CONFIG_FILE = CONFIG_DIR / "config.toml"

STEAM_ROOT = HOME / ".steam" / "root"
STEAM_LIBRARY_VDF = STEAM_ROOT / "config" / "libraryfolders.vdf"
STEAM_ART_CACHE = HOME / ".local" / "share" / "Steam" / "appcache" / "librarycache"

LUTRIS_PGA_DB = HOME / ".local" / "share" / "lutris" / "pga.db"
LUTRIS_GAMES_DIR = HOME / ".local" / "share" / "lutris" / "games"
LUTRIS_COVERS_DIR = HOME / ".local" / "share" / "lutris" / "covers"
LUTRIS_GAMES_DIR2 = HOME / ".config" / "lutris" / "games"  # older installs

# ---------------------------------------------------------------------------
# Default config (overridable via config.toml)
# ---------------------------------------------------------------------------
_DEFAULTS: dict = {
    "general": {
        "window_width": 1280,
        "window_height": 720,
        "fps": 60,
        "vsync": True,
        "show_playtime": True,
        "sort_by": "name",  # "name" | "playtime" | "last_played"
    },
    "art": {
        "sgdb_api_key": "",
        "sgdb_style": "white_logo",  # white_logo | alternate | material | no_logo
        "sgdb_dimensions": "600x900",
        "prefer_animated": False,
        "card_width": 190,  # display width per card in the grid
    },
    "igdb": {
        "client_id": "",
        "client_secret": "",
    },
    "ui": {
        "grid_cols": 4,
        "grid_gap": 30,
        "sidebar_width": 300,
        "card_border_width": 2,
        "scroll_speed": 8,
        "animation_speed": 12.0,  # higher = snappier
        "pixel_shift": False,  # OLED orbital pixel shift state
        "animations": False,  # enable transitions and scrolling animations
    },
}


def _apply_toml(cfg: dict, path) -> None:
    """Merge a toml file into cfg in-place, ignoring parse errors."""
    try:
        with open(path, "rb") as f:
            user = tomllib.load(f)
        for section, values in user.items():
            if section in cfg:
                cfg[section].update(values)
            else:
                cfg[section] = values
    except Exception as e:
        print(f"[config] Warning: could not load {path}: {e}")


def _load() -> dict:
    """Load user config, falling back to defaults for missing keys."""
    cfg = {section: dict(values) for section, values in _DEFAULTS.items()}
    if CONFIG_FILE.exists():
        _apply_toml(cfg, CONFIG_FILE)
    local = Path("config.toml")
    if local.exists() and local.resolve() != CONFIG_FILE.resolve():
        _apply_toml(cfg, local)
    return cfg


CFG = _load()


def save_config() -> None:
    """Write current in-memory CFG dict back to CONFIG_FILE as TOML."""
    # Write simple TOML manually to avoid dependencies
    lines = []
    for section, keys in CFG.items():
        lines.append(f"[{section}]")
        for key, val in keys.items():
            if isinstance(val, bool):
                val_str = "true" if val else "false"
            elif isinstance(val, (int, float)):
                val_str = str(val)
            else:
                # String escapes
                val_str = f'"{str(val)}"'
            lines.append(f"{key} = {val_str}")
        lines.append("")
    try:
        CONFIG_FILE.write_text("\n".join(lines), encoding="utf-8")
    except Exception as e:
        print(f"[config] Error saving config to {CONFIG_FILE}: {e}")


# Ensure cache / config dirs exist
CONFIG_DIR.mkdir(parents=True, exist_ok=True)
CACHE_DIR.mkdir(parents=True, exist_ok=True)
ART_CACHE.mkdir(parents=True, exist_ok=True)

# Write .trackerignore files to stop localsearch-3 from monitoring these directories
try:
    (CONFIG_DIR / ".trackerignore").touch(exist_ok=True)
    (CACHE_DIR / ".trackerignore").touch(exist_ok=True)
except Exception as e:
    print(f"[config] Warning: could not write .trackerignore: {e}")

# ---------------------------------------------------------------------------
# Color palette — Dracula variant
# ---------------------------------------------------------------------------
BG = (0, 0, 0)           # main background
BG_DEEP = (0, 0, 0)      # alias kept for badge text contrast
BG_PANEL = (22, 22, 26)  # panel / sidebar
BG_CARD = (22, 22, 26)   # card background

VIOLET = (102, 0, 255)   # primary accent
MAGENTA = (255, 0, 255)  # secondary accent
CYAN = (0, 255, 255)     # highlight / info
WHITE = (240, 240, 255)  # near-white text
GREY = (100, 80, 140)    # muted text
BORDER_BG = (0, 0, 0)    # unselected border

# Platform badge colours
BADGE_STEAM = (102, 192, 234)
BADGE_LUTRIS = (255, 167, 38)
BADGE_BN = (0, 164, 255)  # Battle.net blue


# ---------------------------------------------------------------------------
# Convenience accessors
# ---------------------------------------------------------------------------
def get(section: str, key: str):
    return CFG[section][key]


def set_val(section: str, key: str, value) -> None:
    """Set in-memory config value and persist to CONFIG_FILE only if changed."""
    if section not in CFG:
        CFG[section] = {}
    if CFG[section].get(key) == value:
        return
    CFG[section][key] = value
    save_config()


def sgdb_api_key() -> str:
    return CFG["art"]["sgdb_api_key"]


def window_size() -> tuple[int, int]:
    return CFG["general"]["window_width"], CFG["general"]["window_height"]


def card_width() -> int:
    return CFG["art"]["card_width"]


def card_height() -> int:
    w = CFG["art"]["card_width"]
    return int(w * 900 / 600)  # maintain 600×900 aspect ratio


def grid_cols() -> int:
    return CFG["ui"]["grid_cols"]


def grid_gap() -> int:
    return CFG["ui"]["grid_gap"]


def sidebar_width() -> int:
    return CFG["ui"]["sidebar_width"]


def igdb_credentials() -> tuple[str, str]:
    igdb_sec = CFG.get("igdb", {})
    return igdb_sec.get("client_id", ""), igdb_sec.get("client_secret", "")
