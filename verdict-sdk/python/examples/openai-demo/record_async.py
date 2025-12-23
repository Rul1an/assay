import asyncio
import os
import sys
from dataclasses import dataclass, field
from typing import Any, List, Optional

from verdict_sdk import TraceWriter
from verdict_sdk.async_openai import (record_chat_completions,
                                      record_chat_completions_stream,
                                      record_chat_completions_with_tools)

# --- Tool Executors ---
try:
    sys.path.append(os.path.dirname(__file__))
    from tools import GetWeather
except ImportError:

    def GetWeather(args):
        return {"temp": 22}


# --- Mock Infrastructure (Async) ---
@dataclass
class MockUsage:
    prompt_tokens: int = 10
    completion_tokens: int = 20
    total_tokens: int = 30

    def dict(self):
        return {"prompt_tokens": 10, "completion_tokens": 20, "total_tokens": 30}


@dataclass
class MockFunction:
    name: str = "GetWeather"
    arguments: str = "{}"


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


@dataclass
class MockStreamChunk:
    choices: List[MockStreamChoice]
    model: str = "gpt-4o-stream"


class MockCompletions:
    async def create(self, **kwargs):
        stream = kwargs.get("stream", False)
        if stream:
            return self._create_stream(**kwargs)

        # Simulate Network Delay
        await asyncio.sleep(0.01)

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
                choices=[MockChoice(message=MockMessage(content="The weather is 22C."))]
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
                                    id="call_mock_async",
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

    async def _create_stream(self, **kwargs):
        # Async Generator for streaming chunks
        await asyncio.sleep(0.01)
        yield MockStreamChunk(
            choices=[
                MockStreamChoice(
                    delta=MockDelta(role="assistant", content="Async Streaming ")
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


# --- Async Main ---


async def main():
    api_key = os.environ.get("OPENAI_API_KEY", "")
    use_mock = api_key == "mock" or not api_key
    mode = os.environ.get("RECORDER_MODE", "simple")

    if use_mock:
        print("Using Async Mock OpenAI Client")
        client = MockClient()
    else:
        from openai import AsyncOpenAI

        client = AsyncOpenAI(api_key=api_key)

    trace_path = os.environ.get("VERDICT_TRACE", "traces/openai_async.jsonl")
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
        print(f"Recording Async Loop to {trace_path}...")
        result = await record_chat_completions_with_tools(
            writer=writer,
            client=client,
            model="gpt-4o-mini",
            messages=messages,
            tools=tools,
            tool_executors={"GetWeather": GetWeather},
            episode_id="async_loop_demo",
            test_id="async_loop_demo",
            prompt=messages[0]["content"],
        )
        print(f"Done Async Loop. Result: {result['content']}")

    elif mode == "stream":
        print(f"Recording Async Stream to {trace_path}...")
        async with record_chat_completions_stream(
            writer=writer,
            client=client,
            model="gpt-4o-mini",
            messages=messages,
            tools=tools,
            episode_id="async_stream_demo",
            test_id="async_stream_demo",
            prompt=messages[0]["content"],
        ) as stream:
            async for chunk in stream:
                pass
        print("Done Async Stream.")

    else:
        print(f"Recording Async Simple to {trace_path}...")
        await record_chat_completions(
            writer=writer,
            client=client,
            model="gpt-4o-mini",
            messages=messages,
            tools=tools,
            episode_id="async_simple_demo",
            test_id="async_simple_demo",
            prompt=messages[0]["content"],
        )
        print(f"Done Async Simple.")


if __name__ == "__main__":
    asyncio.run(main())
