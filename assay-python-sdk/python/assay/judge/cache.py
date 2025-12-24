from __future__ import annotations

import hashlib
import json
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any, Dict, Optional

from .client import JudgeClient
from .types import JudgeRequest, JudgeResponse


def _sha(x: str) -> str:
    return hashlib.sha256(x.encode("utf-8")).hexdigest()


def _canon(obj: Any) -> str:
    return json.dumps(obj, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


@dataclass
class CachedJudge(JudgeClient):
    inner: JudgeClient
    cache_dir: Path

    def __post_init__(self) -> None:
        self.cache_dir.mkdir(parents=True, exist_ok=True)

    def _key(self, req: JudgeRequest) -> str:
        payload = {
            "metric": req.metric.name,
            "question": req.question,
            "answer": req.answer,
            "context": req.context,
            # include spec params if they exist?
            # Yes, if params change (e.g. k=3), cache should break
            "params": req.metric.params,
        }
        return _sha(_canon(payload))

    def evaluate(self, req: JudgeRequest) -> JudgeResponse:
        key = self._key(req)
        p = self.cache_dir / f"{key}.json"
        if p.exists():
            try:
                data = json.loads(p.read_text(encoding="utf-8"))
                return JudgeResponse(**data)
            except Exception:
                pass

        res = self.inner.evaluate(req)
        # Serialize JudgeResponse
        data = {
            "score": res.score,
            "passed": res.passed,
            "rationale": res.rationale,
            "raw": res.raw,
        }
        p.write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding="utf-8")
        return res
