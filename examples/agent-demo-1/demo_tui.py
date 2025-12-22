#!/usr/bin/env python3
"""
SOTA 2025 DX Demo for Verdict (Agent Demo 1)

Usage:
  python3 demo_tui.py all

This script orchestrates the agent-demo-1 flow with a modern rich TUI.
Steps:
1. Record: Run demo_agent.py -> traces/ci.jsonl
2. Ingest: ingest-otel -> traces/replay.jsonl
3. Verify: verdict ci --replay-strict
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
import textwrap
import json
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, List, Optional, Tuple

from rich import box
from rich.console import Console, Group
from rich.live import Live
from rich.panel import Panel
from rich.progress import (
    BarColumn,
    Progress,
    SpinnerColumn,
    TextColumn,
    TimeElapsedColumn,
)
from rich.rule import Rule
from rich.syntax import Syntax
from rich.table import Table
from rich.text import Text

console = Console()


@dataclass
class CmdResult:
    code: int
    out_tail: List[str]
    full_out: str


def find_repo_root(start: Path) -> Path:
    cur = start.resolve()
    for _ in range(8):
        if (cur / "Cargo.toml").exists():
            return cur
        if cur.parent == cur:
            break
        cur = cur.parent
    return start.resolve()


def detect_verdict_bin(repo_root: Path) -> Tuple[List[str], str]:
    env_bin = os.environ.get("VERDICT_BIN")
    if env_bin:
        return ([env_bin], f"VERDICT_BIN={env_bin}")

    local = repo_root / "target" / "debug" / "verdict"
    if local.exists():
        return ([str(local)], f"{local}")

    return (["cargo", "run", "-q", "-p", "verdict-cli", "--"], "cargo run -p verdict-cli")


def run_streamed(
    title: str,
    cmd: List[str],
    cwd: Path,
    env: Optional[Dict[str, str]] = None,
    log_file: Optional[Path] = None,
    log_tail_lines: int = 18,
    live: Optional[Live] = None,
    capture_stderr_for_log: bool = False,
    progress: Optional[Progress] = None,
) -> CmdResult:
    merged_env = os.environ.copy()
    if env:
        merged_env.update(env)

    out_tail: List[str] = []
    full_lines: List[str] = []

    def push_line(line: str):
        full_lines.append(line)
        out_tail.append(line)
        if len(out_tail) > log_tail_lines:
            out_tail.pop(0)

    header = Text(f"▶ {title}", style="bold cyan")

    # Mode 1: Split streams (Record Trace)
    # stdout -> file
    # stderr -> TUI log
    if capture_stderr_for_log and log_file:
        log_file.parent.mkdir(parents=True, exist_ok=True)
        f_handle = open(log_file, "w")

        proc = subprocess.Popen(
            cmd,
            cwd=str(cwd),
            env=merged_env,
            stdout=f_handle,     # Direct to file
            stderr=subprocess.PIPE, # Capture for TUI
            text=True,
            bufsize=1,
            universal_newlines=True,
        )

        # We read stderr for the TUI
        while True:
            line = proc.stderr.readline() if proc.stderr else ""
            if line:
                stripped = line.rstrip("\n")
                push_line(stripped) # Show logs in TUI
            elif proc.poll() is not None:
                break

            if live:
                live.update(render_ui(header, out_tail, None, progress), refresh=True)

        f_handle.close()
        code = proc.wait()
        full_out = "\n".join(full_lines) # Only logs in this case

    # Mode 2: Standard (Merge streams)
    else:
        proc = subprocess.Popen(
            cmd,
            cwd=str(cwd),
            env=merged_env,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            bufsize=1,
            universal_newlines=True,
        )

        f_handle = None
        if log_file:
            log_file.parent.mkdir(parents=True, exist_ok=True)
            f_handle = open(log_file, "w")

        while True:
            line = proc.stdout.readline() if proc.stdout else ""
            if line:
                stripped = line.rstrip("\n")
                push_line(stripped)
                if f_handle:
                    # Write to file if needed (e.g. debug log)
                    f_handle.write(line)
            elif proc.poll() is not None:
                break

            if live:
                live.update(render_ui(header, out_tail, None, progress), refresh=True)

        if f_handle:
            f_handle.close()

        code = proc.wait()
        full_out = "\n".join(full_lines)

    if live:
        status = Text("OK", style="bold green") if code == 0 else Text("FAILED", style="bold red")
        live.update(render_ui(header, out_tail, status, progress), refresh=True)

    return CmdResult(code=code, out_tail=out_tail, full_out=full_out)


def render_ui(header: Text, tail: List[str], status: Optional[Text], progress: Progress) -> Panel:
    log_text = "\n".join(tail) if tail else "(no output yet)"
    log_panel = Panel(
        log_text,
        title="Live output (tail)",
        border_style="dim",
        padding=(0, 1),
        box=box.ROUNDED,
        height=12
    )

    status_line = Text.assemble(
        ("Status: ", "bold"),
        (status if status else Text("RUNNING", style="bold yellow")),
    )

    body = Group(
        header,
        progress,
        status_line,
        log_panel,
    )

    return Panel(
        body,
        title="Verdict Demo 1 (SOTA TUI)",
        subtitle="record → ingest-otel → verify",
        border_style="cyan",
        padding=(1, 2),
        box=box.DOUBLE,
    )


def parse_verdict_summary(output: str) -> Dict[str, int]:
    out = {"passed": 0, "failed": 0, "error": 0}
    for line in output.splitlines():
        if "PASS" in line or "✅" in line: # Support both output styles
            out["passed"] = out.get("passed", 0) + 1
        elif "FAIL" in line or "❌" in line:
            out["failed"] = out.get("failed", 0) + 1
        elif "ERROR" in line or "💥" in line:
            out["error"] = out.get("error", 0) + 1
    return out


def make_scoreboard(summary: Dict[str, int]) -> Panel:
    t = Table(box=box.SIMPLE_HEAVY, show_header=True, header_style="bold")
    t.add_column("Metric")
    t.add_column("Value", justify="right")

    t.add_row("Passed", Text(str(summary.get("passed", 0)), style="green"))
    t.add_row("Failed", Text(str(summary.get("failed", 0)), style="red"))
    t.add_row("Errors", Text(str(summary.get("error", 0)), style="yellow"))

    return Panel(
        Group(
            Text("Run Summary", style="bold"),
            t,
        ),
        title="Scoreboard",
        border_style="green",
        box=box.ROUNDED,
        padding=(1, 2),
    )


def main() -> int:
    here = Path(__file__).resolve().parent
    repo_root = find_repo_root(here)
    verdict_prefix, verdict_desc = detect_verdict_bin(repo_root)

    parser = argparse.ArgumentParser(prog="demo_tui.py", description="SOTA DX demo for Verdict Demo 1")
    sub = parser.add_subparsers(dest="cmd", required=True)
    sub.add_parser("all", help="Run full flow: record -> ingest -> verify")

    args = parser.parse_args()

    # Progress UI
    progress = Progress(
        SpinnerColumn(),
        TextColumn("[bold]{task.description}"),
        BarColumn(),
        TimeElapsedColumn(),
        transient=True,
    )

    summary: Dict[str, int] = {}

    # Clean up previous runs
    (here / "traces").mkdir(exist_ok=True)
    if (here / "traces/ci.jsonl").exists():
        (here / "traces/ci.jsonl").unlink()

    steps = [
        # Set capture_stderr_for_log=True for this step
        ("Record Trace (demo_agent.py)", ["python3", "demo_agent.py"], here / "traces/ci.jsonl", True),

        ("Ingest OTel Trace", [
            *verdict_prefix, "trace", "ingest-otel",
            "--input", "traces/ci.jsonl",
            "--db", ".eval/eval.db",
            "--out-trace", "traces/replay.jsonl",
            "--suite", "demo-agent-suite"
        ], None, False),

        ("Verify (Verdict CI)", [
            *verdict_prefix, "ci",
            "--config", "demo.yaml",
            "--trace-file", "traces/replay.jsonl",
            "--replay-strict"
        ], None, False)
    ]

    task_id = progress.add_task("Running stages", total=len(steps))

    # Pass progress to initial UI
    initial_ui = render_ui(Text("Initializing…", style="bold cyan"), [], None, progress)

    with Live(initial_ui, refresh_per_second=12, console=console) as live:
        final_out = ""
        ok = True

        for (label, cmd, outfile, cap_stderr) in steps:
            # Update header
            live.update(render_ui(Text(f"Stage: {label}", style="bold cyan"), [], None, progress), refresh=True)

            res = run_streamed(label, cmd=cmd, cwd=here, log_file=outfile, live=live, capture_stderr_for_log=cap_stderr, progress=progress)

            if res.code != 0:
                ok = False
                progress.advance(task_id, 1)
                final_out = res.full_out # Capture failure output
                break
            else:
                if "Verify" in label:
                    final_out = res.full_out
                progress.advance(task_id, 1)

        # Parse summary from final verify output
        if final_out:
            summary = parse_verdict_summary(final_out)

        live.update(
            Panel(
                Group(
                    Text("Done", style="bold green" if ok else "bold red"),
                    Rule(style="dim"),
                    make_scoreboard(summary),
                ),
                title="Verdict Demo 1 (SOTA TUI)",
                border_style="green" if ok else "red",
                box=box.DOUBLE,
                padding=(1, 2),
            ),
            refresh=True,
        )

    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
