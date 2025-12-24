from __future__ import annotations

import sys
import time
from dataclasses import dataclass
from math import nan
from pathlib import Path
from typing import Any, Dict, List, Optional, Sequence, Tuple, Union

from .artifacts import RunWriter
from .baseline import BaselineRef, BaselineStore
from .config import EvalConfig, MetricSpec
from .config_loader import canonical_config_hash, load_config
from .errors import (BaselineIncompatibleError, BaselineNotFoundError,
                     ConfigError)
from .judge.client import JudgeClient
from .metrics.ops import passes_threshold
from .result import (CompareResult, EvalRun, MetricResult, Regression,
                     ResultArtifacts, TestResult)

ConfigLike = Union[None, str, Path, Dict[str, Any], EvalConfig]
TraceLike = Union[str, Path]


@dataclass(frozen=True)
class EvaluatorOptions:
    workdir: Union[str, Path] = ".eval"
    cache: bool = True
    strict: bool = False
    baseline_overwrite: bool = True


class Evaluator:
    """v0.7.0 SDK-first evaluation."""

    def __init__(
        self,
        config: ConfigLike = None,
        *,
        workdir: Union[str, Path] = ".eval",
        cache: bool = True,
        strict: bool = False,
        baseline_overwrite: bool = True,
    ) -> None:
        self._config = load_config(config)
        self._options = EvaluatorOptions(
            workdir=workdir,
            cache=cache,
            strict=strict,
            baseline_overwrite=baseline_overwrite,
        )
        self._workdir_path = Path(workdir)
        self._writer = RunWriter(self._workdir_path)
        self._store = BaselineStore(self._workdir_path)

        # Initialize Judge (Phase 3.3 Production)
        self._judge: Optional[JudgeClient] = None
        has_judge_metrics = any(
            m.kind == "judge" for t in self._config.tests for m in t.metrics
        )

        if has_judge_metrics:
            from .judge.openai_judge import OpenAI, OpenAIJudge

            # Check OpenAI availability
            if OpenAI is None:
                raise ConfigError(
                    "Judge metrics used, but 'openai' package not installed."
                )

            client = OpenAI()  # env var OPENAI_API_KEY
            base_judge = OpenAIJudge(client=client, model=self._config.judge.model)

            if self._config.judge.cache:
                from .judge.cache import CachedJudge

                # Use configured cache dir or default in workdir
                cdir = Path(self._config.judge.cache_dir)
                if not cdir.is_absolute():
                    cdir = self._workdir_path / "judge_cache"  # Default relative path

                self._judge = CachedJudge(base_judge, cdir)
            else:
                self._judge = base_judge

    @property
    def config(self) -> EvalConfig:
        return self._config

    @property
    def workdir(self) -> Path:
        return self._workdir_path

    def run(
        self,
        trace: TraceLike,
        *,
        test_ids: Optional[Sequence[str]] = None,
        tags: Optional[Sequence[str]] = None,
    ) -> EvalRun:
        start_ms = _now_ms()

        trace_path = Path(trace).absolute()

        from .trace_reader import extract_last_model_content, read_events

        all_events = read_events(trace_path)
        episode_events = all_events

        episode_answer = extract_last_model_content(episode_events)

        test_results = []
        full_pass = True

        from .judge.types import JudgeRequest
        from .metrics.builtin import eval_builtin

        for t in self._config.tests:
            if test_ids and t.id not in test_ids:
                continue

            metrics_out = []
            t_pass = True

            for m in t.metrics:
                m_res = None

                if m.kind == "builtin":
                    m_res = eval_builtin(m, episode_events)

                elif m.kind == "judge":
                    if self._judge is None:
                        # Should be caught by init, but as failsafe:
                        m_res = MetricResult(
                            name=m.name,
                            value=0.0,
                            passed=False,
                            meta={"error": "Judge not configured"},
                        )
                    else:
                        try:
                            req = JudgeRequest(
                                metric=m,
                                question=t.prompt,
                                answer=episode_answer,
                                context="",
                            )
                            jr = self._judge.evaluate(req)

                            val = float(jr.score)
                            passed = bool(jr.passed)
                            if m.threshold is not None:
                                passed = passes_threshold(val, float(m.threshold), m.op)

                            m_res = MetricResult(
                                name=m.name,
                                value=val,
                                passed=passed,
                                threshold=m.threshold,
                                meta={
                                    "judge_passed": jr.passed,
                                    "rationale": jr.rationale,
                                    "raw": jr.raw,
                                },
                            )
                        except Exception as e:
                            # snippet for debugging
                            m_res = MetricResult(
                                name=m.name,
                                value=0.0,
                                passed=False,
                                meta={"error": str(e), "error_type": type(e).__name__},
                            )
                else:
                    m_res = MetricResult(
                        name=m.name,
                        value=0.0,
                        passed=False,
                        meta={"error": "unknown_kind"},
                    )

                # CRITICAL: Always append
                metrics_out.append(m_res)

                if not m_res.passed:
                    t_pass = False

            if not t_pass:
                full_pass = False

            test_results.append(
                TestResult(
                    test_id=t.id,
                    prompt=t.prompt,
                    passed=t_pass,
                    metrics=metrics_out,
                    duration_ms=10,
                )
            )

        run_id = f"run_{start_ms}"

        artifacts = ResultArtifacts(
            run_json=self._writer.runs_dir / run_id / "run.json",
            results_jsonl=self._writer.runs_dir / run_id / "results.jsonl",
            junit_xml=self._writer.runs_dir / run_id / "junit.xml",
            sarif=None,
            diff_json=None,
            trace_path=trace_path,
        )

        run = EvalRun(
            run_id=run_id,
            suite=self._config.suite,
            created_at_ms=start_ms,
            config_path=None,
            config_hash=canonical_config_hash(self._config),
            trace_path=trace_path,
            tests=test_results,
            passed=full_pass,
            artifacts=artifacts,
        )

        self._writer.write_run(run)
        return run

    def save_baseline(
        self,
        name: str,
        run: EvalRun,
        *,
        overwrite: Optional[bool] = None,
    ) -> BaselineRef:
        ov = overwrite if overwrite is not None else self._options.baseline_overwrite
        return self._store.save(name, run, overwrite=ov)

    def compare(
        self,
        trace: TraceLike,
        *,
        baseline: str = "main",
        create_if_missing: bool = False,
        fail_on_regression: bool = True,
        test_ids: Optional[Sequence[str]] = None,
        tags: Optional[Sequence[str]] = None,
    ) -> CompareResult:

        current = self.run(trace, test_ids=test_ids, tags=tags)

        try:
            base_ref = self._store.load(baseline)
            base_run = base_ref.run
        except BaselineNotFoundError:
            if create_if_missing:
                self.save_baseline(baseline, current)
                return CompareResult(
                    passed=True,
                    exit_code=0,
                    summary=f"Baseline '{baseline}' created.",
                    baseline=baseline,
                    baseline_run_id=None,
                    current_run_id=current.run_id,
                    tests=current.tests,
                    regressions=[],
                    artifacts=current.artifacts,
                )
            else:
                raise

        # Helpers
        def _index_run(run: EvalRun) -> Dict[Tuple[str, str], MetricResult]:
            out = {}
            for tr in run.tests:
                for mr in tr.metrics:
                    out[(tr.test_id, mr.name)] = mr
            return out

        def _keys(run: EvalRun) -> set[tuple[str, str]]:
            return {(t.test_id, m.name) for t in run.tests for m in t.metrics}

        def _metric_spec_map(cfg: EvalConfig) -> Dict[Tuple[str, str], MetricSpec]:
            out = {}
            for t in cfg.tests:
                for m in t.metrics:
                    out[(t.id, m.name)] = m
            return out

        base_idx = _index_run(base_run)
        spec_map = _metric_spec_map(self._config)

        # Strict Mode Check
        missing_in_baseline = sorted(list(_keys(current) - _keys(base_run)))
        if missing_in_baseline and self._options.strict:
            raise BaselineIncompatibleError(baseline, missing_in_baseline)

        regressions: List[Regression] = []
        is_pass = True

        if not current.passed:
            is_pass = False

        for tr in current.tests:
            for mr in tr.metrics:
                key = (tr.test_id, mr.name)
                spec = spec_map.get(key)
                base_mr = base_idx.get(key)

                b_val = base_mr.value if base_mr else nan
                delta = (mr.value - base_mr.value) if base_mr else nan

                # 1) Threshold gate (Absolute floor) - Always checked
                if spec and spec.threshold is not None:
                    if not passes_threshold(mr.value, float(spec.threshold), spec.op):
                        regressions.append(
                            Regression(
                                test_id=tr.test_id,
                                metric=mr.name,
                                baseline_value=b_val,
                                current_value=mr.value,
                                delta=delta,
                                threshold=float(spec.threshold),
                                severity="regression",
                            )
                        )
                        is_pass = False
                        continue

                # 2) If Baseline Missing: Handle per Policy
                if base_mr is None:
                    if self._options.strict:
                        raise BaselineIncompatibleError(
                            baseline, [(tr.test_id, mr.name)]
                        )
                    else:
                        continue

                # 3) Baseline regression gate (Relative drop)
                if spec and base_mr:
                    max_reg = spec.params.get("max_regression", None)
                    if max_reg is not None:
                        max_reg_val = float(max_reg)
                        if delta < -max_reg_val:
                            regressions.append(
                                Regression(
                                    test_id=tr.test_id,
                                    metric=mr.name,
                                    baseline_value=base_mr.value,
                                    current_value=mr.value,
                                    delta=delta,
                                    threshold=None,
                                    severity="regression",
                                )
                            )
                            is_pass = False
                            continue

        regressions.sort(key=lambda r: (r.test_id, r.metric))

        passed_ct = sum(1 for t in current.tests if t.passed)
        total_ct = len(current.tests)

        icon = "✅" if is_pass else "❌"
        reg_msg = f"{len(regressions)} regressions" if regressions else "0 regressions"
        summary = (
            f"{icon} {total_ct} tests, {passed_ct} passed. {reg_msg} vs '{baseline}'."
        )

        result = CompareResult(
            passed=is_pass,
            exit_code=0 if is_pass else 1,
            summary=summary,
            baseline=baseline,
            baseline_run_id=base_run.run_id,
            current_run_id=current.run_id,
            tests=current.tests,
            regressions=regressions,
            artifacts=current.artifacts,
        )

        self._writer.write_diff(result)
        return result


def _now_ms() -> int:
    return int(time.time() * 1000)
