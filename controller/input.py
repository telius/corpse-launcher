"""
DualSense / PS5 controller + keyboard + mouse input handler.

Uses pygame's SDL2 GameController API where possible for normalised
button mapping, with a fallback to raw joystick for hats.

Button constants follow SDL2 GameController layout:
  A (Cross)    = 0
  B (Circle)   = 1
  X (Square)   = 2
  Y (Triangle) = 3
  Back/Select  = 4
  Guide        = 5
  Start        = 6 / Options on PS5
  L3           = 7
  R3           = 8
  LB (L1)      = 9
  RB (R1)      = 10
  DPad Up      = 11
  DPad Down    = 12
  DPad Left    = 13
  DPad Right   = 14
  Touchpad     = 15  (PS5-specific, via HIDAPI mode)

Axis layout:
  Left stick X  = 0    Left stick Y  = 1
  Right stick X = 2    Right stick Y = 3
  L2 trigger    = 4    R2 trigger    = 5
"""

from __future__ import annotations
from dataclasses import dataclass, field
from enum import Enum, auto
import pygame

# ---------------------------------------------------------------------------
# Named actions the UI cares about
# ---------------------------------------------------------------------------

class Action(Enum):
    NAV_UP     = auto()
    NAV_DOWN   = auto()
    NAV_LEFT   = auto()
    NAV_RIGHT  = auto()
    CONFIRM    = auto()   # Cross  ✕ → launch
    BACK       = auto()   # Circle ◯ → back / close overlay
    DETAILS    = auto()   # Triangle △ → open details overlay
    HIDE       = auto()   # Square □ → hide/unhide selected game
    SHOW_HIDDEN= auto()   # Create/Select → toggle showing hidden entries
    PAGE_LEFT  = auto()   # L1
    PAGE_RIGHT = auto()   # R1
    REFRESH    = auto()   # Options → reimport library
    QUIT       = auto()   # keyboard ESC
    HELP       = auto()   # F1 → show keybinds overlay
    MOUSE_CLICK = auto()  # left mouse click (carries position via event data)
    MOUSE_DBLCLICK = auto()  # double-click
    SCROLL_UP  = auto()   # mouse wheel up
    SCROLL_DOWN= auto()   # mouse wheel down


# ---------------------------------------------------------------------------
# Deadzone / repeat constants
# ---------------------------------------------------------------------------

STICK_DEADZONE    = 0.25     # lowered deadzone for snappier analog responsiveness
STICK_REPEAT_INIT = 0.25     # reduced delay before first repeat (250ms)
STICK_REPEAT_RATE = 0.08     # snappier repeating scroll (80ms)

HAT_REPEAT_INIT   = 0.22     # 220ms initial delay
HAT_REPEAT_RATE   = 0.06     # 60ms repeat interval for D-Pad navigation

# Keyboard repeat (for arrow keys / WASD held down)
KEY_REPEAT_INIT   = 0.22     # 220ms initial delay
KEY_REPEAT_RATE   = 0.05     # 50ms repeat interval for rapid keyboard scroll

DPAD_MAP = {
    ( 0,  1): Action.NAV_UP,
    ( 0, -1): Action.NAV_DOWN,
    (-1,  0): Action.NAV_LEFT,
    ( 1,  0): Action.NAV_RIGHT,
}

BUTTON_ACTION = {
    0:  Action.CONFIRM,
    1:  Action.BACK,
    2:  Action.DETAILS,     # Square -> Details
    3:  Action.DETAILS,     # Triangle -> Details
    9:  Action.PAGE_LEFT,
    10: Action.PAGE_RIGHT,
    4:  Action.SHOW_HIDDEN, # Create / Select
    6:  Action.REFRESH,     # Options
    11: Action.HIDE,        # Dpad Up -> Hide item
}

# Stick directions → action
STICK_ACTION: dict[tuple[int, int], Action] = {
    # (axis_index, sign)
    (1, -1): Action.NAV_UP,
    (1,  1): Action.NAV_DOWN,
    (0, -1): Action.NAV_LEFT,
    (0,  1): Action.NAV_RIGHT,
}

# Keys that support held-repeat
_REPEATABLE_KEYS = {
    pygame.K_UP, pygame.K_DOWN, pygame.K_LEFT, pygame.K_RIGHT,
    pygame.K_w, pygame.K_s, pygame.K_a, pygame.K_d,
}


# ---------------------------------------------------------------------------
# Repeat state tracker
# ---------------------------------------------------------------------------

