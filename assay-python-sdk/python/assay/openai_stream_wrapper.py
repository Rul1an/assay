from __future__ import annotations

from contextlib import contextmanager
from typing import Any, Callable, Dict, Iterator, List, Optional

from .clock import Clock, SystemClock
from .openai_instrumentor import _extract_usage  # reuse
from .openai_streaming import StreamAccumulator
from .recorder import EpisodeRecorder


@contextmanager
def record_chat_completions_stream(
    *,
    writer: Any,
    client: Any,
    model: str,
    messages: List[Dict[str, Any]],
    tools: Optional[List[Dict[str, Any]]] = None,
    temperature: Optional[float] = 0.0,
    episode_id: str,
    test_id: Optional[str] = None,
    prompt: Optional[str] = None,
    meta: Optional[Dict[str, Any]] = None,
    clock: Optional[Clock] = None,
) -> Iterator[Iterator[Any]]:
    if prompt is None:
        prompt = ""
        for m in reversed(messages):
            if m.get("role") == "user":
                prompt = str(m.get("content", "") or "")
                break

    with EpisodeRecorder(
        writer=writer,
        episode_id=episode_id,
        test_id=test_id,
        prompt=prompt,
        meta=meta or {},
        clock=clock or SystemClock(),
    ) as ep:
        acc = StreamAccumulator()

        stream = client.chat.completions.create(
            model=model,
            messages=messages,
            tools=tools,
            temperature=temperature,
            stream=True,
        )

        def _iter() -> Iterator[Any]:
            try:
                for chunk in stream:
                    acc.feed_chunk(chunk)
                    yield chunk
            finally:
                content = acc.aggregated_content()
                sid = ep.model_step(
                    content=content,
                    meta={
                        "gen_ai.request.model": model,
                        "gen_ai.response.model": model,
                        "gen_ai.stream": True,
                        "gen_ai.finish_reason": acc.finish_reason,
                    },
                )

                # emit tool_call events (no results here)
                for tc in acc.tool_calls():
                    ep.tool_call(
                        tool_name=tc["name"],
                        args=tc["args"],
                        step_id=sid,
                        call_index=int(tc["index"]),
                        tool_call_id=str(tc["id"]),
                    )

                ep.end(outcome="pass")

        yield _iter()


def record_chat_completions_stream_with_tools(
    *,
    writer: Any,
    client: Any,
    model: str,
    messages: List[Dict[str, Any]],
    tools: Optional[List[Dict[str, Any]]] = None,
    tool_executors: Optional[Dict[str, Callable[[Dict[str, Any]], Any]]] = None,
    max_tool_rounds: int = 4,
    temperature: Optional[float] = 0.0,
    episode_id: str,
    test_id: Optional[str] = None,
    prompt: Optional[str] = None,
    meta: Optional[Dict[str, Any]] = None,
    clock: Optional[Clock] = None,
) -> Dict[str, Any]:
    """
    Streaming first call, execute tool(s), then do a non-stream follow-up call to get final answer.
    (Keeps complexity low + works across client versions.)
    """
    if prompt is None:
        prompt = ""
        for m in reversed(messages):
            if m.get("role") == "user":
                prompt = str(m.get("content", "") or "")
                break

    tool_executors = tool_executors or {}
    current_messages = [dict(m) for m in messages]

    with EpisodeRecorder(
        writer=writer,
        episode_id=episode_id,
        test_id=test_id,
        prompt=prompt,
        meta=meta or {},
        clock=clock or SystemClock(),
    ) as ep:
        final_content = ""
        tool_calls_summary: List[Dict[str, Any]] = []

        for round_idx in range(max_tool_rounds + 1):
            # 1) stream model response
            acc = StreamAccumulator()
            stream = client.chat.completions.create(
                model=model,
                messages=current_messages,
                tools=tools,
                temperature=temperature,
                stream=True,
            )
            for chunk in stream:
                acc.feed_chunk(chunk)

            content = acc.aggregated_content()
            sid = ep.model_step(
                content=content,
                meta={
                    "gen_ai.request.model": model,
                    "gen_ai.response.model": model,
                    "gen_ai.stream": True,
                    "gen_ai.finish_reason": acc.finish_reason,
                    "round": round_idx,
                },
            )

            tcs = acc.tool_calls()
            if not tcs:
                final_content = content
                break

            # 2) record tool calls + execute

            # 2a) Construct and append assistant message first
            assistant_msg = {
                "role": "assistant",
                "content": content,
                "tool_calls": [
                    {
                        "id": str(tc["id"]),
                        "type": "function",
                        "function": {
                            "name": tc["name"],
                            # Re-serialize args for history fidelity, or use empty string if raw was bad
                            "arguments": (
                                json.dumps(tc["args"])
                                if "_raw" not in tc["args"]
                                else tc["args"]["_raw"]
                            ),
                        },
                    }
                    for tc in tcs
                ],
            }
            current_messages.append(assistant_msg)

            # 2b) Execute tools and append results
            for tc in tcs:
                tc_id = str(tc["id"])
                tool_name = tc["name"]
                args = tc["args"]
                idx = int(tc["index"])

                ep.tool_call(
                    tool_name=tool_name,
                    args=args,
                    step_id=sid,
                    call_index=idx,
                    tool_call_id=tc_id,
                )

                executor = tool_executors.get(tool_name)
                result = None
                error = None
                if executor is None:
                    error = "NO_EXECUTOR"
                else:
                    try:
                        result = executor(args)
                    except Exception as e:
                        error = str(e)

                ep.tool_call_result(
                    step_id=sid,
                    call_index=idx,
                    tool_name=tool_name,
                    args=args,
                    result=result,
                    error=error,
                    tool_call_id=tc_id,
                    meta={"round": round_idx},
                )

                tool_calls_summary.append(
                    {
                        "id": tc_id,
                        "name": tool_name,
                        "args": args,
                        "result": result,
                        "error": error,
                    }
                )

                current_messages.append(
                    {
                        "role": "tool",
                        "tool_call_id": tc_id,
                        "content": "" if result is None else str(result),
                    }
                )

            # 3) follow-up (non-stream) to keep client compat easy
            resp2 = client.chat.completions.create(
                model=model,
                messages=current_messages,
                temperature=temperature,
                stream=False,
            )
            choice = (getattr(resp2, "choices", None) or [])[0]
            msg = choice.message
            final_content = msg.content or ""

            ep.model_step(
                content=final_content,
                meta={
                    "gen_ai.request.model": model,
                    "gen_ai.response.model": getattr(resp2, "model", model),
                    "gen_ai.stream": False,
                    "gen_ai.usage": _extract_usage(resp2),
                    "round": round_idx,
                },
            )
            break

        ep.end(outcome="pass")
        return {
            "episode_id": episode_id,
            "test_id": test_id or episode_id,
            "content": final_content,
            "tool_calls": tool_calls_summary,
        }
