from __future__ import annotations

import time
from dataclasses import dataclass


class Clock:
    def now_ms(self) -> int:
        raise NotImplementedError


class SystemClock(Clock):
    def now_ms(self) -> int:
        return int(time.time() * 1000)


@dataclass
class FrozenClock(Clock):
    t_ms: int = 0
    step_ms: int = 0

    def now_ms(self) -> int:
        cur = self.t_ms
        self.t_ms += self.step_ms
        return cur
