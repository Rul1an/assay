from __future__ import annotations
import json
from typing import Any, Dict, List, Optional, Union, Callable
from .writer import TraceWriter
from .recorder import EpisodeRecorder
from .clock import Clock, SystemClock

def _extract_usage(response: Any) -> Dict[str, Any]:
    usage_dict = {}
    if getattr(response, "usage", None):
        u = response.usage
        if hasattr(u, "dict"):
            usage_dict = u.dict()
        elif hasattr(u, "model_dump"):
            usage_dict = u.model_dump()
        else:
            usage_dict = dict(u)
    return usage_dict

def _jsonable(x: Any) -> Any:
    """Helper to ensure value is JSON serializable, else returns stringified dict."""
    try:
        json.dumps(x)
        return x
    except (TypeError, ValueError, OverflowError):
        return {"_raw": str(x)}

def record_chat_completions(
    *,
    writer: TraceWriter,
    client: Any,
    model: str,
    messages: List[Dict[str, Any]],
    tools: Optional[List[Dict[str, Any]]] = None,
    tool_choice: Any = None,
    temperature: Optional[float] = 0.0,
    episode_id: str,
    test_id: Optional[str] = None,
    prompt: Optional[str] = None,
    meta: Optional[Dict[str, Any]] = None,
    clock: Optional[Clock] = None,
) -> Dict[str, Any]:
    """
    Executes OpenAI chat.completions call and records Verdict V2 trace events.
    """
    # Prompt determination logic
    if prompt is None:
        for m in reversed(messages):
            if m.get("role") == "user":
                prompt = str(m.get("content", ""))
                break
        if prompt is None and messages:
            prompt = str(messages[-1].get("content", ""))
        if prompt is None:
            prompt = ""

    # Setup Recorder
    rec_meta = meta or {}

    with EpisodeRecorder(
        writer=writer,
        episode_id=episode_id,
        test_id=test_id,
        prompt=prompt,
        meta=rec_meta,
        clock=clock or SystemClock()
    ) as ep:

        kwargs = {
            "model": model,
            "messages": messages,
            "temperature": temperature,
        }
        if tools is not None:
            kwargs["tools"] = tools
        if tool_choice is not None:
            kwargs["tool_choice"] = tool_choice

        response = client.chat.completions.create(**kwargs)

        choices = getattr(response, "choices", None) or []
        if not choices:
            raise RuntimeError("OpenAI returned no choices")
        choice = choices[0]
        message = choice.message

        content = message.content or ""

        # Model Output
        sid = ep.step(
            kind="model",
            name="openai",
            content=content,
            meta={
                "gen_ai.usage": _extract_usage(response),
                "gen_ai.request.model": model,
                "gen_ai.response.model": getattr(response, "model", model)
            }
        )

        # Tool Calls
        tool_calls_out = []
        if message.tool_calls:
            for i, tc in enumerate(message.tool_calls):
                fn = tc.function
                args_val = fn.arguments
                try:
                    args_obj = json.loads(args_val)
                except (json.JSONDecodeError, TypeError):
                    args_obj = {"_raw": str(args_val)}

                # Safe ID extraction (consistent with Loop logic)
                tc_id = getattr(tc, "id", None) or f"{sid}:{i}"

                ep.tool_call(
                    tool_name=fn.name,
                    args=args_obj,
                    result=None,
                    error=None,
                    step_id=sid,
                    call_index=i,
                    tool_call_id=tc_id
                )

                tool_calls_out.append({
                    "name": fn.name,
                    "args": args_obj,
                    "id": tc_id
                })

        # Explicit Pass
        ep.end(outcome="pass")

        return {
            "episode_id": episode_id,
            "test_id": test_id or episode_id,
            "usage": _extract_usage(response),
            "tool_calls": tool_calls_out,
            "content": content
        }

