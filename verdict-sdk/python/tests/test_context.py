import asyncio

import pytest
from verdict_sdk.context import current_writer, install, reset
from verdict_sdk.writer import TraceWriter


class MockWriter(TraceWriter):
    def __init__(self, name):
        super().__init__("mock.jsonl")
        self.name = name


@pytest.mark.asyncio
async def test_context_propagation():
    w1 = MockWriter("w1")
    token = install(w1)

    assert current_writer().name == "w1"

    async def subtask():
        # Verify propagation to subtask
        assert current_writer().name == "w1"
        return True

    assert await asyncio.create_task(subtask())

    reset(token)
    assert current_writer() is None


@pytest.mark.asyncio
async def test_context_isolation():
    w1 = MockWriter("w1")
    w2 = MockWriter("w2")

    async def task_a():
        token = install(w1)
        await asyncio.sleep(0.01)
        assert current_writer().name == "w1"
        reset(token)

    async def task_b():
        token = install(w2)
        await asyncio.sleep(0.01)
        assert current_writer().name == "w2"
        reset(token)

    await asyncio.gather(task_a(), task_b())
