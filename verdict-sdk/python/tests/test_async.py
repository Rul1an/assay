from typing import Any
from unittest.mock import AsyncMock, MagicMock

import pytest
from verdict_sdk.async_openai import (record_chat_completions,
                                      record_chat_completions_stream,
                                      record_chat_completions_with_tools)
from verdict_sdk.clock import FrozenClock
from verdict_sdk.writer import TraceWriter

# --- Mock Infrastructure ---


class MockWriter(TraceWriter):
    def __init__(self):
        super().__init__("mock.jsonl")
        self.events = []

    def write_event(self, event: Any):
        self.events.append(event)


@pytest.fixture
def mock_writer():
    return MockWriter()


@pytest.fixture
def mock_clock():
    return FrozenClock(1000)


@pytest.fixture
def mock_client():
    client = AsyncMock()
    # Setup default response structure
    client.chat.completions.create = AsyncMock()
    return client


# --- Tests ---


@pytest.mark.asyncio
async def test_record_chat_completions_async(mock_writer, mock_client, mock_clock):
    # Setup Mock Response
    mock_response = MagicMock()
    mock_response.choices = [
        MagicMock(message=MagicMock(content="Hello Async", tool_calls=None))
    ]
    mock_response.model = "gpt-4-async"
    mock_response.usage.prompt_tokens = 10

    # Configure Awaitable
    mock_client.chat.completions.create.return_value = mock_response

    input_msgs = [{"role": "user", "content": "Hi"}]

    result = await record_chat_completions(
        writer=mock_writer,
        client=mock_client,
        model="gpt-4",
        messages=input_msgs,
        episode_id="async_test_1",
        clock=mock_clock,
    )

    assert result["content"] == "Hello Async"
    assert len(mock_writer.events) == 3  # Start, Step, End

    # Verify Step Event
    step = mock_writer.events[1]
    assert step["type"] == "step"
    assert step["content"] == "Hello Async"
    assert step["meta"]["gen_ai.request.model"] == "gpt-4"


@pytest.mark.asyncio
async def test_record_with_tools_async(mock_writer, mock_client, mock_clock):
    # Round 1: Assistant calls tool
    msg1 = MagicMock()
    msg1.content = "Thinking..."
    fn_mock = MagicMock()
    fn_mock.name = "my_tool"
    fn_mock.arguments = '{"x": 1}'
    msg1.tool_calls = [MagicMock(id="call_1", function=fn_mock)]

    # Round 2: Final Answer
    msg2 = MagicMock()
    msg2.content = "Done"
    msg2.tool_calls = None

    # Setup side_effect for consecutive calls
    resp1 = MagicMock(choices=[MagicMock(message=msg1)], model="gpt-4")
    resp2 = MagicMock(choices=[MagicMock(message=msg2)], model="gpt-4")

    mock_client.chat.completions.create.side_effect = [resp1, resp2]

    # Async Tool Executor
    async def my_tool(args):
        return {"y": args["x"] + 1}

    result = await record_chat_completions_with_tools(
        writer=mock_writer,
        client=mock_client,
        model="gpt-4",
        messages=[{"role": "user", "content": "do tool"}],
        tool_executors={"my_tool": my_tool},
        episode_id="async_tools",
        clock=mock_clock,
    )

    assert result["content"] == "Done"

    # Events: Start(1) + Model(1) + ToolCall(1) + ToolResult(1, type="tool_call") + Model(2) + End(1) = 6 events
    assert len(mock_writer.events) == 6

    # Check Tool Result Event
    res_evt = mock_writer.events[3]
    assert res_evt["type"] == "tool_call"
    assert res_evt["result"] == {"y": 2}  # 1+1


@pytest.mark.asyncio
async def test_streaming_async(mock_writer, mock_client, mock_clock):
    # Setup Async Iterator for Stream
    chunks = [
        {"choices": [{"index": 0, "delta": {"role": "assistant", "content": "He"}}]},
        {"choices": [{"index": 0, "delta": {"content": "llo"}}]},
        {"choices": [{"index": 0, "finish_reason": "stop", "delta": {}}]},
    ]

    async def async_iter():
        for c in chunks:
            yield c

    mock_client.chat.completions.create.return_value = async_iter()

    collected_content = ""
    async with record_chat_completions_stream(
        writer=mock_writer,
        client=mock_client,
        model="gpt-4",
        messages=[],
        episode_id="async_stream",
        clock=mock_clock,
    ) as stream:
        async for chunk in stream:
            delta = chunk["choices"][0]["delta"].get("content")
            if delta:
                collected_content += delta

    assert collected_content == "Hello"

    # Events: Start + Model (aggregated) + End
    assert len(mock_writer.events) == 3
    model_evt = mock_writer.events[1]
    assert model_evt["content"] == "Hello"
    assert model_evt["meta"]["gen_ai.stream"] is True
