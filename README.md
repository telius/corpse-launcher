# corpse-launcher

> Lightweight native Rust + SDL2 Linux game launcher with PS5 DualSense controller support, Steam + Lutris auto-import, SteamGridDB & IGDB metadata, and a Dracula-variant color scheme.

## Features
- **Native Rust + SDL2** — Sub-second startup, hardware-accelerated GPU texture & glyph caching, and `~30 MB` memory footprint
- **Portrait 600×900 grid** — SteamGridDB `white_logo` style art + custom cover art support
- **PS5 DualSense navigation** — D-pad, left stick, ✕ to launch, ○ back, △/□ details, L1/R1 page
- **Select on Hover** — High-performance \(O(1)\) mouse-over selection
- **Auto-imports** Steam (including `shortcuts.vdf` non-Steam games) and Lutris (`pga.db` + Battle.net overrides)
- **Smart launch routing** — Steam+Lutris overlap → launch via Lutris, plus dual-client World of Warcraft sync & launch
- **Hiding Games** — Toggle visibility of any item with a dedicated action, plus a global hidden items toggle
- **OLED Protection** — Orbital pixel shift (sinusoidal offset to avoid burn-in) with an F1 menu toggle
- **Inactivity Autodimmer** — Fades out display when inactive for 30 seconds, flushing GPU caches and throttling to 15 FPS when idle

## Quick Start

```bash
# 1. Build release binary
cargo build --release

# 2. Run
./run.sh

# 3. Dry-run (print library only, no window)
./run.sh --dry-run
```

## File Structure

```
corpse-launcher/
├── Cargo.toml
├── run.sh
└── src/
    ├── main.rs       # Entry point, SDL2 init & main loop
    ├── config.rs     # Colors, paths, TOML config loader/saver
    ├── game.rs       # Unified Game struct & Platform enum
    ├── hidden.rs     # Hidden games persistence
    ├── vdf.rs        # Text (.vdf/.acf) & binary (shortcuts.vdf) VDF parsers
    ├── steam.rs      # Steam library + shortcuts importer
    ├── lutris.rs     # Lutris pga.db SQLite + YAML importer
    ├── library.rs    # Steam + Lutris merge, dedup & sort
    ├── sgdb.rs       # SteamGridDB v2 API client & disk cache
    ├── igdb.rs       # IGDB OAuth2 metadata fetcher & cache
    ├── art.rs        # GPU texture cache & async background art fetcher
    ├── audio.rs      # Procedural audio synthesizer (SDL2 AudioQueue)
    ├── input.rs      # DualSense controller, keyboard & mouse handler
    ├── launcher.rs   # Game launcher & dual-WoW process manager
    └── ui.rs         # Font/text/icon caches, Grid, Sidebar & Overlays
```
