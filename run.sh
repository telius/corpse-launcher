#!/usr/bin/env bash
# corpse-launcher quick start
# Auto-detects Wayland/X11

cd "$(dirname "$0")"

if [ -n "$WAYLAND_DISPLAY" ]; then
    export SDL_VIDEODRIVER=wayland
elif [ -n "$DISPLAY" ]; then
    export SDL_VIDEODRIVER=x11
fi

exec python3 main.py "$@"
