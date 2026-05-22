#!/usr/bin/env python3
"""Identity preservation probe for the Gemini Python google-genai second-runtime line.

This script implements the "Identity preservation probe (first implementation step)"
required by https://github.com/Rul1an/assay/issues/1307. It verifies that the
identity assumption the candidate selection (#1305) rests on holds in practice:

- Gemini's gemini-3.5-flash API returns a populated FunctionCall.id for every
  function-call in a generate_content() response
- The id is byte-stable between live record and offline cassette replay
- The google-genai SDK preserves the id without client-side synthesis

The probe is **maintainer-only curation**. It must be run by a maintainer with a
Gemini API key in a controlled environment, before any fixture/acceptance work
is built on top of it. Delegated CI does not run this probe.

Usage:

    # one-time setup (maintainer workstation):
    python3 -m venv .venv
    source .venv/bin/activate
    pip install -r requirements.txt

    # record one cassette using a live API key (key never written to disk):
    GEMINI_API_KEY=<live-key> python3 identity_probe.py --record

    # replay the recorded cassette with no key, no network:
    python3 identity_probe.py --replay

    # one-shot record + replay + compare:
    GEMINI_API_KEY=<live-key> python3 identity_probe.py --record-and-replay

The probe writes its outcome to probe-results/<UTC-date>.json. That outcome
file is the artifact the maintainer commits alongside the cassette.

Per the kill criteria in https://github.com/Rul1an/assay/issues/1307, if the
probe fails (id absent, id mismatched between record and replay, or SDK
behaves differently than the typed source claims), the implementation PR
must stop and either:

- open a follow-up evaluation PR that updates the Gemini candidate outcome
  in second-runtime-candidate-selection.md, or
- open a separate decision PR for the relevant follow-up issue.

Do not work around a failing probe by synthesizing a tool_call_id or by
suppressing the assertion.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

# Strict import order: google.genai and vcr must be present from the fixture-
# local requirements.txt. Failures here mean the maintainer has not run
# `pip install -r requirements.txt` in the active venv.
try:
    import vcr  # type: ignore[import-untyped]
    from google import genai  # type: ignore[import-untyped]
    from google.genai import types as genai_types  # type: ignore[import-untyped]
except ImportError as exc:  # pragma: no cover - environment guard
    sys.stderr.write(
        f"identity_probe: required dependency missing ({exc.name}).\n"
        "Install fixture-local dependencies first:\n"
        "  python3 -m venv .venv && source .venv/bin/activate && "
        "pip install -r requirements.txt\n"
    )
    sys.exit(2)

# Pinned model per #1305 candidate evaluation. Do not change without a new
# evaluation PR; the level-3 stable-identity guarantee is scoped to the
# Gemini 3 family for this exact identifier.
MODEL_PIN = "gemini-3.5-flash"

# Cassette path is relative to the script directory so the probe is
# self-contained.
SCRIPT_DIR = Path(__file__).resolve().parent
CASSETTE_PATH = SCRIPT_DIR / "cassettes" / "identity-probe.yaml"
RESULTS_DIR = SCRIPT_DIR / "probe-results"

# Auth fields to strip from every recorded request before commit. Mirrors
# the cassette redaction contract in #1307.
HEADER_FILTERS = [
    ("x-goog-api-key", "REDACTED"),
    ("Authorization", "REDACTED"),
    ("X-Goog-User-Project", "REDACTED"),
]
QUERY_FILTERS = [
    ("key", "REDACTED"),
    ("access_token", "REDACTED"),
]


def vcr_config() -> vcr.VCR:
    """Build a VCR configuration that strips Gemini auth credentials.

    Critical property: this MUST NOT touch response bodies, because the
    response body carries FunctionCall.id, which is the identity seam the
    level-3 evaluation rests on. Only request-side auth is filtered.
    """
    return vcr.VCR(
        filter_headers=HEADER_FILTERS,
        filter_query_parameters=QUERY_FILTERS,
        decode_compressed_response=True,
        record_mode="none",  # default for replay; overridden in record mode
    )


def read_api_key_from_env() -> str:
    """Read the Gemini API key from the environment for record mode only.

    Per the cassette redaction contract, the key MUST NOT be written to
    disk, environment files, or commit history. This function reads from
    env and returns to the caller; no caching, no logging.
    """
    for var in ("GEMINI_API_KEY", "GOOGLE_API_KEY"):
        value = os.environ.get(var)
        if value:
            return value
    sys.stderr.write(
        "identity_probe: GEMINI_API_KEY (or GOOGLE_API_KEY) must be set "
        "for record mode.\n"
        "This is the only point where a live key touches the probe. The "
        "recording session does not write the key to disk or to the cassette.\n"
    )
    sys.exit(64)


def build_client(api_key: str | None) -> Any:
    """Construct a google-genai client.

    In replay mode the api_key may be a placeholder; the cassette's recorded
    response is replayed by VCR.py without any real network call. In record
    mode the live key is required.
    """
    return genai.Client(api_key=api_key or "replay-no-key")


def build_function_tool() -> Any:
    """Define the read_file function declaration used by the probe.

    Same shape as the eventual fixture call. One parameter (path), strict
    schema, no fancy types. The probe does not actually execute the
    function; it only verifies that Gemini returns a FunctionCall part
    naming this function.
    """
    return genai_types.Tool(
        function_declarations=[
            genai_types.FunctionDeclaration(
                name="read_file",
                description="Read the deterministic runner-spike fixture file.",
                parameters=genai_types.Schema(
                    type="OBJECT",
                    properties={
                        "path": genai_types.Schema(type="STRING"),
                    },
                    required=["path"],
                ),
            )
        ]
    )


def call_model(client: Any) -> Any:
    """Run one non-streaming generate_content() call.

    Returns the raw response object. The caller extracts FunctionCall.id
    for the identity assertions.
    """
    return client.models.generate_content(
        model=MODEL_PIN,
        contents="Read the deterministic fixture file at /tmp/probe-input.txt.",
        config=genai_types.GenerateContentConfig(
            tools=[build_function_tool()],
            tool_config=genai_types.ToolConfig(
                function_calling_config=genai_types.FunctionCallingConfig(
                    mode="ANY",
                    allowed_function_names=["read_file"],
                )
            ),
        ),
    )


def extract_function_call(response: Any) -> dict[str, Any]:
    """Pull the first FunctionCall from a response.

    Asserts FunctionCall.id is present and non-empty. Returns a small dict
    suitable for JSON comparison between record and replay.
    """
    candidates = getattr(response, "candidates", None) or []
    if not candidates:
        raise AssertionError("response.candidates is empty")
    parts = getattr(candidates[0].content, "parts", None) or []
    function_calls = [
        getattr(part, "function_call", None) for part in parts
        if getattr(part, "function_call", None) is not None
    ]
    if not function_calls:
        raise AssertionError("no function_call part found in response")
    if len(function_calls) > 1:
        raise AssertionError(
            f"expected exactly one function_call, got {len(function_calls)}"
        )
    fc = function_calls[0]
    fc_id = getattr(fc, "id", None)
    if not fc_id:
        raise AssertionError(
            "FunctionCall.id is missing or empty; this contradicts the level-3 "
            "stable-identity assumption used in #1305. Per #1307 kill criterion "
            "1 or 2, stop the implementation line."
        )
    return {
        "id": fc_id,
        "name": getattr(fc, "name", None),
        "args_keys": sorted((getattr(fc, "args", None) or {}).keys()),
    }


@dataclass
class ProbeOutcome:
    """Structured probe result, JSON-serialized to probe-results/."""

    mode: str
    timestamp_utc: str
    model_pin: str
    function_call: dict[str, Any] | None
    error: str | None
    passed: bool

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema": "assay.runner.gemini_identity_probe.v0",
            "mode": self.mode,
            "timestamp_utc": self.timestamp_utc,
            "model_pin": self.model_pin,
            "function_call": self.function_call,
            "error": self.error,
            "passed": self.passed,
        }


def run_record(api_key: str) -> ProbeOutcome:
    CASSETTE_PATH.parent.mkdir(parents=True, exist_ok=True)
    CASSETTE_PATH.unlink(missing_ok=True)
    cfg = vcr_config()
    cfg.record_mode = "all"  # record a fresh single-interaction cassette
    timestamp = datetime.now(timezone.utc).isoformat()
    with cfg.use_cassette(str(CASSETTE_PATH)):
        client = build_client(api_key)
        try:
            response = call_model(client)
            function_call = extract_function_call(response)
        except Exception as exc:  # pragma: no cover - probe failure path
            return ProbeOutcome(
                mode="record",
                timestamp_utc=timestamp,
                model_pin=MODEL_PIN,
                function_call=None,
                error=str(exc),
                passed=False,
            )
    return ProbeOutcome(
        mode="record",
        timestamp_utc=timestamp,
        model_pin=MODEL_PIN,
        function_call=function_call,
        error=None,
        passed=True,
    )


def run_replay() -> ProbeOutcome:
    if not CASSETTE_PATH.exists():
        return ProbeOutcome(
            mode="replay",
            timestamp_utc=datetime.now(timezone.utc).isoformat(),
            model_pin=MODEL_PIN,
            function_call=None,
            error=f"cassette not found at {CASSETTE_PATH}",
            passed=False,
        )
    cfg = vcr_config()
    cfg.record_mode = "none"  # strict replay; any network call is a failure
    timestamp = datetime.now(timezone.utc).isoformat()
    with cfg.use_cassette(str(CASSETTE_PATH)):
        client = build_client(api_key=None)
        try:
            response = call_model(client)
            function_call = extract_function_call(response)
        except Exception as exc:  # pragma: no cover - probe failure path
            return ProbeOutcome(
                mode="replay",
                timestamp_utc=timestamp,
                model_pin=MODEL_PIN,
                function_call=None,
                error=str(exc),
                passed=False,
            )
    return ProbeOutcome(
        mode="replay",
        timestamp_utc=timestamp,
        model_pin=MODEL_PIN,
        function_call=function_call,
        error=None,
        passed=True,
    )


def compare_outcomes(record: ProbeOutcome, replay: ProbeOutcome) -> ProbeOutcome:
    """Combine a record + replay outcome into one comparison result."""
    timestamp = datetime.now(timezone.utc).isoformat()
    if not record.passed or not replay.passed:
        return ProbeOutcome(
            mode="record-and-replay",
            timestamp_utc=timestamp,
            model_pin=MODEL_PIN,
            function_call=None,
            error=(
                f"record passed={record.passed}, replay passed={replay.passed}; "
                f"record error={record.error}; replay error={replay.error}"
            ),
            passed=False,
        )
    record_fc = record.function_call or {}
    replay_fc = replay.function_call or {}
    if record_fc.get("id") != replay_fc.get("id"):
        return ProbeOutcome(
            mode="record-and-replay",
            timestamp_utc=timestamp,
            model_pin=MODEL_PIN,
            function_call=None,
            error=(
                f"FunctionCall.id mismatch between record and replay: "
                f"record={record_fc.get('id')!r}, replay={replay_fc.get('id')!r}. "
                "Per #1307 kill criterion 3, stop the implementation line."
            ),
            passed=False,
        )
    return ProbeOutcome(
        mode="record-and-replay",
        timestamp_utc=timestamp,
        model_pin=MODEL_PIN,
        function_call=record_fc,
        error=None,
        passed=True,
    )


def write_result(outcome: ProbeOutcome) -> Path:
    RESULTS_DIR.mkdir(parents=True, exist_ok=True)
    safe_ts = outcome.timestamp_utc.replace(":", "-").replace(".", "-")
    out_path = RESULTS_DIR / f"identity-probe-{safe_ts}.json"
    out_path.write_text(json.dumps(outcome.to_dict(), indent=2, sort_keys=True) + "\n")
    return out_path


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--record", action="store_true", help="record-only")
    mode.add_argument("--replay", action="store_true", help="replay-only")
    mode.add_argument(
        "--record-and-replay",
        action="store_true",
        help="record then replay and compare",
    )
    args = parser.parse_args()

    if args.record:
        outcome = run_record(read_api_key_from_env())
    elif args.replay:
        outcome = run_replay()
    else:
        record = run_record(read_api_key_from_env())
        replay = run_replay() if record.passed else ProbeOutcome(
            mode="replay",
            timestamp_utc=datetime.now(timezone.utc).isoformat(),
            model_pin=MODEL_PIN,
            function_call=None,
            error="skipped because record failed",
            passed=False,
        )
        outcome = compare_outcomes(record, replay)

    out_path = write_result(outcome)
    print(f"probe result -> {out_path}")
    print(json.dumps(outcome.to_dict(), indent=2, sort_keys=True))
    return 0 if outcome.passed else 1


if __name__ == "__main__":
    sys.exit(main())