def record_chat_completions_with_tools(
    *,
    writer: TraceWriter,
    client: Any,
    model: str,
    messages: List[Dict[str, Any]],
    tools: Optional[List[Dict[str, Any]]] = None,
    tool_choice: Any = None,
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
    Executes OpenAI chat loop with tool execution.
    """
    if prompt is None:
        for m in reversed(messages):
            if m.get("role") == "user":
                prompt = str(m.get("content", ""))
                break
        if prompt is None and messages:
            prompt = str(messages[-1].get("content", ""))
        if prompt is None:
            prompt = ""

    current_messages = [dict(m) for m in messages]
    tool_executors = tool_executors or {}

    with EpisodeRecorder(
        writer=writer,
        episode_id=episode_id,
        test_id=test_id,
        prompt=prompt,
        meta=meta or {},
        clock=clock or SystemClock()
    ) as ep:

        tool_calls_summary = []
        content = ""

        for round_idx in range(max_tool_rounds + 1):
            kwargs = {
                "model": model,
                "messages": current_messages,
                "temperature": temperature,
            }
            if tools is not None:
                kwargs["tools"] = tools
            if tool_choice is not None:
                kwargs["tool_choice"] = tool_choice

            response = client.chat.completions.create(**kwargs)
            choices = getattr(response, "choices", None) or []
            if not choices:
                break
            message = choices[0].message
            content_chunk = message.content or ""
            content = content_chunk

            sid = ep.model_step(
                content=content_chunk,
                meta={
                    "gen_ai.usage": _extract_usage(response),
                    "gen_ai.request.model": model,
                    "gen_ai.response.model": getattr(response, "model", model),
                    "round": round_idx
                }
            )

            # Normalize Tool Calls
            normalized_tool_calls = []
            if message.tool_calls:
                for i, tc in enumerate(message.tool_calls):
                    tc_id = getattr(tc, "id", None) or f"{sid}:{i}"
                    normalized_tool_calls.append((i, tc_id, tc))

            # Construct Assistant Message
            assistant_msg = {"role": "assistant", "content": content_chunk}
            if normalized_tool_calls:
                assistant_msg["tool_calls"] = [
                    {
                        "id": tc_id,
                        "type": "function",
                        "function": {
                            "name": tc.function.name,
                            "arguments": tc.function.arguments,
                        },
                    }
                    for (i, tc_id, tc) in normalized_tool_calls
                ]
            current_messages.append(assistant_msg)

            # Process Tools
            if normalized_tool_calls:
                for i, tc_id, tc in normalized_tool_calls:
                    fn = tc.function
                    args_val = fn.arguments
                    try:
                        args_obj = json.loads(args_val)
                    except (json.JSONDecodeError, TypeError):
                        args_obj = {"_raw": str(args_val)}

                    result_obj = None
                    error_msg = None
                    executor = tool_executors.get(fn.name)

                    tool_call_res_meta = {}
                    if executor:
                        try:
                            raw_result = executor(args_obj)
                            result_obj = _jsonable(raw_result)
                        except Exception as e:
                            error_msg = str(e)
                            tool_call_res_meta["error_type"] = type(e).__name__
                    else:
                        error_msg = "NO_EXECUTOR"
                        tool_call_res_meta["error_type"] = "ConfigurationError"

                    ep.tool_call_result(
                        step_id=sid,
                        call_index=i,
                        tool_name=fn.name,
                        args=args_obj,
                        result=result_obj,
                        error=error_msg,
                        tool_call_id=tc_id,
                        meta=tool_call_res_meta
                    )

                    tool_calls_summary.append({
                        "name": fn.name,
                        "args": args_obj,
                        "result": result_obj,
                        "error": error_msg
                    })

                    tool_content = json.dumps(result_obj, ensure_ascii=False, sort_keys=True) if result_obj is not None else (error_msg or "")

                    current_messages.append({
                        "role": "tool",
                        "tool_call_id": tc_id,
                        "content": tool_content
                    })

                continue
            else:
                break

        ep.end(outcome="pass")

        return {
            "episode_id": episode_id,
            "test_id": test_id or episode_id,
            "tool_calls": tool_calls_summary,
            "content": content,
            "messages": current_messages
        }
