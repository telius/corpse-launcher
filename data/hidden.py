"""
Hidden game persistence.
Stores hidden game slugs in ~/.config/corpse-launcher/hidden.txt (one per line).
"""

from __future__ import annotations
import config

_HIDDEN_FILE = config.CONFIG_DIR / "hidden.txt"

# In-memory set — loaded once at import
_hidden: set[str] = set()
_loaded = False


def _ensure_loaded():
    global _loaded
    if _loaded:
        return
    _loaded = True
    if _HIDDEN_FILE.exists():
        try:
            for line in _HIDDEN_FILE.read_text().splitlines():
                s = line.strip()
                if s:
                    _hidden.add(s)
        except Exception as e:
            print(f"[hidden] Could not load hidden list: {e}")


def _save():
    try:
        _HIDDEN_FILE.parent.mkdir(parents=True, exist_ok=True)
        _HIDDEN_FILE.write_text("\n".join(sorted(_hidden)) + "\n")
    except Exception as e:
        print(f"[hidden] Could not save hidden list: {e}")


def is_hidden(slug: str) -> bool:
    _ensure_loaded()
    return slug in _hidden


def hide(slug: str):
    _ensure_loaded()
    _hidden.add(slug)
    _save()
    print(f"[hidden] Hidden: {slug}")


def unhide(slug: str):
    _ensure_loaded()
    _hidden.discard(slug)
    _save()
    print(f"[hidden] Unhidden: {slug}")


def toggle(slug: str) -> bool:
    """Toggle hidden state. Returns True if now hidden."""
    _ensure_loaded()
    if slug in _hidden:
        unhide(slug)
        return False
    else:
        hide(slug)
        return True


def all_hidden() -> set[str]:
    _ensure_loaded()
    return set(_hidden)
