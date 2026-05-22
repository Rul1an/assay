#!/usr/bin/env python3
"""Extract the FunctionCall.id from the Gemini fixture cassette.

Used by the shell wrapper to set ASSAY_RUNNER_SDK_TOOL_CALL_ID before the
Python fixture and policy wrapper run. This ensures SDK and policy bind to
the same value, which is the cassette's recorded FunctionCall.id (the value
Gemini's API generated during the maintainer's recording session).

Prints the id to stdout on success. Exits non-zero with a diagnostic on
stderr if the cassette is missing, malformed, or does not contain a single
FunctionCall.id. Per #1307 kill criteria 1-3, a missing or absent id is a
stop-the-line event; this helper must fail loudly and not synthesize one.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

try:
    import yaml  # type: ignore[import-untyped]
except ImportError as exc:  # pragma: no cover - environment guard
    sys.stderr.write(
        f"extract_cassette_tool_call_id: PyYAML missing ({exc.name}). "
        "Install fixture-local dependencies (PyYAML is a vcrpy transitive "
        "dependency).\n"
    )
    sys.exit(2)

SCRIPT_DIR = Path(__file__).resolve().parent
CASSETTE_PATH = SCRIPT_DIR / "cassettes" / "fixture.yaml"


def main() -> int:
    if not CASSETTE_PATH.exists():
        sys.stderr.write(
            f"extract_cassette_tool_call_id: cassette not found at {CASSETTE_PATH}. "
            "Run the maintainer recording step described in "
            "MAINTAINER-PROBE.md to produce cassettes/fixture.yaml.\n"
        )
        return 69

    try:
        cassette = yaml.safe_load(CASSETTE_PATH.read_text(encoding="utf-8"))
    except yaml.YAMLError as exc:
        sys.stderr.write(
            f"extract_cassette_tool_call_id: cassette is not valid YAML: {exc}\n"
        )
        return 65

    interactions = (cassette or {}).get("interactions") or []
    if not interactions:
        sys.stderr.write(
            "extract_cassette_tool_call_id: cassette has no recorded interactions\n"
        )
        return 65

    ids: list[str] = []
    for idx, interaction in enumerate(interactions):
        response = (interaction or {}).get("response") or {}
        body = response.get("body") or {}
        body_string = body.get("string") if isinstance(body, dict) else None
        if not body_string:
            continue
        try:
            payload = json.loads(body_string)
        except json.JSONDecodeError as exc:
            sys.stderr.write(
                f"extract_cassette_tool_call_id: interaction {idx} body is not "
                f"valid JSON: {exc}\n"
            )
            return 65
        for candidate in payload.get("candidates") or []:
            content = candidate.get("content") or {}
            for part in content.get("parts") or []:
                function_call = part.get("functionCall")
                if not function_call:
                    continue
                fc_id = function_call.get("id")
                if fc_id:
                    ids.append(fc_id)

    if not ids:
        sys.stderr.write(
            "extract_cassette_tool_call_id: no functionCall.id found in cassette. "
            "Per #1307 kill criterion 1, the implementation line must stop. "
            "Do not synthesize a tool_call_id.\n"
        )
        return 70
    if len(ids) > 1:
        sys.stderr.write(
            f"extract_cassette_tool_call_id: cassette contains {len(ids)} "
            "functionCall.id values; v0 fixture contract requires exactly one. "
            "Re-record the cassette so it captures a single function call.\n"
        )
        return 70

    print(ids[0])
    return 0


if __name__ == "__main__":
    sys.exit(main())
