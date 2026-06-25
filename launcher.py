"""
Game launcher — fires the correct subprocess for Steam / Lutris / native.
"""

from __future__ import annotations
import subprocess

from data.game import Game, Platform


def launch(game: Game):
    """Launch the given game using the appropriate method."""
    print(
        f"[launch] Launching '{game.name}' "
        f"(platform={game.platform.value}, "
        f"via_lutris={game.launch_via_lutris}, "
        f"lutris_id={game.lutris_launch_id})"
    )

    try:
        # Swap workspace to workspace 5 using swaymsg
        try:
            subprocess.run(["swaymsg", "workspace 5"], capture_output=True)
        except Exception as e:
            print(f"[launch] Failed to run swaymsg: {e}")

        if game.launch_via_lutris and game.lutris_launch_id is not None:
            _launch_lutris_id(game.lutris_launch_id)
        elif game.platform == Platform.STEAM and game.steam_appid:
            _launch_steam(game.steam_appid)
        elif game.launch_via_lutris and game.lutris_slug:
            _launch_lutris_slug(game.lutris_slug)
        else:
            print(
                f"[launch] Don't know how to launch '{game.name}' — no appid or lutris id."
            )
    except Exception as e:
        print(f"[launch] Error launching '{game.name}': {e}")


def _launch_steam(appid: str):
    """Use Steam URI protocol to launch a game."""
    subprocess.Popen(
        ["steam", f"steam://rungameid/{appid}"],
        start_new_session=True,
    )


def _launch_lutris_id(lutris_id: int):
    """Launch a Lutris game by its database id."""
    subprocess.Popen(
        ["lutris", f"lutris:rungameid/{lutris_id}"],
        start_new_session=True,
    )


def _launch_lutris_slug(slug: str):
    """Launch a Lutris game by slug (fallback)."""
    subprocess.Popen(
        ["lutris", f"lutris:rungame/{slug}"],
        start_new_session=True,
    )
