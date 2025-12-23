from verdict_sdk import TraceWriter, EpisodeRecorder

# 1. Initialize Writer
w = TraceWriter("traces/recorded.jsonl")

# 2. Record an Episode
# test_id ensures Verdict CI can link this trace to a test case
with EpisodeRecorder(writer=w, episode_id="weather_simple", test_id="weather_simple", prompt="What's the weather in Tokyo?") as ep:

    # Log a thought step
    sid = ep.step(kind="model", name="agent", content="Checking weather service...")

    # Log a tool call (attached to the thought step)
    ep.tool_call(
        tool_name="GetWeather",
        args={"location": "Tokyo"},
        result={"value": {"temp_c": 22}},
        step_id=sid
    )

    # Ends automatically with "pass" on exit, or explicit:
    ep.end(outcome="pass")

print("Trace written to traces/recorded.jsonl")
