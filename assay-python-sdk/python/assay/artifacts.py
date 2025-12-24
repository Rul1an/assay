from __future__ import annotations

import json
import shutil
from pathlib import Path
from typing import Any, Dict, Optional

from .result import CompareResult, EvalRun, ResultArtifacts


class RunWriter:
    """Manages persistence of EvalRuns and CompareResults to disk."""

    def __init__(self, workdir: Path) -> None:
        self.workdir = workdir
        self.runs_dir = workdir / "runs"
        self.runs_dir.mkdir(parents=True, exist_ok=True)

    def write_run(self, run: EvalRun) -> ResultArtifacts:
        """Persists EvalRun to .eval/runs/<run_id>/."""
        run_dir = self.runs_dir / run.run_id
        run_dir.mkdir(parents=True, exist_ok=True)

        run_json = run_dir / "run.json"
        results_jsonl = run_dir / "results.jsonl"
        junit_xml = run_dir / "junit.xml"
        summary_md = run_dir / "summary.md"
        artifacts_json = run_dir / "artifacts.json"

        self._write_json(run_json, run.to_dict())

        with open(results_jsonl, "w", encoding="utf-8") as f:
            for t in run.tests:
                f.write(json.dumps(t.to_dict()) + "\n")

        if run.artifacts.junit_xml:
            with open(junit_xml, "w", encoding="utf-8") as f:
                f.write(run.to_junit_xml())

        with open(summary_md, "w", encoding="utf-8") as f:
            f.write(run.to_github_summary())

        index_data = {
            "run_id": run.run_id,
            "created_at_ms": run.created_at_ms,
            "paths": {
                "run_json": str(run_json.absolute()),
                "results_jsonl": str(results_jsonl.absolute()),
                "junit_xml": str(junit_xml.absolute()),
                "summary_md": str(summary_md.absolute()),
                "sarif": None,
                "diff_json": None,
                "trace_path": str(run.trace_path.absolute()),
            },
        }
        self._write_json(artifacts_json, index_data)

        # Return updated artifacts object reflecting true paths
        # (Though EvalRun object passed in hasn't changed, callers should know the reliable paths)
        return ResultArtifacts(
            run_json=run_json,
            results_jsonl=results_jsonl,
            junit_xml=junit_xml,
            sarif=None,
            diff_json=None,
            trace_path=run.trace_path,
        )

    def write_diff(self, result: CompareResult) -> None:
        """Persists diff.json to the current run's directory."""
        if not result.current_run_id:
            return
        run_dir = self.runs_dir / result.current_run_id
        if not run_dir.exists():
            run_dir.mkdir(parents=True, exist_ok=True)

        # 1. diff.json
        diff_file = run_dir / "diff.json"
        self._write_json(diff_file, result.to_dict())

        # 2. Update summary.md with comparison details
        summary_path = run_dir / "summary.md"
        with open(summary_path, "w", encoding="utf-8") as f:
            f.write(result.to_github_summary())

        # 3. Update artifacts.json
        index_path = run_dir / "artifacts.json"
        if index_path.exists():
            with open(index_path, "r", encoding="utf-8") as f:
                index_data = json.load(f)
        else:
            # Should exist, but fallback
            index_data = {"run_id": result.current_run_id, "paths": {}}

        index_data["paths"]["diff_json"] = str(diff_file.absolute())
        self._write_json(index_path, index_data)

    def _write_json(self, path: Path, data: Dict[str, Any]) -> None:
        with open(path, "w", encoding="utf-8") as f:
            json.dump(data, f, indent=2, ensure_ascii=False)
