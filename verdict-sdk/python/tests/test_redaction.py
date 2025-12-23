import json
from pathlib import Path

from verdict_sdk import TraceWriter, make_redactor


def test_redaction_masks_values(tmp_path: Path):
    redactor = make_redactor(
        patterns=[r"sk-[A-Za-z0-9]+"],
        key_denylist=("authorization",),
    )

    p = tmp_path / "t.jsonl"
    w = TraceWriter(p, redact_fn=redactor)

    w.write_event({"type": "episode_start", "meta": {"authorization": "Bearer sk-SECRET"}, "input": {"prompt": "hello sk-ABC"}})

    line = p.read_text(encoding="utf-8").splitlines()[0]
    obj = json.loads(line)

    assert obj["meta"]["authorization"] == "[REDACTED]"
    assert obj["input"]["prompt"] == "hello [REDACTED]"
