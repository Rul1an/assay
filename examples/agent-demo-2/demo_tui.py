#!/usr/bin/env python3
"""
SOTA 2025 DX Demo for Verdict (Agent Demo 2)

Usage:
  python3 demo_tui.py record --limit 25
  python3 demo_tui.py verify
  python3 demo_tui.py all --limit 25
  python3 demo_tui.py showcase
  python3 demo_tui.py stress
  python3 demo_tui.py doctor

This script wraps the existing run_demo.py flow but adds a modern rich TUI.
It does NOT require any live LLM calls (OPENAI_API_KEY=mock by default).
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
import textwrap
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, List, Optional, Tuple

from rich import box
from rich.align import Align
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
    """
    Returns (command_prefix, description).
    Prefer VERDICT_BIN, then target/debug/verdict, else cargo run.
    """
    env_bin = os.environ.get("VERDICT_BIN")
    if env_bin:
        p = Path(env_bin)
        if p.exists():
            return ([str(p)], f"VERDICT_BIN={p}")
        return ([env_bin], f"VERDICT_BIN={env_bin} (not found; will still try)")

    local = repo_root / "target" / "debug" / "verdict"
    if local.exists():
        return ([str(local)], f"{local}")

    # fallback: cargo run (slower, but works everywhere)
    return (["cargo", "run", "-q", "-p", "verdict-cli", "--"], "cargo run -p verdict-cli")


def run_streamed(
    title: str,
    cmd: List[str],
    cwd: Path,
    env: Optional[Dict[str, str]] = None,
    log_tail_lines: int = 18,
    live: Optional[Live] = None,
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

    while True:
        line = proc.stdout.readline() if proc.stdout else ""
        if line:
            push_line(line.rstrip("\n"))
        elif proc.poll() is not None:
            break

        if live:
            live.update(render_ui(header, out_tail, None), refresh=True)

    code = proc.wait()
    full_out = "\n".join(full_lines)

    if live:
        status = Text("OK", style="bold green") if code == 0 else Text("FAILED", style="bold red")
        live.update(render_ui(header, out_tail, status), refresh=True)

    return CmdResult(code=code, out_tail=out_tail, full_out=full_out)


def render_ui(header: Text, tail: List[str], status: Optional[Text]) -> Panel:
    log_text = "\n".join(tail) if tail else "(no output yet)"
    log_panel = Panel(
        log_text,
        title="Live output (tail)",
        border_style="dim",
        padding=(1, 2),
        box=box.ROUNDED,
    )

    status_line = Text.assemble(
        ("Status: ", "bold"),
        (status if status else Text("RUNNING", style="bold yellow")),
    )

    body = Group(
        header,
        status_line,
        Rule(style="dim"),
        log_panel,
    )

    return Panel(
        body,
        title="Verdict Demo (SOTA TUI)",
        subtitle="record → ingest → replay-strict → assertions",
        border_style="cyan",
        padding=(1, 2),
        box=box.DOUBLE,
    )


def parse_verdict_summary(output: str) -> Dict[str, int]:
    """
    Best-effort parse: Summary: X passed, Y failed, ... Z error
    """
    out = {"passed": 0, "failed": 0, "error": 0, "skipped": 0, "flaky": 0}
    for line in output.splitlines():
        line = line.strip()
        if line.startswith("Summary:"):
            # Example: Summary: 6 passed, 14 failed, 0 skipped, 0 flaky, 0 unstable, 0 warn, 5 error
            parts = line.replace("Summary:", "").split(",")
            for p in parts:
                p = p.strip()
                try:
                    n_str, key = p.split(" ", 1)
                    n = int(n_str)
                    key = key.strip()
                    if key.startswith("passed"):
                        out["passed"] = n
                    elif key.startswith("failed"):
                        out["failed"] = n
                    elif key.startswith("error"):
                        out["error"] = n
                    elif key.startswith("skipped"):
                        out["skipped"] = n
                    elif key.startswith("flaky"):
                        out["flaky"] = n
                except Exception:
                    continue
    return out


def make_scoreboard(summary: Dict[str, int], extra_notes: List[str]) -> Panel:
    t = Table(box=box.SIMPLE_HEAVY, show_header=True, header_style="bold")
    t.add_column("Metric")
    t.add_column("Value", justify="right")

    def add(metric: str, value: int, style: str):
        t.add_row(metric, Text(str(value), style=style))

    add("Passed", summary.get("passed", 0), "green")
    add("Failed", summary.get("failed", 0), "red")
    add("Errors", summary.get("error", 0), "yellow")
    add("Skipped", summary.get("skipped", 0), "dim")
    add("Flaky", summary.get("flaky", 0), "magenta")

    notes = "\n".join(f"• {n}" for n in extra_notes) if extra_notes else "—"

    return Panel(
        Group(
            Text("Run Summary", style="bold"),
            t,
            Rule(style="dim"),
            Text("Notes", style="bold"),
            Text(notes, style="dim"),
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

    parser = argparse.ArgumentParser(
        prog="demo_tui.py",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        description=textwrap.dedent(
            f"""
            SOTA DX demo runner for Verdict (Agent Demo 2).

            Repo root: {repo_root}
            Verdict:    {verdict_desc}

            Tip: set VERDICT_BIN=/path/to/verdict for faster runs.
            """
        ).strip(),
    )
    sub = parser.add_subparsers(dest="cmd", required=True)

    p_record = sub.add_parser("record", help="Record traces using the demo agent (mock)")
    p_record.add_argument("--limit", type=int, default=25)

    p_verify = sub.add_parser("verify", help="Verify: ingest traces -> verdict ci replay-strict")
    p_verify.add_argument("--trace-file", default="traces/recorded.jsonl")
    p_verify.add_argument("--db", default=".eval/eval.db")

    p_all = sub.add_parser("all", help="Record then verify")
    p_all.add_argument("--limit", type=int, default=25)
    p_all.add_argument("--trace-file", default="traces/recorded.jsonl")
    p_all.add_argument("--db", default=".eval/eval.db")

    p_showcase = sub.add_parser("showcase", help="Showcase: Run 10 'Happy Path' tests (Clean Green)")
    p_showcase.add_argument("--trace-file", default="traces/recorded.jsonl")
    p_showcase.add_argument("--db", default=".eval/eval.db")

    p_stress = sub.add_parser("stress", help="Stress Test: Run full suite (shows safety blocks)")
    p_stress.add_argument("--trace-file", default="traces/recorded.jsonl")
    p_stress.add_argument("--db", default=".eval/eval.db")

    sub.add_parser("doctor", help="Show environment info and suggested commands")

    args = parser.parse_args()

    # Nice header
    header = Panel(
        Group(
            Text("Verdict • Agent Demo 2", style="bold cyan"),
            Text("SOTA 2025 DX runner (rich TUI)", style="dim"),
            Rule(style="dim"),
            Text(f"repo_root: {repo_root}", style="dim"),
            Text(f"verdict:   {verdict_desc}", style="dim"),
            Text(f"cwd:       {here}", style="dim"),
        ),
        border_style="cyan",
        box=box.ROUNDED,
        padding=(1, 2),
    )
    console.print(header)

    if args.cmd == "doctor":
        cmd = " ".join(verdict_prefix + ["--help"])
        snippet = f"""
        Suggested quickstart:

          # build verdict (if needed)
          cargo build

          # record traces (mock)
          python3 demo_tui.py record --limit 25

          # showcase (100% pass)
          python3 demo_tui.py showcase

          # verify (ingest + replay-strict + assertions)
          python3 demo_tui.py verify

        Under the hood:

          python3 run_demo.py record --limit 25
          python3 run_demo.py verify
        """
        console.print(Panel(Syntax(snippet.strip(), "bash", theme="monokai", line_numbers=False), title="Doctor", box=box.ROUNDED))
        console.print(Text(f"Detected verdict runner: {cmd}", style="dim"))
        return 0

    # Progress UI
    progress = Progress(
        SpinnerColumn(),
        TextColumn("[bold]{task.description}"),
        BarColumn(),
        TimeElapsedColumn(),
        transient=True,
    )

    stage_notes: List[str] = []
    summary: Dict[str, int] = {}

    with Live(render_ui(Text("Initializing…", style="bold cyan"), [], None), refresh_per_second=12, console=console) as live:
        with progress:
            steps = []
            if args.cmd in ("record", "all"):
                steps.append(("Record traces", ["python3", "run_demo.py", "record", "--limit", str(getattr(args, "limit", 25))]))

            if args.cmd in ("verify", "all", "showcase", "stress"):
                # run_demo.py verify defaults to verdict.yaml.
                # For showcase/stress, we need to manually construct the verify command or pass args to run_demo.py if it supported it.
                # Since run_demo.py uses "verdict ci" internally with hardcoded config logic relative to args,
                # we might need to invoke verdict directly for custom configs OR rely on run_demo's flexibility.
                # Checking run_demo.py: it does NOT expose config override easily via CLI.
                # So we will invoke `verdict ci` directly here for showcase/stress to ensure config control.

                if args.cmd == "showcase":
                    # Record first? No, assume traces exist or use `all` to record.
                    # Just run verification with showcase config.
                    steps.append(("Showcase Verify (Green)", [
                        *verdict_prefix, "ci",
                        "--config", "verdict-showcase.yaml",
                        "--trace-file", getattr(args, "trace_file", "traces/recorded.jsonl"),
                        "--db", getattr(args, "db", ".eval/eval.db"),
                        "--replay-strict"
                    ]))
                elif args.cmd == "stress":
                    steps.append(("Stress Verify (Full Suite)", [
                        *verdict_prefix, "ci",
                        "--config", "verdict.yaml",
                        "--trace-file", getattr(args, "trace_file", "traces/recorded.jsonl"),
                        "--db", getattr(args, "db", ".eval/eval.db"),
                        "--replay-strict"
                    ]))
                else:
                    # 'verify' or 'all' - use run_demo.py wrapper which handles default config
                    steps.append(("Verify (ingest + replay-strict + assertions)", ["python3", "run_demo.py", "verify"]))

            total = len(steps)
            task_id = progress.add_task("Running stages", total=total)

            final_out = ""
            ok = True

            for (label, cmd) in steps:
                live.update(render_ui(Text(f"Stage: {label}", style="bold cyan"), [], None), refresh=True)
                res = run_streamed(label, cmd=cmd, cwd=here, env={"OPENAI_API_KEY": "mock"}, live=live)
                final_out = res.full_out

                if res.code != 0:
                    ok = False
                    stage_notes.append(f"{label}: failed (exit {res.code})")
                    progress.advance(task_id, 1)
                    break
                else:
                    stage_notes.append(f"{label}: ok")
                    progress.advance(task_id, 1)

            # Try parse scoreboard from the last stage output (verdict ci)
            if final_out:
                summary = parse_verdict_summary(final_out)

            # Final screen
            live.update(
                Panel(
                    Group(
                        Text("Done", style="bold green" if ok else "bold red"),
                        Rule(style="dim"),
                        make_scoreboard(summary, stage_notes),
                        Rule(style="dim"),
                        Text(
                            "Next:\n"
                            "  • If you see E_TRACE_MISS: ensure verdict.yaml prompts match the trace exactly.\n"
                            "  • If assertions fail: either relax assertions (safety-only gate) or make the mock agent smarter.",
                            style="dim",
                        ),
                    ),
                    title="Verdict Demo (SOTA TUI)",
                    border_style="green" if ok else "red",
                    box=box.DOUBLE,
                    padding=(1, 2),
                ),
                refresh=True,
            )

    return 0 if (summary.get("error", 0) == 0 and summary.get("failed", 0) == 0) else 1


if __name__ == "__main__":
    raise SystemExit(main())
