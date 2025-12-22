#!/usr/bin/env python3
"""
Verdict Function Calling Demo - Quick Start
============================================

This demo shows how Verdict catches dangerous LLM tool calls BEFORE production.

The Problem (Dec 2025 reality):
- 39% of companies report AI agents accessing unintended systems
- 32% saw agents allowing inappropriate data access
- "Your AI sales agent just gave a 50% discount" happens more than you'd think

The Solution:
- Verdict gates PRs on tool call assertions
- Deterministic replay for CI (no flaky tests)
- BFCL-style AST verification (no actual tool execution needed)

Usage:
    # Record traces from your agent
    python run_demo.py record

    # Run Verdict CI checks
    python run_demo.py verify

    # Full demo (record + verify)
    python run_demo.py demo

References:
- BFCL V4: gorilla.cs.berkeley.edu/leaderboard.html
- OTel GenAI: opentelemetry.io/docs/specs/semconv/gen-ai/
- OWASP LLM Top 10 2025
"""

import asyncio
import json
import os
import sys
from datetime import datetime
from pathlib import Path

# Add parent to path for imports
sys.path.insert(0, str(Path(__file__).parent))

from scenarios import ALL_SCENARIOS, get_scenarios_by_category, Scenario
from agent import FunctionCallingAgent, save_trace, episode_to_jsonl, Episode


# Colors for terminal output
class Colors:
    GREEN = '\033[92m'
    RED = '\033[91m'
    YELLOW = '\033[93m'
    BLUE = '\033[94m'
    CYAN = '\033[96m'
    BOLD = '\033[1m'
    END = '\033[0m'


def print_header(text: str):
    print(f"\n{Colors.BOLD}{Colors.BLUE}{'='*60}{Colors.END}")
    print(f"{Colors.BOLD}{Colors.BLUE}{text}{Colors.END}")
    print(f"{Colors.BOLD}{Colors.BLUE}{'='*60}{Colors.END}\n")


def print_section(text: str):
    print(f"\n{Colors.CYAN}▶ {text}{Colors.END}")


def print_pass(text: str):
    print(f"  {Colors.GREEN}✓ {text}{Colors.END}")


def print_fail(text: str):
    print(f"  {Colors.RED}✗ {text}{Colors.END}")


def print_warn(text: str):
    print(f"  {Colors.YELLOW}⚠ {text}{Colors.END}")


async def record_scenarios(
    scenarios: list[Scenario],
    output_dir: str = "traces",
    limit: int | None = None
) -> list[Episode]:
    """Record traces for all scenarios."""
    os.makedirs(output_dir, exist_ok=True)

    # Check for API key
    if not os.getenv("OPENAI_API_KEY"):
        print_warn("OPENAI_API_KEY not set!")
        print("\n  Enter your OpenAI API Key to run real traces, or press Enter for Mock Mode:")
        key = input("  > ").strip()
        if key.startswith("sk-"):
            os.environ["OPENAI_API_KEY"] = key
            print_pass("API Key set for this session.")
        else:
             print_warn("Using Mock Mode.")

    agent = FunctionCallingAgent()
    episodes = []

    if limit:
        scenarios = scenarios[:limit]

    print_section(f"Recording {len(scenarios)} scenarios...")

    for i, scenario in enumerate(scenarios, 1):
        print(f"\n  [{i}/{len(scenarios)}] {scenario.id}")
        print(f"      Input: {scenario.input[:50]}...")

        try:
            episode = await agent.run(scenario.input, episode_id=scenario.id)
            episodes.append((scenario, episode))

            # Extract tool calls
            tool_calls = [
                s.tool_call.tool_name
                for s in episode.steps
                if s.tool_call
            ]

            if tool_calls:
                print(f"      Tools: {', '.join(tool_calls)}")
            else:
                print(f"      Tools: (none)")

            print(f"      Tokens: {episode.total_tokens}")

        except Exception as e:
            print_fail(f"Error: {e}")
            continue

    # Save all traces to single file
    trace_file = f"{output_dir}/recorded.jsonl"
    with open(trace_file, "w") as f:
        for i, (scenario, episode) in enumerate(episodes):
             lines = episode_to_jsonl(episode, test_id=scenario.id)
             for line in lines:
                 f.write(line + "\n")

    print_pass(f"Saved {len(episodes)} traces to {trace_file}")
    return episodes


import subprocess

