import os
import sys
from dataclasses import dataclass, field
from typing import Any, Iterator, List, Optional

from verdict_sdk import (TraceWriter, record_chat_completions,
                         record_chat_completions_stream,
                         record_chat_completions_with_tools)

# --- Tool Executors ---
try:
    sys.path.append(os.path.dirname(__file__))
    from tools import GetWeather
except ImportError:

    def GetWeather(args):
        return {"temp": 22}


# --- Mock OpenAI Client ---
@dataclass
class MockUsage:
    prompt_tokens: int = 10
    completion_tokens: int = 20
    total_tokens: int = 30

    def dict(self):
        return {"prompt_tokens": 10, "completion_tokens": 20, "total_tokens": 30}


@dataclass
class MockFunction:
    name: str
    arguments: str


@dataclass
class MockToolCall:
    id: str
    function: MockFunction
    type: str = "function"


@dataclass
class MockMessage:
    content: Optional[str]
    tool_calls: Optional[List[MockToolCall]] = None
    role: str = "assistant"

    def dict(self):
        return {
            "role": self.role,
            "content": self.content,
            "tool_calls": [
                {
                    "id": tc.id,
                    "type": "function",
                    "function": {
                        "name": tc.function.name,
                        "arguments": tc.function.arguments,
                    },
                }
                for tc in (self.tool_calls or [])
            ],
        }


@dataclass
class MockChoice:
    message: MockMessage


@dataclass
class MockResponse:
    choices: List[MockChoice]
    model: str = "gpt-4o-mini"
    usage: MockUsage = field(default_factory=MockUsage)


# For Streaming
@dataclass
class MockDelta:
    role: Optional[str] = None
    content: Optional[str] = None
    tool_calls: Optional[List[dict]] = None

    def dict(self):
        d = {}
        if self.role:
            d["role"] = self.role
        if self.content:
            d["content"] = self.content
        if self.tool_calls:
            d["tool_calls"] = self.tool_calls
        return d


@dataclass
class MockStreamChoice:
    index: int = 0
    delta: MockDelta = field(default_factory=MockDelta)
    finish_reason: Optional[str] = None

    def dict(self):
        return {
            "index": self.index,
            "delta": self.delta.dict(),
            "finish_reason": self.finish_reason,
        }


@dataclass
class MockStreamChunk:
    choices: List[MockStreamChoice]
    model: str = "gpt-4o-stream"

    # Minimal attribute access emulation
    def __getattr__(self, name):
        if name == "choices":
            return self.choices
        raise AttributeError(name)


