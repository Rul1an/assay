from __future__ import annotations
import json
from pathlib import Path
from typing import Any, Dict, Union

class TraceWriter:
    def __init__(self, path: Union[str, Path]):
        self.path = Path(path)
        self.path.parent.mkdir(parents=True, exist_ok=True)

    def write_event(self, event: Dict[str, Any]) -> None:
        # Determinism: sort_keys=True, compact separators
        line = json.dumps(event, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
        with self.path.open("a", encoding="utf-8") as f:
            f.write(line + "\n")
