from verdict_sdk import make_redactor
from verdict_sdk.openai_streaming import StreamAccumulator

# --- Redaction Edge Cases ---

def test_redaction_deeply_nested():
    redactor = make_redactor(key_denylist=("secret",))
    event = {
        "level1": {
            "level2": [
                {"safe": "value", "secret": "HIDDEN"},
                {"level3": {"secret": "HIDDEN_TOO"}}
            ]
        }
    }
    redacted = redactor(event)
    assert redacted["level1"]["level2"][0]["safe"] == "value"
    assert redacted["level1"]["level2"][0]["secret"] == "[REDACTED]"
    assert redacted["level1"]["level2"][1]["level3"]["secret"] == "[REDACTED]"

def test_redaction_case_insensitive_keys():
    redactor = make_redactor(key_denylist=("Authorization",))
    event = {"authorization": "Bearer 123", "AUTHORIZATION": "Bearer 456"}
    redacted = redactor(event)
    assert redacted["authorization"] == "[REDACTED]"
    assert redacted["AUTHORIZATION"] == "[REDACTED]"

def test_redaction_preserves_non_string_types():
    redactor = make_redactor(patterns=[r"foo"])
    event = {"int": 123, "bool": True, "none": None, "list_int": [1, 2]}
    redacted = redactor(event)
    assert redacted == event

def test_redaction_patterns_in_complex_mixed_types():
    redactor = make_redactor(patterns=[r"sk-\w+"])
    event = {
        "messages": [
            {"role": "user", "content": "Here is my key: sk-12345"},
            {"role": "assistant", "content": "Ok, I see sk-99999"}
        ]
    }
    redacted = redactor(event)
    assert "sk-" not in redacted["messages"][0]["content"]
    assert "[REDACTED]" in redacted["messages"][0]["content"]
    assert "[REDACTED]" in redacted["messages"][1]["content"]

# --- Streaming Edge Cases ---

def test_streaming_fragmented_tool_args():
    # Simulates extremely fragmented JSON (1 char per chunk)
    acc = StreamAccumulator()

    # 1. Start tool call
    chunk1 = {"choices": [{"index": 0, "delta": {"tool_calls": [{"index": 0, "id": "call_1", "function": {"name": "TestTool", "arguments": ""}}]}}]}
    acc.feed_chunk(chunk1)

    # 2. Feed arguments character by character: {"arg": "val"}
    full_json = '{"arg": "val"}'
    for char in full_json:
        c = {"choices": [{"index": 0, "delta": {"tool_calls": [{"index": 0, "function": {"arguments": char}}]}}]}
        acc.feed_chunk(c)

    calls = acc.tool_calls()
    assert len(calls) == 1
    assert calls[0]["name"] == "TestTool"
    assert calls[0]["args"] == {"arg": "val"}

def test_streaming_multiple_tool_calls_interleaved():
    # Two tools being built in parallel (rare but possible in spec)
    acc = StreamAccumulator()

    # Init both
    acc.feed_chunk({"choices": [{"index": 0, "delta": {"tool_calls": [{"index": 0, "id": "c1", "function": {"name": "T1"}}]}}]})
    acc.feed_chunk({"choices": [{"index": 0, "delta": {"tool_calls": [{"index": 1, "id": "c2", "function": {"name": "T2"}}]}}]})

    # Arg for T1
    acc.feed_chunk({"choices": [{"index": 0, "delta": {"tool_calls": [{"index": 0, "function": {"arguments": '{"a":1}'}}]}}]})
    # Arg for T2
    acc.feed_chunk({"choices": [{"index": 0, "delta": {"tool_calls": [{"index": 1, "function": {"arguments": '{"b":2}'}}]}}]})

    calls = acc.tool_calls()
    assert len(calls) == 2

    # Sort check is implicit in implementation but let's be robust
    t1 = next(c for c in calls if c["id"] == "c1")
    t2 = next(c for c in calls if c["id"] == "c2")

    assert t1["name"] == "T1" and t1["args"] == {"a": 1}
    assert t2["name"] == "T2" and t2["args"] == {"b": 2}

def test_streaming_malformed_json_args_recovered_as_raw():
    # If LLM hallucinates bad JSON, we should still capture the raw string
    acc = StreamAccumulator()
    acc.feed_chunk({"choices": [{"index": 0, "delta": {"tool_calls": [{"index": 0, "id": "c1", "function": {"name": "BadJson", "arguments": "{bad"}}]}}]})

    calls = acc.tool_calls()
    assert len(calls) == 1
    assert calls[0]["args"] == {"_raw": "{bad"}

def test_streaming_empty_stream():
    # Should not crash on empty
    acc = StreamAccumulator()
    # No chunks fed
    assert acc.aggregated_content() == ""
    assert acc.tool_calls() == []