class MockCompletions:
    def create(self, **kwargs):
        stream = kwargs.get("stream", False)
        if stream:
            return self._create_stream(**kwargs)

        # Non-streaming Logic
        msgs = kwargs.get("messages", [])
        if not msgs:
            return MockResponse(
                choices=[
                    MockChoice(message=MockMessage(content="No messages provided"))
                ]
            )
        last_msg = msgs[-1]

        if last_msg.get("role") == "tool":
            return MockResponse(
                choices=[
                    MockChoice(
                        message=MockMessage(content="The weather in Tokyo is 22C.")
                    )
                ]
            )

        prompt = last_msg.get("content", "") or ""
        if "weather" in prompt.lower():
            return MockResponse(
                choices=[
                    MockChoice(
                        message=MockMessage(
                            content="",
                            tool_calls=[
                                MockToolCall(
                                    id="call_mock_123",
                                    function=MockFunction(
                                        name="GetWeather",
                                        arguments='{"location": "Tokyo"}',
                                    ),
                                )
                            ],
                        )
                    )
                ]
            )
        else:
            return MockResponse(
                choices=[MockChoice(message=MockMessage(content="I am a mock AI."))]
            )

    def _create_stream(self, **kwargs):
        # Check prompt for "weather" to trigger tool call
        msgs = kwargs.get("messages", [])
        prompt = (msgs[-1].get("content", "") or "") if msgs else ""

        if "weather" in prompt.lower():
            # Mock Tool Call Stream
            # 1. Start Tool Call
            yield MockStreamChunk(
                choices=[
                    MockStreamChoice(
                        delta=MockDelta(
                            role="assistant",
                            tool_calls=[
                                {
                                    "index": 0,
                                    "id": "call_stream_123",
                                    "type": "function",
                                    "function": {"name": "GetWeather", "arguments": ""},
                                }
                            ],
                        )
                    )
                ]
            )
            # 2. Args
            yield MockStreamChunk(
                choices=[
                    MockStreamChoice(
                        delta=MockDelta(
                            tool_calls=[
                                {
                                    "index": 0,
                                    "function": {"arguments": '{"location": "Tokyo"}'},
                                }
                            ]
                        )
                    )
                ]
            )
            # 3. Finish
            yield MockStreamChunk(
                choices=[
                    MockStreamChoice(finish_reason="tool_calls", delta=MockDelta())
                ]
            )
        else:
            # Generator for streaming chunks
            yield MockStreamChunk(
                choices=[
                    MockStreamChoice(
                        delta=MockDelta(role="assistant", content="Streaming ")
                    )
                ]
            )
            yield MockStreamChunk(
                choices=[MockStreamChoice(delta=MockDelta(content="mock "))]
            )
            yield MockStreamChunk(
                choices=[MockStreamChoice(delta=MockDelta(content="data."))]
            )
            yield MockStreamChunk(
                choices=[MockStreamChoice(finish_reason="stop", delta=MockDelta())]
            )


class MockChat:
    completions = MockCompletions()


class MockClient:
    chat = MockChat()


# --- Main Example Flow ---


def main():
    api_key = os.environ.get("OPENAI_API_KEY", "")
    use_mock = api_key == "mock" or not api_key
    mode = os.environ.get("RECORDER_MODE", "simple")  # simple | loop | stream

    if use_mock:
        print("Using Mock OpenAI Client")
        client = MockClient()
    else:
        import openai

        client = openai.OpenAI(api_key=api_key)

    trace_path = os.environ.get("VERDICT_TRACE", "traces/openai.jsonl")
    writer = TraceWriter(trace_path)

    messages = [{"role": "user", "content": "What's the weather like in Tokyo?"}]
    tools = [
        {
            "type": "function",
            "function": {
                "name": "GetWeather",
                "parameters": {
                    "type": "object",
                    "properties": {"location": {"type": "string"}},
                },
            },
        }
    ]

    if mode == "loop":
        print(f"Recording Loop to {trace_path}...")
        result = record_chat_completions_with_tools(
            writer=writer,
            client=client,
            model="gpt-4o-mini",
            messages=messages,
            tools=tools,
            tool_executors={"GetWeather": GetWeather},
            episode_id="openai_loop_demo",
            test_id="openai_loop_demo",
            prompt=messages[0]["content"],
        )
        print(f"Done Loop. Tool Results: {result['tool_calls']}")
    elif mode == "stream":
        print(f"Recording Stream to {trace_path}...")
        with record_chat_completions_stream(
            writer=writer,
            client=client,
            model="gpt-4o-mini",
            messages=messages,
            tools=tools,  # Stream doesn't execute tools by default unless using specialized wrapper
            episode_id="openai_stream_demo",
            test_id="openai_stream_demo",
            prompt=messages[0]["content"],
        ) as stream:
            for chunk in stream:
                # Just consume
                pass
        print("Done Stream.")
    else:
        print(f"Recording Simple to {trace_path}...")
        record_chat_completions(
            writer=writer,
            client=client,
            model="gpt-4o-mini",
            messages=messages,
            tools=tools,
            episode_id="openai_weather_demo",
            test_id="openai_weather_demo",
            prompt=messages[0]["content"],
        )
        print(f"Done Simple.")


if __name__ == "__main__":
    main()
