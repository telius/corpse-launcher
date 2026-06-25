# corpse-launcher

> Lightweight Linux game launcher with PS5 DualSense controller support, Steam + Lutris auto-import, SteamGridDB art, and a Dracula-variant color scheme.

## Features
- **Portrait 600×900 grid** — SteamGridDB `white_logo` style art
- **PS5 DualSense navigation** — D-pad, left stick, ✕ to launch, ○ back, L1/R1 page
- **Select on Hover** — High-performance \(O(1)\) mouse-over selection
- **Auto-imports** Steam and Lutris (deduplicated)
- **Smart launch routing** — Steam+Lutris overlap → launch via Lutris
- **Hiding Games** — Toggle visibility of any item with a dedicated action, plus a global hidden items toggle
- **OLED Protection** — Orbital pixel shift (sinusoidal offset to avoid burn-in) with an F1 menu toggle
- **Inactivity Autodimmer** — Fades out display when inactive for 30 seconds, dropping loop to 5 FPS for minimum CPU usage
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
| □ (Square)    | Hide / unhide game|
| L1            | Page left         |
| R1            | Page right        |
| Select        | Toggle hidden list|
| Options       | Refresh library   |

## Keyboard Fallback

- `↑↓←→` or `WASD`: Navigate grid
- `Enter` / `Space`: Launch selected game
- `Esc` / `Backspace`: Back / Close overlay
- `I`: Open details overlay
- `H`: Hide / unhide selected game
- `Tab`: Toggle showing hidden games
- `F1`: Open keybinds / settings overlay
- `P` (in F1 menu): Toggle Pixel Shift
- `F5` / `R`: Refresh library

## File Structure

```
corpse-launcher/
├── corpse-launcher.py   # Entry point + game loop
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
    ├── keybinds.py      # Help & settings overlay (F1)
    ├── details.py       # Extended details overlay (I)
    └── animations.py    # Easing, lerp, Tween, Pulse
```

## Art Cache

Art is cached at `~/.cache/corpse-launcher/art/`. Delete files there to force re-download.

Steam's own cached art is used first (no API call needed for installed Steam games).

