from __future__ import annotations

from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Any, Dict, List, Optional


@dataclass(frozen=True)
class MetricResult:
    name: str
    value: float
    passed: bool
    threshold: Optional[float] = None
    meta: Optional[Dict[str, Any]] = None

    def to_dict(self) -> Dict[str, Any]:
        return asdict(self)

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> MetricResult:
        return cls(**data)


@dataclass(frozen=True)
class TestResult:
    test_id: str
    prompt: str
    passed: bool
    metrics: List[MetricResult]
    duration_ms: int
    meta: Optional[Dict[str, Any]] = None

    def to_dict(self) -> Dict[str, Any]:
        d = asdict(self)
        d["metrics"] = [m.to_dict() for m in self.metrics]
        return d

    def to_junit_xml_case(self) -> str:
        """
        Convert to a JUnit <testcase> XML string.
        """

        # Minimal escaping for XML
        def escape(s: str) -> str:
            return (
                s.replace("&", "&amp;")
                .replace("<", "&lt;")
                .replace(">", "&gt;")
                .replace('"', "&quot;")
                .replace("'", "&apos;")
            )

        classname = "verdict.tests"
        name = escape(self.test_id)
        time_s = self.duration_ms / 1000.0

        xml = (
            f'    <testcase classname="{classname}" name="{name}" time="{time_s:.3f}">'
        )

        if not self.passed:
            # Aggregate failures
            failed_metrics = [m for m in self.metrics if not m.passed]
            message = "Test failed on metrics: " + ", ".join(
                m.name for m in failed_metrics
            )

            details = []
            for m in failed_metrics:
                details.append(f"{m.name}: value={m.value}, threshold={m.threshold}")
                if m.meta and "rationale" in m.meta:
                    details.append(f"  rationale: {m.meta['rationale']}")

            text = escape("\n".join(details))
            message = escape(message)
            xml += f'\n      <failure message="{message}">{text}</failure>\n    '

        xml += "</testcase>"
        return xml

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> TestResult:
        metrics = [MetricResult.from_dict(m) for m in data.pop("metrics", [])]
        return cls(metrics=metrics, **data)


@dataclass(frozen=True)
class ResultArtifacts:
    run_json: Path
    results_jsonl: Path
    junit_xml: Optional[Path]
    sarif: Optional[Path]
    diff_json: Optional[Path]
    trace_path: Path

    def to_dict(self) -> Dict[str, Any]:
        return {k: str(v) if v is not None else None for k, v in asdict(self).items()}

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> ResultArtifacts:
        # Convert str paths back to Path objects
        kwargs = {}
        for k, v in data.items():
            if v is not None:
                kwargs[k] = Path(v)
            else:
                kwargs[k] = None
        return cls(**kwargs)  # type: ignore


