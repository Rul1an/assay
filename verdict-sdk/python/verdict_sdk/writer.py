from __future__ import annotations
import json
from pathlib import Path
from typing import Any, Callable, Dict, Optional, Union

RedactFn = Callable[[Dict[str, Any]], Dict[str, Any]]

class TraceWriter:
    def __init__(self, path: Union[str, Path], *, redact_fn: Optional[RedactFn] = None):
        self.path = Path(path)
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self._redact_fn = redact_fn

    def write_event(self, event: Dict[str, Any]) -> None:
        e = self._redact_fn(event) if self._redact_fn else event
        # Determinism: sort_keys=True, compact separators
        line = json.dumps(e, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
        with self.path.open("a", encoding="utf-8") as f:
            f.write(line + "\n")
