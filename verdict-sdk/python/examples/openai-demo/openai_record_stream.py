import os

from verdict_sdk import TraceWriter
from verdict_sdk.mocks.openai_stream_mock import MockOpenAIClient
from verdict_sdk.openai_stream_wrapper import record_chat_completions_stream


def main():
    trace_path = os.environ.get("VERDICT_TRACE", "traces/openai_stream.jsonl")
    writer = TraceWriter(trace_path)

    client = MockOpenAIClient()

    messages = [{"role": "user", "content": "What's the weather like in Tokyo?"}]
    tools = [
        {
            "type": "function",
            "function": {
                "name": "GetWeather",
                "description": "Get current weather",
                "parameters": {
                    "type": "object",
                    "properties": {"location": {"type": "string"}},
                },
            },
        }
    ]

    with record_chat_completions_stream(
        writer=writer,
        client=client,
        model="gpt-4o-mini",
        messages=messages,
        tools=tools,
        episode_id="openai_stream_demo",
        test_id="openai_stream_demo",
        prompt=messages[0]["content"],
    ) as stream:
        for _chunk in stream:
            pass

    print(f"wrote: {trace_path}")


if __name__ == "__main__":
    main()
