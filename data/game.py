"""
Unified Game dataclass — represents a single game entry from any source.
"""

from __future__ import annotations
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import Optional


class Platform(str, Enum):
    STEAM   = "steam"
    LUTRIS  = "lutris"
    BATTLENET = "battlenet"
    GOG     = "gog"
    EPIC    = "epic"
    UNKNOWN = "unknown"


class Runner(str, Enum):
    NATIVE  = "native"
    WINE    = "wine"
    PROTON  = "proton"
    DOSBOX  = "dosbox"
    UNKNOWN = "unknown"


@dataclass
class Game:
    """Single game entry unified from Steam / Lutris / other sources."""

    # Core identity
    slug: str                          # unique key for dedup (e.g. "diablo-iv")
    name: str                          # display name
    platform: Platform = Platform.UNKNOWN

    # Steam-specific
    steam_appid: Optional[str] = None  # "1091500"

    # Lutris-specific
    lutris_id: Optional[int] = None    # pga.db row id
    lutris_slug: Optional[str] = None  # "diablo-iv"
    runner: str = ""                   # "steam", "wine", "lutris", etc.

    # Art
    art_path: Optional[Path] = None    # local cached 600×900 image
    art_loaded: bool = False           # True once surface has been fetched

    # Metadata
    install_dir: Optional[Path] = None
    playtime_minutes: int = 0
    last_played: int = 0               # unix timestamp
    year: Optional[int] = None

    # Launch override — set after dedup merge
    # If set, this overrides the default launch strategy
    launch_via_lutris: bool = False
    lutris_launch_id: Optional[int] = None   # lutris db id to use for launch

    # Internal — used for dedup merge priority
    _mtime: float = field(default=0.0, repr=False, compare=False)

    # -----------------------------------------------------------------------
    def display_platform_label(self) -> str:
        match self.platform:
            case Platform.STEAM:     return "Steam"
            case Platform.LUTRIS:    return "Lutris"
            case Platform.BATTLENET: return "Battle.net"
            case Platform.GOG:       return "GOG"
            case Platform.EPIC:      return "Epic"
            case _:                  return "Unknown"

    def playtime_str(self) -> str:
        if self.playtime_minutes <= 0:
            return ""
        h = self.playtime_minutes // 60
        m = self.playtime_minutes % 60
        if h:
            return f"{h}h {m}m"
        return f"{m}m"

    def __hash__(self):
        return hash(self.slug)

    def __eq__(self, other):
        if isinstance(other, Game):
            return self.slug == other.slug
        return NotImplemented
