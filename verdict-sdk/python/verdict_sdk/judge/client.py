from __future__ import annotations

from typing import Protocol, runtime_checkable

from .types import JudgeRequest, JudgeResponse


@runtime_checkable
class JudgeClient(Protocol):
    def evaluate(self, req: JudgeRequest) -> JudgeResponse: ...
