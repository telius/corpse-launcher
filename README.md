# corpse-launcher

> Lightweight Linux game launcher with PS5 DualSense controller support, Steam + Lutris auto-import, SteamGridDB art, and a Dracula-variant color scheme.

## Features
- **Portrait 600×900 grid** — SteamGridDB `white_logo` style art
- **PS5 DualSense navigation** — D-pad, left stick, ✕ to launch, ○ back, L1/R1 page
- **Auto-imports** Steam and Lutris (deduplicated)
- **Smart launch routing** — Steam+Lutris overlap → launch via Lutris
- **Resizable window** — 4-column grid, animated sidebar panel
- **~50 MB RAM** footprint

## Quick Start

```bash
# 1. Clone / enter project
cd ~/projects/corpse-launcher

# 2. Run
./run.sh

# 3. Dry-run (print library only, no window)
./run.sh --dry-run
```

## SteamGridDB Setup

1. Get a free API key at https://www.steamgriddb.com/profile/preferences/api
2. Copy the config template:
   ```bash
   mkdir -p ~/.config/corpse-launcher
   cp config.example.toml ~/.config/corpse-launcher/config.toml
   ```
3. Edit `~/.config/corpse-launcher/config.toml` and set `sgdb_api_key = "YOUR_KEY"`

Art downloads automatically on first view and is cached at `~/.cache/corpse-launcher/art/`.

## Controller Map (PS5 DualSense)

| Button        | Action            |
|---------------|-------------------|
| D-pad / L-stick | Navigate grid   |
| ✕ (Cross)     | Launch game       |
| ○ (Circle)    | Back / Quit       |
| △ (Triangle)  | Game details      |
| L1            | Page left         |
| R1            | Page right        |
| Select        | Refresh library   |

## Keyboard Fallback

`↑↓←→` or `WASD` to navigate, `Enter`/`Space` to launch, `Esc` to quit, `F5` to refresh.

## File Structure

```
corpse-launcher/
├── main.py              # Entry point + game loop
├── config.py            # Colors, paths, config loader
├── launcher.py          # Subprocess launch (Steam / Lutris)
├── controller/
│   └── input.py         # DualSense / keyboard input handler
├── data/
│   ├── game.py          # Unified Game dataclass
│   ├── library.py       # Steam + Lutris merge + dedup
│   ├── sgdb.py          # SteamGridDB API client
│   └── art.py           # Surface cache + async art loader
├── importer/
│   ├── steam.py         # Steam VDF / ACF parser
│   └── lutris.py        # Lutris pga.db + YAML parser
└── ui/
    ├── grid.py          # Scrollable portrait grid
    ├── sidebar.py       # Selected game detail panel
    ├── topbar.py        # Title bar + status
    └── animations.py    # Easing, lerp, Tween, Pulse
```

## Art Cache

Art is cached at `~/.cache/corpse-launcher/art/`. Delete files there to force re-download.

Steam's own cached art is used first (no API call needed for installed Steam games).