def verify_traces(trace_file: str = "traces/recorded.jsonl") -> tuple[int, int, list]:
    """
    Verify traces using Verdict CLI.
    """
    if not os.path.exists(trace_file):
        print_fail(f"Trace file not found: {trace_file}")
        print("  Run: python run_demo.py record")
        return 0, 0, []

    print_section(f"Ingesting traces from {trace_file}...")

    # Ingest to DB
    # Using .eval/eval.db to match typical CI setup
    db_path = ".eval/eval.db"
    if os.path.exists(db_path):
        os.remove(db_path)

    # We also need to ensure parent dir exists for DB
    os.makedirs(os.path.dirname(db_path), exist_ok=True)

    cmd_ingest = [
        "../../target/debug/verdict", "trace", "ingest",
        "--input", trace_file,
        "--output", db_path
    ]

    res = subprocess.run(cmd_ingest, capture_output=True, text=True)
    if res.returncode != 0:
        print_fail(f"Ingest failed: {res.stderr}")
        return 0, 1, []

    print_pass(f"Ingested to {db_path}")

    print_section("Running Verdict CI...")
    cmd_ci = [
        "../../target/debug/verdict", "ci",
        "--config", "verdict.yaml",
        "--db", db_path,
        "--trace-file", trace_file,
        "--replay-strict"
    ]

    # We allow CI to fail (return non-zero) if tests fail
    res = subprocess.run(cmd_ci, capture_output=True, text=True)

    print(res.stdout)
    if res.stderr:
        print(f"{Colors.YELLOW}{res.stderr}{Colors.END}")

    if res.returncode == 0:
        return 1, 0, [] # All good
    else:
        return 0, 1, [] # Failed


def generate_junit_report(passed: int, failed: int, failures: list, output: str = "reports/junit.xml"):
    """Generate JUnit XML report for CI integration."""
    os.makedirs(os.path.dirname(output) or ".", exist_ok=True)

    xml = f'''<?xml version="1.0" encoding="UTF-8"?>
<testsuites>
  <testsuite name="function-calling-safety" tests="{passed + failed}" failures="{failed}" timestamp="{datetime.now().isoformat()}">
'''

    for f in failures:
        xml += f'''    <testcase name="{f['scenario']}" classname="safety">
      <failure message="Tool call assertion failed">
{json.dumps(f, indent=2)}
      </failure>
    </testcase>
'''

    xml += '''  </testsuite>
</testsuites>
'''

    with open(output, "w") as f:
        f.write(xml)

    print_pass(f"JUnit report: {output}")


def print_summary(passed: int, failed: int, failures: list):
    """Print test summary with Verdict value proposition."""
    total = passed + failed

    print_header("Test Summary")

    if failed == 0:
        print(f"  {Colors.GREEN}✓ All {total} tests passed{Colors.END}")
    else:
        print(f"  {Colors.RED}✗ {failed}/{total} tests failed{Colors.END}")
        print(f"  {Colors.GREEN}✓ {passed}/{total} tests passed{Colors.END}")

    if failures:
        print_section("Failures Detail")
        for f in failures[:5]:  # Show first 5
            print(f"\n  {Colors.RED}{f['scenario']}{Colors.END}")
            if f.get("violations"):
                for v in f["violations"]:
                    print(f"    • {v}")
            if f.get("missing"):
                for m in f["missing"]:
                    print(f"    • {m}")
            print(f"    Actual tools: {f['tool_calls']}")

    # Value proposition
    print_header("What Verdict Caught")

    safety_failures = [f for f in failures if "ApplyDiscount" in str(f) or "DeleteAccount" in str(f)]
    if safety_failures:
        print(f"  {Colors.RED}🚨 {len(safety_failures)} dangerous tool calls would have reached production!{Colors.END}")
        print()
        print("  Without Verdict:")
        print("    → Customer demands 50% discount... agent applies it")
        print("    → Social engineering attack... account deleted")
        print("    → Prompt injection... unauthorized refund executed")
        print()
        print("  With Verdict in CI:")
        print(f"    → {Colors.GREEN}PR blocked{Colors.END} - regression caught before merge")
        print(f"    → {Colors.GREEN}Zero production incidents{Colors.END}")
    else:
        print(f"  {Colors.GREEN}✓ All dangerous tool calls properly blocked{Colors.END}")
        print()
        print("  Your agent correctly:")
        print("    → Refused unauthorized discounts")
        print("    → Blocked account deletion attempts")
        print("    → Rejected prompt injection attacks")


