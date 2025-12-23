from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Dict, List, Literal, Optional

from .errors import ConfigError

MetricKind = Literal["builtin", "judge"]
CompareOp = Literal[">=", ">", "<=", "<"]


@dataclass(frozen=True)
class JudgeConfig:
    provider: str = "openai"
    model: str = "gpt-4o"
    temperature: float = 0.0
    prompt: str = "faithfulness"
    prompt_version: int = 1
    cache: bool = True
    cache_dir: str = ".eval/judge_cache"
    k: int = 1
    allow_empty_context: bool = False


@dataclass(frozen=True)
class MetricSpec:
    name: str
    kind: MetricKind
    threshold: Optional[float] = None
    op: CompareOp = ">="
    params: Dict[str, Any] = field(default_factory=dict)
    weight: float = 1.0


@dataclass(frozen=True)
class TestSpec:
    id: str
    prompt: str
    metrics: List[MetricSpec] = field(default_factory=list)
    tags: List[str] = field(default_factory=list)
    meta: Dict[str, Any] = field(default_factory=dict)


@dataclass(frozen=True)
class EvalConfig:
    version: int = 1
    suite: str = "default"
    judge: JudgeConfig = field(default_factory=JudgeConfig)
    tests: List[TestSpec] = field(default_factory=list)
    meta: Dict[str, Any] = field(default_factory=dict)

    def validate(self) -> None:
        if self.version != 1:
            raise ConfigError(f"Unsupported config version: {self.version}")
        if not self.tests:
            raise ConfigError("Config has no tests.")
        ids = [t.id for t in self.tests]
        if len(ids) != len(set(ids)):
            raise ConfigError("Duplicate test ids found in config.")
        for t in self.tests:
            if not t.prompt:
                raise ConfigError(f"Test '{t.id}' missing prompt.")
            for m in t.metrics:
                if m.kind not in ("builtin", "judge"):
                    raise ConfigError(
                        f"Invalid metric kind '{m.kind}' in test '{t.id}'."
                    )
