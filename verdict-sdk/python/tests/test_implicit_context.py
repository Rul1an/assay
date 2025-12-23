from unittest.mock import AsyncMock, MagicMock

import pytest
from verdict_sdk.async_openai import record_chat_completions as record_async
from verdict_sdk.context import install, reset
from verdict_sdk.openai_instrumentor import \
    record_chat_completions as record_sync
from verdict_sdk.writer import TraceWriter


class MockWriter(TraceWriter):
    def __init__(self):
        super().__init__("mock.jsonl")
        self.events = []

    def write_event(self, event):
        self.events.append(event)


@pytest.fixture
def mock_writer():
    return MockWriter()


def test_sync_implicit_writer(mock_writer):
    client = MagicMock()
    client.chat.completions.create.return_value.choices = [
        MagicMock(message=MagicMock(content="Sync"))
    ]
    client.chat.completions.create.return_value.model = "gpt-4"

    token = install(mock_writer)
    try:
        # Call WITHOUT writer
        result = record_sync(
            client=client,
            model="gpt-4",
            messages=[{"role": "user", "content": "Hi"}],
            episode_id="implicit_sync",
        )
        assert result["content"] == "Sync"
        assert len(mock_writer.events) > 0
        assert mock_writer.events[0]["episode_id"] == "implicit_sync"
    finally:
        reset(token)


@pytest.mark.asyncio
async def test_async_implicit_writer(mock_writer):
    client = AsyncMock()
    mock_resp = MagicMock()
    mock_resp.choices = [MagicMock(message=MagicMock(content="Async"))]
    mock_resp.model = "gpt-4"
    client.chat.completions.create.return_value = mock_resp

    token = install(mock_writer)
    try:
        # Call WITHOUT writer
        result = await record_async(
            client=client,
            model="gpt-4",
            messages=[{"role": "user", "content": "Hi"}],
            episode_id="implicit_async",
        )
        assert result["content"] == "Async"
        assert len(mock_writer.events) > 0
        assert mock_writer.events[0]["episode_id"] == "implicit_async"
    finally:
        reset(token)


def test_missing_context_error():
    client = MagicMock()
    with pytest.raises(ValueError, match="No writer provided"):
        record_sync(client=client, model="gpt-4", messages=[], episode_id="fail")
