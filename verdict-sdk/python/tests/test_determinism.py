
from verdict_sdk.writer import TraceWriter

def test_trace_writer_determinism(tmp_path):
    trace_file = tmp_path / "trace.jsonl"
    writer = TraceWriter(trace_file)

    # Intentionally unsorted input keys
    event = {
        "type": "event",
        "z": 1,
        "a": 2,
        "nested": {"y": 3, "x": 4}
    }

    writer.write_event(event)

    content = trace_file.read_text(encoding="utf-8").strip()

    # Expect sorted keys and no spaces after separators
    expected = '{"a":2,"nested":{"x":4,"y":3},"type":"event","z":1}'
    assert content == expected
