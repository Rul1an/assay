from __future__ import annotations
from dataclasses import dataclass, field
from typing import Any, Dict, Optional
from .clock import Clock, SystemClock
from .writer import TraceWriter

def _step_id(n: int) -> str:
    return f"step_{n:03d}"

@dataclass
class EpisodeRecorder:
    writer: TraceWriter
    episode_id: str
    prompt: str
    test_id: Optional[str] = None
    clock: Clock = field(default_factory=SystemClock)
    meta: Dict[str, Any] = field(default_factory=dict)

    _idx: int = 0
    _step_no: int = 0
    _ended: bool = False

    def __enter__(self) -> "EpisodeRecorder":
        m = dict(self.meta)
        if self.test_id:
            m["test_id"] = self.test_id

        self.writer.write_event({
            "type": "episode_start",
            "episode_id": self.episode_id,
            "timestamp": int(self.clock.now_ms()),
            "input": {"prompt": self.prompt},
            "meta": m,
        })
        return self

    def step(self, *, kind: str, name: str, content: str = "", meta: Optional[Dict[str, Any]] = None) -> str:
        self._step_no += 1
        sid = _step_id(self._step_no)
        self.writer.write_event({
            "type": "step",
            "episode_id": self.episode_id,
            "step_id": sid,
            "idx": self._idx,
            "timestamp": int(self.clock.now_ms()),
            "kind": kind,
            "name": name,
            "content": content,
            "meta": meta or {},
        })
        self._idx += 1
        return sid

    def model_step(self, *, name: str = "openai", content: str = "", meta: Optional[Dict[str, Any]] = None) -> str:
        """Helper for standard model steps"""
        return self.step(kind="model", name=name, content=content, meta=meta)

    def tool_call_result(
        self,
        *,
        step_id: str,
        call_index: int,
        tool_name: str,
        args: Any,
        result: Optional[Any],
        error: Optional[str] = None,
        tool_call_id: Optional[str] = None,
        meta: Optional[Dict[str, Any]] = None,
    ) -> None:
        """Writes a V2 tool_call event with executed result"""
        m = meta or {}
        if tool_call_id:
            m["tool_call_id"] = tool_call_id

        self.writer.write_event({
            "type": "tool_call",
            "episode_id": self.episode_id,
            "step_id": step_id,
            "timestamp": int(self.clock.now_ms()),
            "tool_name": tool_name,
            "call_index": int(call_index),
            "args": args,
            "result": result,
            "error": error,
            "meta": m,
        })

    def tool_call(
        self,
        *,
        tool_name: str,
        args: Dict[str, Any],
        result: Optional[Dict[str, Any]] = None,
        error: Optional[str] = None,
        step_id: Optional[str] = None,
        call_index: int = 0,
        tool_call_id: Optional[str] = None,
        meta: Optional[Dict[str, Any]] = None,
    ) -> None:
        # If no explicit step_id is provided, create an implicit "model" step to attach to
        sid = step_id or self.step(kind="model", name="agent", content="")

        m = meta or {}
        if tool_call_id:
            m["tool_call_id"] = tool_call_id

        self.writer.write_event({
            "type": "tool_call",
            "episode_id": self.episode_id,
            "step_id": sid,
            "timestamp": int(self.clock.now_ms()),
            "tool_name": tool_name,
            "call_index": int(call_index),
            "args": args,
            "result": result,
            "error": error,
            "meta": m,
        })

        return

    def end(self, *, outcome: str = "pass", meta: Optional[Dict[str, Any]] = None) -> None:
        if self._ended:
            return
        self._ended = True
        self.writer.write_event({
            "type": "episode_end",
            "episode_id": self.episode_id,
            "timestamp": int(self.clock.now_ms()),
            "outcome": outcome,
            "meta": meta or {},
        })

    def __exit__(self, exc_type, exc, tb) -> None:
        if not self._ended:
            self.end(outcome="error" if exc else "pass")