def print_usage():
    print("""
Verdict Function Calling Demo
=============================

Commands:
  record    Record traces from live agent (requires OPENAI_API_KEY)
  verify    Verify recorded traces against assertions
  demo      Full demo: record + verify
  scenarios List all test scenarios
  yaml      Generate verdict.yaml config

Examples:
  # Set API key and run full demo
  export OPENAI_API_KEY=sk-...
  python run_demo.py demo

  # Just verify existing traces
  python run_demo.py verify

  # Record only safety scenarios
  python run_demo.py record --category safety --limit 5

What This Demo Shows:
  1. Agent makes tool calls based on user input
  2. Verdict captures traces in JSONL format
  3. Verdict verifies traces against assertions
  4. CI gates PRs on assertion failures

  The key insight: You can catch "agent gave unauthorized discount"
  BEFORE it happens in production, not after.
""")


async def main():
    if len(sys.argv) < 2:
        print_usage()
        sys.exit(0)

    command = sys.argv[1]

    if command == "record":
        print_header("Recording Agent Traces")

        # Parse options
        limit = None
        category = None

        if "--limit" in sys.argv:
            idx = sys.argv.index("--limit")
            limit = int(sys.argv[idx + 1])

        if "--category" in sys.argv:
            idx = sys.argv.index("--category")
            category = sys.argv[idx + 1]

        # Get scenarios
        if category:
            categories = get_scenarios_by_category()
            scenarios = categories.get(category, [])
            if not scenarios:
                print_fail(f"Unknown category: {category}")
                print(f"  Available: {', '.join(categories.keys())}")
                sys.exit(1)
        else:
            scenarios = ALL_SCENARIOS

        await record_scenarios(scenarios, limit=limit)

        print()
        print("Next step: python run_demo.py verify")

    elif command == "verify":
        print_header("Verifying Traces Against Assertions")

        trace_file = "traces/recorded.jsonl"
        if "--trace" in sys.argv:
            idx = sys.argv.index("--trace")
            trace_file = sys.argv[idx + 1]

        passed, failed, failures = verify_traces(trace_file)

        if passed + failed > 0:
            generate_junit_report(passed, failed, failures)
            print_summary(passed, failed, failures)

            # Exit code for CI
            if failed > 0:
                sys.exit(1)

    elif command == "demo":
        print_header("Verdict Function Calling Demo")
        print("This demo shows how Verdict catches dangerous tool calls in CI.")
        print()
        print("Step 1: Record agent traces")
        print("Step 2: Verify against safety assertions")
        print("Step 3: See what Verdict would have caught")
        print()
        input("Press Enter to start...")

        # Record a subset for demo speed
        print_header("Step 1: Recording Agent Traces")
        categories = get_scenarios_by_category()

        # Mix of happy path and safety scenarios
        demo_scenarios = (
            categories["happy_path"][:3] +
            categories["safety"][:3] +
            categories["adversarial"][:2]
        )

        await record_scenarios(demo_scenarios)

        print()
        input("Press Enter to verify traces...")

        # Verify
        print_header("Step 2: Verifying Against Assertions")
        passed, failed, failures = verify_traces()

        if passed + failed > 0:
            generate_junit_report(passed, failed, failures)
            print_summary(passed, failed, failures)

    elif command == "scenarios":
        print_header("Available Test Scenarios")
        categories = get_scenarios_by_category()

        for cat, scenarios in categories.items():
            print(f"\n{Colors.CYAN}{cat.upper()} ({len(scenarios)} tests){Colors.END}")
            for s in scenarios:
                expected = ", ".join(s.expected_tools) if s.expected_tools else "none"
                forbidden = ", ".join(s.forbidden_tools) if s.forbidden_tools else "none"
                print(f"  • {s.id}")
                print(f"    {Colors.GREEN}Expects:{Colors.END} {expected}")
                print(f"    {Colors.RED}Blocks:{Colors.END} {forbidden}")

        print(f"\n{Colors.BOLD}Total: {len(ALL_SCENARIOS)} scenarios{Colors.END}")

    elif command == "yaml":
        # Output scenarios as verdict.yaml


        print("# Auto-generated Verdict config")
        print("# Run: verdict ci --config verdict.yaml")
        print()
        print("version: 1")
        print("suite: function-calling-safety")
        print()
        print("tests:")

        for s in ALL_SCENARIOS:
            print(f"  - id: {s.id}")
            print(f'    description: "{s.description}"')
            print(f"    input:")
            # Escape quotes in input
            escaped = s.input.replace('"', '\\"')
            print(f'      prompt: "{escaped[:80]}"')
            print(f"    expected:")

            for tool in s.expected_tools:
                print(f"      - type: trace_must_call_tool")
                print(f"        tool: {tool}")

            for tool in s.forbidden_tools:
                print(f"      - type: trace_must_not_call_tool")
                print(f"        tool: {tool}")

            print()

    else:
        print_fail(f"Unknown command: {command}")
        print_usage()
        sys.exit(1)


if __name__ == "__main__":
    asyncio.run(main())
