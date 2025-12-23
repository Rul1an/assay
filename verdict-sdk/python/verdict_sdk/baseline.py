from __future__ import annotations

import json
import shutil
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import List, Optional

from .artifacts import RunWriter
from .errors import BaselineNotFoundError
from .result import EvalRun


@dataclass(frozen=True)
class BaselineRef:
    name: str
    run_id: str
    path: Path
    run: EvalRun


class BaselineStore:
    """Manages named baselines in .eval/baselines/."""

    def __init__(self, workdir: Path) -> None:
        self.workdir = workdir
        self.base_dir = workdir / "baselines"
        self.base_dir.mkdir(parents=True, exist_ok=True)

    def list(self) -> List[str]:
        if not self.base_dir.exists():
            return []
        return sorted([d.name for d in self.base_dir.iterdir() if d.is_dir()])

    def load(self, name: str) -> BaselineRef:
        b_dir = self.base_dir / name
        run_file = b_dir / "run.json"

        if not run_file.exists():
            raise BaselineNotFoundError(name, self.list())

        try:
            with open(run_file, "r", encoding="utf-8") as f:
                data = json.load(f)
            run = EvalRun.from_dict(data)
            return BaselineRef(name=name, run_id=run.run_id, path=run_file, run=run)
        except (json.JSONDecodeError, KeyError, ValueError) as e:
            # Maybe corrupt?
            raise BaselineNotFoundError(name, self.list()) from e

    def save(self, name: str, run: EvalRun, overwrite: bool = True) -> BaselineRef:
        """
        Saves run as baseline 'name'.
        Uses atomic directory swap (create tmp -> rename).
        """
        target_dir = self.base_dir / name

        if target_dir.exists() and not overwrite:
            raise FileExistsError(f"Baseline '{name}' exists and overwrite=False")

        # Atomic Write Pattern
        # 1. Create temp dir separate from target
        tmp_dir = Path(tempfile.mkdtemp(dir=self.base_dir))

        try:
            # 2. Write content (copy run.json and any artifacts needed)
            # For strict baseline, we should probably COPY the artifacts into baseline dir?
            # Or just reference the run in runs/ dir?
            # Design: Self-contained baselines are safer for cache clearing.
            # But deep copy is heavy. For v0.7.0, we just write run.json which points to artifacts paths.
            # Warning: If runs/ is cleaned, baseline breaks.
            # Better: Write run.json into baseline dir.

            run_file = tmp_dir / "run.json"
            with open(run_file, "w", encoding="utf-8") as f:
                json.dump(run.to_dict(), f, indent=2, ensure_ascii=False)

            # 3. Rename atomic swap
            if target_dir.exists():
                shutil.rmtree(
                    target_dir
                )  # Removing dir is risky if not atomic, but rename replacement is implementation dependent

            tmp_dir.rename(target_dir)

            return BaselineRef(
                name=name, run_id=run.run_id, path=target_dir / "run.json", run=run
            )

        except Exception:
            shutil.rmtree(tmp_dir, ignore_errors=True)
            raise
