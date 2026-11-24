import pytest
import re
import tempfile
import json
import os
from assay import AssayClient


def _episode():
    return [
        {
            "type": "episode_start",
            "episode_id": "e1",
            "timestamp": 1,
            "input": {"prompt": "calculate 1 + 2"},
        },
        {
            "type": "tool_call",
            "episode_id": "e1",
            "step_id": "s1",
            "timestamp": 2,
            "tool_name": "Calculate",
            "call_index": 0,
            "args": {"operation": "add"},
        },
        {
            "type": "episode_end",
            "episode_id": "e1",
            "timestamp": 3,
            "outcome": "pass",
            "final_output": "3",
        },
    ]


def test_record_trace_success():
    with tempfile.NamedTemporaryFile(suffix=".jsonl", delete=False) as tmp:
        path = tmp.name

    try:
        client = AssayClient(path)
        events = _episode()
        for event in events:
            client.record_trace(event)

        with open(path, "r") as f:
            lines = f.readlines()

        assert [json.loads(line) for line in lines] == events

    finally:
        if os.path.exists(path):
            os.remove(path)


def _tool_call(**changes):
    event = dict(_episode()[1])
    event.update(changes)
    return event


# Each of these was written unchanged before, and trace replay then skipped the line, so a
# policy check over the episode passed without seeing the call.
REFUSED = {
    "float_timestamp": (_tool_call(timestamp=2.5), r"u64"),
    "string_timestamp": (_tool_call(timestamp="2"), r"u64"),
    "unknown_type": (_tool_call(type="tool_kall"), r"tool_kall"),
    "missing_tool_name": (
        {k: v for k, v in _tool_call().items() if k != "tool_name"},
        r"tool_name",
    ),
    "string_call_index": (_tool_call(call_index="0"), r"call_index|u32|integer"),
    "unknown_type_with_tool_field": ({"type": "call_tool", "tool": "ToolA"}, r"call_tool"),
}


@pytest.mark.parametrize("name", sorted(REFUSED))
def test_record_trace_refuses_an_event_replay_cannot_read(name):
    event, detail = REFUSED[name]
    with tempfile.NamedTemporaryFile(suffix=".jsonl", delete=False) as tmp:
        path = tmp.name

    try:
        client = AssayClient(path)
        client.record_trace(_episode()[0])
        with open(path, "rb") as f:
            before = f.read()

        with pytest.raises(ValueError, match=r"record_trace") as raised:
            client.record_trace(event)
        assert re.search(detail, str(raised.value)), str(raised.value)

        with open(path, "rb") as f:
            assert f.read() == before

        # The client stays usable after a refusal.
        client.record_trace(_episode()[1])
        with open(path, "r") as f:
            assert len(f.readlines()) == 2

    finally:
        if os.path.exists(path):
            os.remove(path)


def test_record_trace_still_writes_rows_without_an_event_type(tmp_path):
    # Replay refuses these rows loudly as legacy input that `assay trace migrate` converts, and
    # the Python validate()/Coverage helpers read the {"tool", "args"} form directly.
    rows = [
        {"tool": "ToolA", "args": {"q": "weather"}},
        {"type": "assay.trace", "prompt": "p", "response": "r"},
    ]
    path = tmp_path / "trace.jsonl"
    client = AssayClient(str(path))
    for row in rows:
        client.record_trace(row)
    assert [json.loads(line) for line in path.read_text().splitlines()] == rows


def test_record_trace_no_file():
    # Client initialized without file
    client = AssayClient(None)

    # Should raise RuntimeError (mapped from PyRuntimeError)
    with pytest.raises(RuntimeError, match="trace_file is not configured"):
        client.record_trace({"foo": "bar"})

def test_record_trace_invalid_object():
    with tempfile.NamedTemporaryFile(suffix=".jsonl", delete=False) as tmp:
        path = tmp.name

    try:
        client = AssayClient(path)

        class NotSerializable:
            pass

        # Should raise ValueError (mapped from PyValueError)
        # However, pythonize or serde might raise different errors.
        # Simple non-serializable might be a set (which isn't strictly JSON, but maybe supported?)
        # A circular reference or custom object without dict.

        # Let's try a set, usually not valid JSON unless special handling.
        # Or better, an object.
        with pytest.raises(ValueError):
           client.record_trace(NotSerializable())

    finally:
        if os.path.exists(path):
            os.remove(path)


DOCUMENTED = [
    "assay-python-sdk/README.md",
    "docs/AIcontext/entry-points.md",
    "docs/AIcontext/quick-reference.md",
    "docs/AIcontext/user-flows.md",
]


@pytest.mark.parametrize("doc", DOCUMENTED)
def test_documented_record_trace_events_are_accepted(doc, tmp_path):
    import ast
    from pathlib import Path

    root = Path(__file__).resolve().parents[3]
    text = (root / doc).read_text(encoding="utf-8")
    literals = re.findall(r"client\.record_trace\((\{.*?\})\)\n", text, re.S)
    assert literals, f"{doc}: no record_trace event literal"
    client = AssayClient(str(tmp_path / "trace.jsonl"))
    for literal in literals:
        client.record_trace(ast.literal_eval(literal))
    lines = (tmp_path / "trace.jsonl").read_text(encoding="utf-8").splitlines()
    assert len(lines) == len(literals)
