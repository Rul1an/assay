from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Dict, Iterator, List, Optional


@dataclass
class MockDelta:
    content: Optional[str] = None
    tool_calls: Optional[List[Dict[str, Any]]] = None


@dataclass
class MockChoice:
    delta: MockDelta
    finish_reason: Optional[str] = None


@dataclass
class MockChunk:
    choices: List[MockChoice]
    usage: Optional[Dict[str, Any]] = None  # optional final usage


class MockStream(Iterator[MockChunk]):
    def __init__(self, chunks: List[MockChunk]) -> None:
        self._chunks = chunks
        self._i = 0

    def __iter__(self) -> "MockStream":
        return self

    def __next__(self) -> MockChunk:
        if self._i >= len(self._chunks):
            raise StopIteration
        c = self._chunks[self._i]
        self._i += 1
        return c

    def close(self) -> None:
        return


class MockCompletions:
    def create(self, **kwargs: Any):
        # Stream mode only for this mock
        if not kwargs.get("stream", False):
            raise RuntimeError("MockStream only supports stream=True")

        messages = kwargs.get("messages", []) or []
        prompt = ""
        if messages and isinstance(messages[-1], dict):
            prompt = str(messages[-1].get("content", "") or "")

        # Simulate: tool call with split arguments across 2 chunks
        if "weather" in prompt.lower():
            call_id = "call_mock_123"
            chunks: List[MockChunk] = [
                MockChunk(
                    choices=[
                        MockChoice(
                            delta=MockDelta(
                                tool_calls=[
                                    {
                                        "index": 0,
                                        "id": call_id,
                                        "type": "function",
                                        "function": {"name": "GetWeather", "arguments": '{"location": '},
                                    }
                                ]
                            )
                        )
                    ]
                ),
                MockChunk(
                    choices=[
                        MockChoice(
                            delta=MockDelta(
                                tool_calls=[
                                    {
                                        "index": 0,
                                        "id": call_id,
                                        "type": "function",
                                        "function": {"name": "GetWeather", "arguments": '"Tokyo"}'},
                                    }
                                ]
                            )
                        )
                    ]
                ),
                MockChunk(choices=[MockChoice(delta=MockDelta(content=""))], usage={"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}),
                MockChunk(choices=[MockChoice(delta=MockDelta(content=""), finish_reason="tool_calls")]),
            ]
            return MockStream(chunks)

        # Default: content stream
        chunks2: List[MockChunk] = [
            MockChunk(choices=[MockChoice(delta=MockDelta(content="Hello "))]),
            MockChunk(choices=[MockChoice(delta=MockDelta(content="world"))]),
            MockChunk(choices=[MockChoice(delta=MockDelta(content="!"), finish_reason="stop")], usage={"prompt_tokens": 1, "completion_tokens": 3, "total_tokens": 4}),
        ]
        return MockStream(chunks2)


class MockChat:
    def __init__(self) -> None:
        self.completions = MockCompletions()


class MockOpenAIClient:
    def __init__(self) -> None:
        self.chat = MockChat()
