#!/usr/bin/env python3
"""Gemini Python google-genai second-runtime fixture.

Mirrors the S5 OpenAI Agents fixture (`fixture-agent.js`) structurally:
- emits three normalized SDK events to ASSAY_RUNNER_SDK_EVENT_LOG
  (tool_call_started, tool_call_completed, run_finished)
- uses ASSAY_RUNNER_SDK_TOOL_CALL_ID as the stable tool-call id for the SDK
  layer
- does not perform live network calls; replay-only against a checked-in
  cassette under cassettes/fixture.yaml

The critical isolation point from S5 is the source of `tool_call_id`:
S5 uses a hardcoded `tc_runner_policy_001` (the DeterministicToolCallModel
chooses it). The Gemini fixture takes the id straight from the recorded
Gemini API response (FunctionCall.id, set by Gemini 3 APIs per
ai.google.dev/gemini-api/docs/function-calling). The fixture must NOT
synthesize an id if the cassette response is missing one; per #1307 kill
criteria 1-3 it must fail loudly.

The caller (shell wrapper) is responsible for:
- pre-setting ASSAY_RUNNER_SDK_TOOL_CALL_ID to the cassette's FunctionCall.id
  before invoking this fixture and the policy wrapper
- ensuring no live API key is in the environment during fixture execution
  (replay mode is enforced here regardless)
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path
from typing import Any

try:
    import vcr  # type: ignore[import-untyped]
    from google import genai  # type: ignore[import-untyped]
    from google.genai import types as genai_types  # type: ignore[import-untyped]
except ImportError as exc:  # pragma: no cover - environment guard
    sys.stderr.write(
        f"fixture: required dependency missing ({exc.name}).\n"
        "Install fixture-local dependencies first in an active venv:\n"
        "  pip install -r tests/fixtures/runner-spike/gemini-google-genai/requirements.txt\n"
    )
    sys.exit(2)

MODEL_PIN = "gemini-3.5-flash"
SDK_PACKAGE_NAME = "google-genai"

SCRIPT_DIR = Path(__file__).resolve().parent
CASSETTE_PATH = SCRIPT_DIR / "cassettes" / "fixture.yaml"

HEADER_FILTERS = [
    ("x-goog-api-key", "REDACTED"),
    ("Authorization", "REDACTED"),
    ("X-Goog-User-Project", "REDACTED"),
]
QUERY_FILTERS = [
    ("key", "REDACTED"),
    ("access_token", "REDACTED"),
]


def required_env(name: str) -> str:
    value = os.environ.get(name)
    if not value:
        sys.stderr.write(f"fixture: required environment variable {name} is unset\n")
        sys.exit(64)
    return value


def load_sdk_metadata() -> tuple[str, str]:
    """Return (package_name, version) loaded from installed package metadata.

    Mirrors the S5 fixture's loadSdkMetadata pattern: read from the installed
    package metadata, not a hardcoded constant, so version bumps surface as
    SDK event content changes rather than silent drift.
    """
    try:
        from importlib.metadata import version  # type: ignore[import-not-found]
    except ImportError:  # pragma: no cover - python <3.8 unsupported
        from importlib_metadata import version  # type: ignore[import-not-found]
    return SDK_PACKAGE_NAME, version(SDK_PACKAGE_NAME)


def vcr_config() -> vcr.VCR:
    """VCR.py configuration. Strict replay; no body scrubbing.

    Replay mode is `none` — any attempted network call raises and fails the
    fixture loudly. This is the cassette redaction contract from #1307:
    delegated acceptance MUST NOT issue a live call.
    """
    return vcr.VCR(
        filter_headers=HEADER_FILTERS,
        filter_query_parameters=QUERY_FILTERS,
        decode_compressed_response=True,
        record_mode="none",
    )


def read_api_key_from_env() -> str:
    """Read the Gemini API key for maintainer-only cassette recording."""
    for var in ("GEMINI_API_KEY", "GOOGLE_API_KEY"):
        value = os.environ.get(var)
        if value:
            return value
    sys.stderr.write(
        "fixture: GEMINI_API_KEY (or GOOGLE_API_KEY) must be set for "
        "maintainer record mode. Delegated replay mode must not set either.\n"
    )
    sys.exit(64)


def build_function_tool() -> Any:
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


def call_model(client: Any, fixture_path: str) -> Any:
    return client.models.generate_content(
        model=MODEL_PIN,
        contents=f"Read the deterministic fixture file at {fixture_path}.",
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


def extract_function_call(response: Any) -> tuple[str, str]:
    """Return (function_call.id, function_call.name) or fail loudly.

    Per #1307 kill criteria 1-3, a missing id is a stop-the-line event. The
    fixture MUST NOT synthesize a tool_call_id.
    """
    candidates = getattr(response, "candidates", None) or []
    if not candidates:
        sys.stderr.write("fixture: response.candidates is empty in cassette replay\n")
        sys.exit(70)
    parts = getattr(candidates[0].content, "parts", None) or []
    function_calls = [
        getattr(part, "function_call", None)
        for part in parts
        if getattr(part, "function_call", None) is not None
    ]
    if len(function_calls) != 1:
        sys.stderr.write(
            f"fixture: expected exactly one function_call in cassette response, "
            f"got {len(function_calls)}. "
            "This contradicts the v0 single-binding fixture contract.\n"
        )
        sys.exit(70)
    fc = function_calls[0]
    fc_id = getattr(fc, "id", None)
    fc_name = getattr(fc, "name", None)
    if not fc_id:
        sys.stderr.write(
            "fixture: FunctionCall.id is missing or empty in cassette response. "
            "Per #1307 kill criterion 1, the implementation line must stop. "
            "Do not synthesize a tool_call_id.\n"
        )
        sys.exit(70)
    if not fc_name:
        sys.stderr.write("fixture: FunctionCall.name is missing in cassette response\n")
        sys.exit(70)
    return fc_id, fc_name


def record_fixture_cassette(work_dir: Path) -> int:
    """Maintainer-only canonical fixture cassette recording.

    This records exactly the request shape replayed by the delegated fixture:
    same model pin, same non-streaming generate_content() call, same
    work-dir-relative prompt. It does not emit SDK events or run the policy
    layer; those are delegated replay concerns.
    """
    work_dir.mkdir(parents=True, exist_ok=True)
    fixture_path = work_dir / "gemini-input.txt"
    if not fixture_path.exists():
        fixture_path.write_text("gemini google-genai fixture input\n", encoding="utf-8")

    CASSETTE_PATH.parent.mkdir(parents=True, exist_ok=True)
    CASSETTE_PATH.unlink(missing_ok=True)

    cfg = vcr_config()
    cfg.record_mode = "all"
    with cfg.use_cassette(str(CASSETTE_PATH)):
        client = genai.Client(api_key=read_api_key_from_env())
        response = call_model(client, str(fixture_path))

    tool_call_id, tool_name = extract_function_call(response)
    result = {
        "schema": "assay.runner.gemini_fixture_record.v0",
        "cassette": str(CASSETTE_PATH),
        "model_pin": MODEL_PIN,
        "function_call": {
            "id": tool_call_id,
            "name": tool_name,
        },
    }
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


def emit_event(log_path: Path, run_id: str, schema: str, seq: int, payload: dict[str, Any]) -> None:
    """Append a normalized SDK event matching assay.runner.sdk_event.v0."""
    sdk_name, sdk_version = load_sdk_metadata()
    event = {
        "schema": schema,
        "run_id": run_id,
        "seq": seq,
        "source": "gemini-google-genai-fixture",
        "sdk_name": sdk_name,
        "sdk_version": sdk_version,
        **payload,
    }
    with log_path.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(event, sort_keys=True, separators=(",", ":")) + "\n")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--record",
        action="store_true",
        help="maintainer-only: record the canonical fixture cassette",
    )
    parser.add_argument("work_dir", help="fixture work directory")
    args = parser.parse_args()

    work_dir = Path(args.work_dir)

    if args.record:
        return record_fixture_cassette(work_dir)

    work_dir.mkdir(parents=True, exist_ok=True)

    fixture_path = work_dir / "gemini-input.txt"
    if not fixture_path.exists():
        fixture_path.write_text("gemini google-genai fixture input\n", encoding="utf-8")

    log_path = Path(required_env("ASSAY_RUNNER_SDK_EVENT_LOG"))
    run_id = required_env("ASSAY_RUNNER_RUN_ID")
    schema = required_env("ASSAY_RUNNER_SDK_EVENT_SCHEMA")
    expected_tool_call_id = required_env("ASSAY_RUNNER_SDK_TOOL_CALL_ID")

    # Reset the SDK event log so determinism wrappers see byte-identical
    # content across runs.
    log_path.write_text("", encoding="utf-8")

    if not CASSETTE_PATH.exists():
        sys.stderr.write(
            f"fixture: cassette not found at {CASSETTE_PATH}. "
            "Run the maintainer recording step described in "
            "MAINTAINER-PROBE.md to produce cassettes/fixture.yaml.\n"
        )
        return 69

    cfg = vcr_config()
    with cfg.use_cassette(str(CASSETTE_PATH)):
        # api_key is a non-empty placeholder because google-genai's Client
        # rejects empty strings; the live key path is never reached because
        # VCR.py intercepts the HTTPS call before any network I/O.
        client = genai.Client(api_key="replay-no-network")
        response = call_model(client, str(fixture_path))

    tool_call_id, tool_name = extract_function_call(response)

    if tool_call_id != expected_tool_call_id:
        sys.stderr.write(
            f"fixture: cassette tool_call_id {tool_call_id!r} does not equal "
            f"ASSAY_RUNNER_SDK_TOOL_CALL_ID {expected_tool_call_id!r}. "
            "The shell wrapper must extract the cassette id BEFORE invoking "
            "the fixture so SDK and policy bind to the same value.\n"
        )
        return 70

    if tool_name != "read_file":
        sys.stderr.write(
            f"fixture: cassette function_call.name {tool_name!r}, expected 'read_file'\n"
        )
        return 70

    # Execute the tool body. This is the agent-equivalent of S5's
    # readFile.execute callback: the fixture reads the deterministic input
    # file under work_dir so the kernel layer captures the openat() call.
    if not fixture_path.exists():
        sys.stderr.write(f"fixture: read_file target not found: {fixture_path}\n")
        return 70
    _ = fixture_path.read_text(encoding="utf-8")

    emit_event(
        log_path,
        run_id,
        schema,
        seq=0,
        payload={
            "event_type": "tool_call_started",
            "tool_call_id": tool_call_id,
            "tool": tool_name,
        },
    )
    emit_event(
        log_path,
        run_id,
        schema,
        seq=1,
        payload={
            "event_type": "tool_call_completed",
            "tool_call_id": tool_call_id,
            "tool": tool_name,
        },
    )
    emit_event(
        log_path,
        run_id,
        schema,
        seq=2,
        payload={
            "event_type": "run_finished",
        },
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
