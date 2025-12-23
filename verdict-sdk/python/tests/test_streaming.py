from verdict_sdk.openai_streaming import StreamAccumulator
from verdict_sdk.mocks.openai_stream_mock import MockOpenAIClient


def test_stream_accumulates_tool_args():
    client = MockOpenAIClient()
    stream = client.chat.completions.create(
        model="gpt-4o-mini",
        messages=[{"role": "user", "content": "What's the weather like in Tokyo?"}],
        stream=True,
    )

    acc = StreamAccumulator()
    for chunk in stream:
        acc.feed_chunk(chunk)

    tcs = acc.tool_calls()
    assert len(tcs) == 1
    assert tcs[0]["name"] == "GetWeather"
    assert tcs[0]["args"]["location"] == "Tokyo"
