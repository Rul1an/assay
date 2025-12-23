from __future__ import annotations

import json
from typing import Any, Dict, List, Optional, Tuple


def _get(obj: Any, key: str, default=None):
    if isinstance(obj, dict):
        return obj.get(key, default)
    return getattr(obj, key, default)


def _as_dict(obj: Any) -> Any:
    if isinstance(obj, dict):
        return obj
    if hasattr(obj, "dict"):
        return obj.dict()
    if hasattr(obj, "model_dump"):
        return obj.model_dump()
    return obj


def _choice0(chunk: Any) -> Optional[Any]:
    choices = _get(chunk, "choices", None) or []
    return choices[0] if choices else None


def _delta(choice: Any) -> Any:
    # OpenAI streaming uses choice.delta
    return _get(choice, "delta", {}) or {}


class StreamAccumulator:
    """
    Collects streaming chunks into:
      - aggregated assistant content
      - stitched tool calls (id/name/arguments)
    """

    def __init__(self) -> None:
        self._content_parts: List[str] = []
        # key: (index, id) -> {"name": str, "args_parts": [str]}
        self._tool_buf: Dict[Tuple[int, str], Dict[str, Any]] = {}
        # Optimization: Map index to known ID to avoid O(n) iteration
        self._index_to_id: Dict[int, str] = {}
        self.finish_reason: Optional[str] = None

    def feed_chunk(self, chunk: Any) -> None:
        choice = _choice0(chunk)
        if choice is None:
            return

        self.finish_reason = _get(choice, "finish_reason", self.finish_reason)

        d = _delta(choice)
        d = _as_dict(d)

        # content delta
        c = _get(d, "content", None)
        if c:
            self._content_parts.append(str(c))

        # tool_calls delta (OpenAI shape)
        tool_calls = _get(d, "tool_calls", None) or []

        for tc in tool_calls:
            tc = _as_dict(tc)
            idx = int(_get(tc, "index", 0))
            tc_id = _get(tc, "id", None)

            # extract other fields for later use
            fn = _as_dict(_get(tc, "function", {}) or {})
            name = _get(fn, "name", None) or "UNKNOWN_TOOL"
            args_piece = _get(fn, "arguments", None) or ""

            # Correctly identify "call_idx" vs explicit ID
            # In streaming, the ID is often only present in the first chunk for that tool call index.

            if tc_id:
                # Explicit ID provided (usually first chunk)
                self._index_to_id[idx] = str(tc_id)
                key = (idx, str(tc_id))
            else:
                # No ID, look up known ID for this index
                known_id = self._index_to_id.get(idx)
                if known_id:
                    key = (idx, known_id)
                else:
                    # Fallback (implicit/unknown ID)
                    key = (idx, f"call_{idx}")

            buf = self._tool_buf.get(key)
            if buf is None:
                # Handle late-arriving IDs by migrating provisional buffers if needed.
                # OpenAI usually sends IDs in the first chunk, but we must be robust.

                # If we just found the ID but had previous args on 'call_{idx}'...
                if tc_id:
                    provisional_key = (idx, f"call_{idx}")
                    if provisional_key in self._tool_buf:
                        # Migrate provisional buffer to real ID
                        self._tool_buf[key] = self._tool_buf.pop(provisional_key)
                        buf = self._tool_buf[key]
                    else:
                        buf = {"name": name, "args_parts": []}
                        self._tool_buf[key] = buf
                else:
                    buf = {"name": name, "args_parts": []}
                    self._tool_buf[key] = buf

            # name may arrive later in some streams
            if name and buf.get("name") in (None, "", "UNKNOWN_TOOL"):
                buf["name"] = name

            if args_piece:
                buf["args_parts"].append(str(args_piece))

    def aggregated_content(self) -> str:
        return "".join(self._content_parts)

    def tool_calls(self) -> List[Dict[str, Any]]:
        out: List[Dict[str, Any]] = []
        for (idx, tc_id), buf in sorted(self._tool_buf.items(), key=lambda x: (x[0][0], x[0][1])):
            args_str = "".join(buf.get("args_parts", []))
            try:
                args = json.loads(args_str) if args_str else {}
            except Exception:
                args = {"_raw": args_str}

            out.append(
                {
                    "index": idx,
                    "id": tc_id,
                    "name": buf.get("name") or "UNKNOWN_TOOL",
                    "args": args,
                }
            )
        return out