@dataclass
class _RepeatState:
    action: Action | None = None
    timer:  float = 0.0
    phase:  int   = 0      # 0 = initial delay, 1 = repeating


# ---------------------------------------------------------------------------
# InputHandler
# ---------------------------------------------------------------------------

class InputHandler:
    """
    Processes pygame events into Action instances with repeat-delay logic.
    Call `poll(dt)` each frame; it returns a list of Actions fired this frame.
    Also stores last mouse position for hover/click handling.
    """

    def __init__(self):
        self._controller: pygame.joystick.JoystickType | None = None
        self._hat_state: tuple[int, int] = (0, 0)
        self._stick_dir: tuple[int, int] = (0, 0)   # (x_sign, y_sign)
        self._hat_repeat   = _RepeatState()
        self._stick_repeat = _RepeatState()
        self._quit_held    = 0.0   # seconds start button held

        # Keyboard repeat state
        self._held_key: int | None = None
        self._key_repeat = _RepeatState()

        # Mouse state
        self.mouse_pos: tuple[int, int] = (0, 0)
        self._last_click_time: float = 0.0
        self._dblclick_threshold: float = 0.35  # seconds

        self._init_controller()

    # -----------------------------------------------------------------------
    def _init_controller(self):
        try:
            pygame.joystick.quit()
            pygame.joystick.init()
            count = pygame.joystick.get_count()
            if count > 0:
                self._controller = pygame.joystick.Joystick(0)
                self._controller.init()
                name = self._controller.get_name()
                print(f"[input] Controller: {name} ({self._controller.get_numbuttons()} buttons, "
                      f"{self._controller.get_numaxes()} axes)")
            else:
                self._controller = None
                print("[input] No controller detected. Keyboard + mouse navigation.")
        except Exception as e:
            self._controller = None
            print(f"[input] Joystick initialization error: {e}")

    # -----------------------------------------------------------------------
    def _stick_direction(self) -> tuple[int, int]:
        """Return (x_sign, y_sign) of left stick, or (0, 0) if in deadzone."""
        if self._controller is None:
            return (0, 0)
        try:
            ax = self._controller.get_axis(0)
            ay = self._controller.get_axis(1)
        except Exception:
            return (0, 0)
        x = 0 if abs(ax) < STICK_DEADZONE else (1 if ax > 0 else -1)
        y = 0 if abs(ay) < STICK_DEADZONE else (1 if ay > 0 else -1)
        return (x, y)

    # -----------------------------------------------------------------------
    def _action_from_stick(self) -> Action | None:
        sx, sy = self._stick_dir
        if sy == -1: return Action.NAV_UP
        if sy ==  1: return Action.NAV_DOWN
        if sx == -1: return Action.NAV_LEFT
        if sx ==  1: return Action.NAV_RIGHT
        return None

    def _action_from_hat(self) -> Action | None:
        return DPAD_MAP.get(self._hat_state)

    # -----------------------------------------------------------------------
    def poll(self, dt: float) -> list[Action]:
        actions: list[Action] = []

        # --- Process pygame events ------------------------------------------
        for event in pygame.event.get():
            if event.type == pygame.QUIT:
                actions.append(Action.QUIT)

            # Keyboard press
            elif event.type == pygame.KEYDOWN:
                key_actions = self._key_actions(event.key)
                actions += key_actions
                # Start key repeat for navigation keys
                if event.key in _REPEATABLE_KEYS and key_actions:
                    self._held_key = event.key
                    self._key_repeat = _RepeatState(
                        action=key_actions[0], timer=0.0, phase=0)

            elif event.type == pygame.KEYUP:
                if event.key == self._held_key:
                    self._held_key = None
                    self._key_repeat = _RepeatState()

            # Mouse button
            elif event.type == pygame.MOUSEBUTTONDOWN:
                if event.button == 1:  # left click
                    self.mouse_pos = event.pos
                    import time
                    now = time.monotonic()
                    if now - self._last_click_time < self._dblclick_threshold:
                        actions.append(Action.MOUSE_DBLCLICK)
                    else:
                        actions.append(Action.MOUSE_CLICK)
                    self._last_click_time = now
                elif event.button == 4:  # scroll up
                    actions.append(Action.SCROLL_UP)
                elif event.button == 5:  # scroll down
                    actions.append(Action.SCROLL_DOWN)

            elif event.type == pygame.MOUSEWHEEL:
                if event.y > 0:
                    actions.append(Action.SCROLL_UP)
                elif event.y < 0:
                    actions.append(Action.SCROLL_DOWN)

            elif event.type == pygame.MOUSEMOTION:
                self.mouse_pos = event.pos

            # Controller button press
            elif event.type == pygame.JOYBUTTONDOWN:
                btn = event.button
                if btn in BUTTON_ACTION:
                    actions.append(BUTTON_ACTION[btn])

            # D-pad hat
            elif event.type == pygame.JOYHATMOTION:
                self._hat_state = (event.value[0], event.value[1])
                self._hat_repeat = _RepeatState()
                act = self._action_from_hat()
                if act:
                    actions.append(act)
                    self._hat_repeat.action = act
                    self._hat_repeat.timer  = 0.0
                    self._hat_repeat.phase  = 0

            # Joystick axis (controller re-connect)
            elif event.type == pygame.JOYDEVICEADDED:
                self._init_controller()

            elif event.type == pygame.JOYDEVICEREMOVED:
                self._controller = None

            # Window resize
            elif event.type == pygame.VIDEORESIZE:
                pass  # handled externally

        # --- Keyboard repeat logic ------------------------------------------
        if self._held_key is not None and self._key_repeat.action:
            self._key_repeat.timer += dt
            threshold = KEY_REPEAT_INIT if self._key_repeat.phase == 0 else KEY_REPEAT_RATE
            if self._key_repeat.timer >= threshold:
                self._key_repeat.timer = 0.0
                self._key_repeat.phase = 1
                actions.append(self._key_repeat.action)

        # --- Stick continuous state -----------------------------------------
        new_stick_dir = self._stick_direction()
        if new_stick_dir != self._stick_dir:
            self._stick_dir = new_stick_dir
            self._stick_repeat = _RepeatState()
            act = self._action_from_stick()
            if act:
                actions.append(act)
                self._stick_repeat.action = act
                self._stick_repeat.timer  = 0.0
                self._stick_repeat.phase  = 0

        # --- Repeat logic (hat) --------------------------------------------
        if self._hat_repeat.action:
            self._hat_repeat.timer += dt
            threshold = HAT_REPEAT_INIT if self._hat_repeat.phase == 0 else HAT_REPEAT_RATE
            if self._hat_repeat.timer >= threshold:
                self._hat_repeat.timer = 0.0
                self._hat_repeat.phase = 1
                if self._action_from_hat() == self._hat_repeat.action:
                    actions.append(self._hat_repeat.action)
                else:
                    self._hat_repeat = _RepeatState()

        # --- Repeat logic (stick) ------------------------------------------
        if self._stick_repeat.action:
            self._stick_repeat.timer += dt
            threshold = STICK_REPEAT_INIT if self._stick_repeat.phase == 0 else STICK_REPEAT_RATE
            if self._stick_repeat.timer >= threshold:
                self._stick_repeat.timer = 0.0
                self._stick_repeat.phase = 1
                current_act = self._action_from_stick()
                if current_act == self._stick_repeat.action:
                    actions.append(self._stick_repeat.action)
                else:
                    self._stick_repeat = _RepeatState()

        return actions

    # -----------------------------------------------------------------------
    def _key_actions(self, key: int) -> list[Action]:
        """Keyboard fallback mappings."""
        mapping = {
            pygame.K_UP:     Action.NAV_UP,
            pygame.K_DOWN:   Action.NAV_DOWN,
            pygame.K_LEFT:   Action.NAV_LEFT,
            pygame.K_RIGHT:  Action.NAV_RIGHT,
            pygame.K_w:      Action.NAV_UP,
            pygame.K_s:      Action.NAV_DOWN,
            pygame.K_a:      Action.NAV_LEFT,
            pygame.K_d:      Action.NAV_RIGHT,
            pygame.K_RETURN: Action.CONFIRM,
            pygame.K_SPACE:  Action.CONFIRM,
            pygame.K_ESCAPE: Action.BACK,
            pygame.K_F5:     Action.REFRESH,
            pygame.K_F1:     Action.HELP,
            pygame.K_h:      Action.HIDE,
            pygame.K_TAB:    Action.SHOW_HIDDEN,
            pygame.K_i:      Action.DETAILS,
            pygame.K_PAGEUP:   Action.PAGE_LEFT,
            pygame.K_PAGEDOWN: Action.PAGE_RIGHT,
        }
        return [mapping[key]] if key in mapping else []

    # -----------------------------------------------------------------------
    @property
    def has_controller(self) -> bool:
        return self._controller is not None