@dataclass(frozen=True)
class EvalRun:
    run_id: str
    suite: str
    created_at_ms: int
    config_path: Optional[Path]
    config_hash: str
    trace_path: Path
    tests: List[TestResult]
    passed: bool
    artifacts: ResultArtifacts
    meta: Optional[Dict[str, Any]] = None

    def to_dict(self) -> Dict[str, Any]:
        d = asdict(self)
        d["config_path"] = str(self.config_path) if self.config_path else None
        d["trace_path"] = str(self.trace_path)
        d["tests"] = [t.to_dict() for t in self.tests]
        d["artifacts"] = self.artifacts.to_dict()
        return d

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> EvalRun:
        # Handle complex types manual reconstruction
        # (Assuming data comes from JSON deserialization)
        tests = [TestResult.from_dict(t) for t in data.pop("tests", [])]
        artifacts = ResultArtifacts.from_dict(data.pop("artifacts", {}))

        config_path = data.pop("config_path", None)
        if config_path:
            config_path = Path(config_path)

        trace_path = Path(data.pop("trace_path"))

        return cls(
            tests=tests,
            artifacts=artifacts,
            config_path=config_path,
            trace_path=trace_path,
            **data,
        )

    def to_junit_xml(self) -> str:
        """
        Convert the entire run into a JUnit XML string.
        """
        import datetime

        failures = sum(1 for t in self.tests if not t.passed)
        total_time_s = sum(t.duration_ms for t in self.tests) / 1000.0
        timestamp = datetime.datetime.fromtimestamp(
            self.created_at_ms / 1000.0
        ).isoformat()

        xml = '<?xml version="1.0" encoding="UTF-8"?>\n'
        xml += "<testsuites>\n"
        xml += (
            f'  <testsuite name="{self.suite}" tests="{len(self.tests)}" '
            f'failures="{failures}" time="{total_time_s:.3f}" timestamp="{timestamp}">\n'
        )

        for t in self.tests:
            xml += t.to_junit_xml_case() + "\n"

        xml += "  </testsuite>\n"
        xml += "</testsuites>\n"
        return xml

    def to_github_summary(self) -> str:
        """
        Generate a GitHub-compatible markdown summary for a single run.
        """
        lines = []
        lines.append(f"## Verdict Run: {'✅ PASS' if self.passed else '❌ FAIL'}")
        lines.append("")
        lines.append(f"**Suite:** `{self.suite}`")
        lines.append(f"**Run ID:** `{self.run_id}`")

        failures = sum(1 for t in self.tests if not t.passed)
        lines.append(f"**Tests:** {len(self.tests)} total, {failures} failed")

        if failures > 0:
            lines.append("")
            lines.append("### Failures")
            for t in self.tests:
                if not t.passed:
                    failed_metrics = [m.name for m in t.metrics if not m.passed]
                    lines.append(
                        f"- `{t.test_id}` failed on: {', '.join(failed_metrics)}"
                    )

        return "\n".join(lines) + "\n"


@dataclass(frozen=True)
class Regression:
    test_id: str
    metric: str
    baseline_value: float
    current_value: float
    delta: float
    threshold: Optional[float] = None
    severity: str = "regression"

    def to_dict(self) -> Dict[str, Any]:
        return asdict(self)

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> Regression:
        return cls(**data)


@dataclass(frozen=True)
class CompareResult:
    passed: bool
    exit_code: int
    summary: str

    baseline: str
    baseline_run_id: Optional[str]
    current_run_id: str

    tests: List[TestResult]
    regressions: List[Regression]

    artifacts: ResultArtifacts

    def __bool__(self) -> bool:
        return self.passed

    def raise_for_status(self) -> None:
        from .errors import RegressionError, VerdictError

        if self.exit_code == 1:
            raise RegressionError(self)
        if self.exit_code == 2:
            raise VerdictError(self.summary)

    def to_github_summary(self) -> str:
        lines = []
        lines.append(f"## Verdict Eval: {'✅ PASS' if self.passed else '❌ FAIL'}")
        lines.append("")
        lines.append(self.summary)
        if self.regressions:
            lines.append("")
            lines.append("### Regressions")
            for r in self.regressions[:20]:
                lines.append(
                    f"- `{r.test_id}` / `{r.metric}`: {r.baseline_value:.3f} → {r.current_value:.3f} "
                    f"({r.delta:+.3f})"
                )
        return "\n".join(lines) + "\n"

    def to_dict(self) -> Dict[str, Any]:
        d = asdict(self)
        d["tests"] = [t.to_dict() for t in self.tests]
        d["regressions"] = [r.to_dict() for r in self.regressions]
        d["artifacts"] = self.artifacts.to_dict()
        return d

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> CompareResult:
        tests = [TestResult.from_dict(t) for t in data.pop("tests", [])]
        regressions = [Regression.from_dict(r) for r in data.pop("regressions", [])]
        artifacts = ResultArtifacts.from_dict(data.pop("artifacts", {}))
        return cls(tests=tests, regressions=regressions, artifacts=artifacts, **data)
