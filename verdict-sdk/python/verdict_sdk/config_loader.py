from __future__ import annotations

import hashlib
import json
from pathlib import Path
from typing import Any, Dict, Optional, Union

from .config import EvalConfig, JudgeConfig, MetricSpec, TestSpec
from .errors import ConfigError

ConfigLike = Union[None, str, Path, Dict[str, Any], EvalConfig]


def _sha256_text(s: str) -> str:
    return hashlib.sha256(s.encode("utf-8")).hexdigest()


def canonical_config_hash(cfg: EvalConfig) -> str:
    d = config_to_dict(cfg)
    txt = json.dumps(d, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
    return _sha256_text(txt)


def load_config(config: ConfigLike) -> EvalConfig:
    if isinstance(config, EvalConfig):
        cfg = config
        cfg.validate()
        return cfg

    raw: Dict[str, Any]
    if config is None:
        for p in (Path("eval.yaml"), Path("verdict.yaml")):
            if p.exists():
                raw = _load_yaml(p)
                cfg = parse_any_config(raw)
                cfg.validate()
                return cfg
        raise ConfigError(
            "No config provided and no eval.yaml/verdict.yaml found in cwd."
        )

    if isinstance(config, (str, Path)):
        p = Path(config)
        raw = _load_yaml(p)
        cfg = parse_any_config(raw)
        cfg.validate()
        return cfg

    if isinstance(config, dict):
        cfg = parse_any_config(config)
        cfg.validate()
        return cfg

    raise ConfigError(f"Unsupported config type: {type(config)!r}")


def _load_yaml(path: Path) -> Dict[str, Any]:
    try:
        import yaml  # type: ignore
    except Exception as e:
        raise ConfigError(
            "PyYAML is required to load YAML configs. "
            "Install it via: pip install pyyaml"
        ) from e

    data = yaml.safe_load(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict):
        raise ConfigError(f"Config at {path} must be a YAML mapping/object.")
    return data


import sys


def parse_any_config(raw: Dict[str, Any]) -> EvalConfig:
    # sys.stderr.write(f"DEBUG RAW: {raw}\n")
    tests = raw.get("tests", [])
    if not isinstance(tests, list):
        raise ConfigError("'tests' must be a list.")

    is_cli_style = False
    for t in tests:
        if isinstance(t, dict) and (
            "input" in t or "expected" in t or "assertions" in t
        ):
            is_cli_style = True
            break

    if is_cli_style:
        return _parse_cli_compat(raw)
    return _parse_sdk_native(raw)


def _parse_sdk_native(raw: Dict[str, Any]) -> EvalConfig:
    judge = _parse_judge(raw.get("judge", {}) or {})
    suite = str(raw.get("suite", "default"))
    version = int(raw.get("version", 1))

    tests: list[TestSpec] = []
    for t in raw.get("tests", []):
        if not isinstance(t, dict):
            raise ConfigError("Each test must be an object.")
        tid = str(t.get("id", "")).strip()
        prompt = str(t.get("prompt", "") or "")
        metrics_raw = t.get("metrics", []) or []

        metrics: list[MetricSpec] = []
        for m in metrics_raw:

            if not isinstance(m, dict):
                raise ConfigError(f"metrics entries must be objects (test={tid}).")
            metrics.append(
                MetricSpec(
                    name=str(m["name"]),
                    kind=str(m.get("kind", "judge")),
                    threshold=m.get("threshold", None),
                    op=str(m.get("op", ">=")),
                    params=dict(m.get("params", {}) or {}),
                    weight=float(m.get("weight", 1.0)),
                )
            )
        tests.append(
            TestSpec(
                id=tid,
                prompt=prompt,
                metrics=metrics,
                tags=list(t.get("tags", []) or []),
                meta=dict(t.get("meta", {}) or {}),
            )
        )

    cfg = EvalConfig(
        version=version,
        suite=suite,
        judge=judge,
        tests=tests,
        meta=dict(raw.get("meta", {}) or {}),
    )
    return cfg


def _parse_cli_compat(raw: Dict[str, Any]) -> EvalConfig:
    judge = _parse_judge(raw.get("judge", {}) or {})
    suite = str(raw.get("suite", "default"))
    version = int(raw.get("version", 1))

    tests: list[TestSpec] = []
    for t in raw.get("tests", []):
        if not isinstance(t, dict):
            raise ConfigError("Each test must be an object.")
        tid = str(t.get("id", "")).strip()

        input_obj = t.get("input", {}) or {}
        if not isinstance(input_obj, dict):
            raise ConfigError(f"test.input must be an object (test={tid}).")
        prompt = str(input_obj.get("prompt", "") or "")

        metrics: list[MetricSpec] = []

        expected = t.get("expected", None)
        if expected is not None:
            if not isinstance(expected, dict):
                raise ConfigError(f"test.expected must be an object (test={tid}).")
            etype = str(expected.get("type", "")).strip()
            params = dict(expected)
            params.pop("type", None)
            metrics.append(MetricSpec(name=etype, kind="builtin", params=params))

        assertions = t.get("assertions", []) or []
        if not isinstance(assertions, list):
            raise ConfigError(f"test.assertions must be a list (test={tid}).")
        for a in assertions:
            if not isinstance(a, dict):
                raise ConfigError(f"assertion must be an object (test={tid}).")
            atype = str(a.get("type", "")).strip()
            params = dict(a)
            params.pop("type", None)
            metrics.append(MetricSpec(name=atype, kind="builtin", params=params))

        tests.append(TestSpec(id=tid, prompt=prompt, metrics=metrics))

    return EvalConfig(version=version, suite=suite, judge=judge, tests=tests)


def _parse_judge(raw: Dict[str, Any]) -> JudgeConfig:
    if not isinstance(raw, dict):
        raise ConfigError("judge must be an object.")
    return JudgeConfig(
        provider=str(raw.get("provider", "openai")),
        model=str(raw.get("model", "gpt-4o")),
        temperature=float(raw.get("temperature", 0.0)),
        prompt=str(raw.get("prompt", "faithfulness")),
        prompt_version=int(raw.get("prompt_version", 1)),
        cache=bool(raw.get("cache", True)),
        cache_dir=str(raw.get("cache_dir", ".eval/judge_cache")),
        k=int(raw.get("k", 1)),
        allow_empty_context=bool(raw.get("allow_empty_context", False)),
    )


def config_to_dict(cfg: EvalConfig) -> Dict[str, Any]:
    return {
        "version": cfg.version,
        "suite": cfg.suite,
        "judge": {
            "provider": cfg.judge.provider,
            "model": cfg.judge.model,
            "temperature": cfg.judge.temperature,
            "prompt": cfg.judge.prompt,
            "prompt_version": cfg.judge.prompt_version,
            "cache": cfg.judge.cache,
            "cache_dir": cfg.judge.cache_dir,
            "k": cfg.judge.k,
            "allow_empty_context": cfg.judge.allow_empty_context,
        },
        "tests": [
            {
                "id": t.id,
                "prompt": t.prompt,
                "tags": list(t.tags),
                "meta": dict(t.meta),
                "metrics": [
                    {
                        "name": m.name,
                        "kind": m.kind,
                        "threshold": m.threshold,
                        "op": m.op,
                        "weight": m.weight,
                        "params": dict(m.params),
                    }
                    for m in t.metrics
                ],
            }
            for t in cfg.tests
        ],
        "meta": dict(cfg.meta),
    }
