"""
Easing and animation helpers — all delta-time based.
No dependencies beyond Python stdlib.
"""

from __future__ import annotations
import math


# ---------------------------------------------------------------------------
# Easing functions (take t in [0.0, 1.0], return eased value)
# ---------------------------------------------------------------------------

def ease_out_cubic(t: float) -> float:
    return 1 - (1 - t) ** 3

def ease_in_out_quad(t: float) -> float:
    if t < 0.5:
        return 2 * t * t
    return 1 - (-2 * t + 2) ** 2 / 2

def ease_out_expo(t: float) -> float:
    if t >= 1.0:
        return 1.0
    return 1 - pow(2, -10 * t)

def ease_out_back(t: float, overshoot: float = 1.70158) -> float:
    c3 = overshoot + 1
    return 1 + c3 * pow(t - 1, 3) + overshoot * pow(t - 1, 2)


# ---------------------------------------------------------------------------
# Smooth scalar lerp (frame-rate independent exponential decay)
# ---------------------------------------------------------------------------

def lerp(current: float, target: float, speed: float, dt: float) -> float:
    """
    Move `current` towards `target` at `speed` (units/s, approx).
    dt is frame delta in seconds.
    Returns new current value.
    """
    diff = target - current
    if abs(diff) < 0.5:
        return target
    return current + diff * min(1.0, speed * dt)


def lerp_int(current: float, target: float, speed: float, dt: float) -> int:
    return int(lerp(current, target, speed, dt))


# ---------------------------------------------------------------------------
# Tween — animate a value from a → b over `duration` seconds
# ---------------------------------------------------------------------------

class Tween:
    def __init__(self, start: float, end: float, duration: float,
                 easing=ease_out_cubic):
        self._start    = start
        self._end      = end
        self._duration = max(duration, 0.001)
        self._elapsed  = 0.0
        self._easing   = easing
        self.value     = float(start)
        self.done      = False

    def update(self, dt: float):
        if self.done:
            return
        self._elapsed += dt
        t = min(self._elapsed / self._duration, 1.0)
        self.value = self._start + (self._end - self._start) * self._easing(t)
        if t >= 1.0:
            self.value = self._end
            self.done  = True

    def reset(self, start: float, end: float, duration: float = None):
        self._start   = start
        self._end     = end
        if duration is not None:
            self._duration = max(duration, 0.001)
        self._elapsed = 0.0
        self.value    = float(start)
        self.done     = False


# ---------------------------------------------------------------------------
# Float pulse (sin wave — for glow / breathing effects)
# ---------------------------------------------------------------------------

class Pulse:
    def __init__(self, period: float = 2.0, lo: float = 0.4, hi: float = 1.0):
        self._period = period
        self._lo     = lo
        self._hi     = hi
        self._t      = 0.0

    def update(self, dt: float) -> float:
        self._t = (self._t + dt) % self._period
        phase   = self._t / self._period * 2 * math.pi
        norm    = (math.sin(phase) + 1) / 2   # 0..1
        return self._lo + norm * (self._hi - self._lo)

    @property
    def value(self) -> float:
        return self._lo + ((math.sin(self._t / self._period * 2 * math.pi) + 1) / 2) * (self._hi - self._lo)
