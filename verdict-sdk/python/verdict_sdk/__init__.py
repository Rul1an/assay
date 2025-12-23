"""
Verdict Python SDK: deterministic trace recording for regression gating.
"""

from .writer import TraceWriter
from .recorder import EpisodeRecorder
from .clock import SystemClock, FrozenClock
from .openai_instrumentor import (
    record_chat_completions,
    record_chat_completions_with_tools,
)
from .redaction import make_redactor
from .openai_stream_wrapper import (
    record_chat_completions_stream,
    record_chat_completions_stream_with_tools,
)

__all__ = [
    "TraceWriter",
    "EpisodeRecorder",
    "SystemClock",
    "FrozenClock",
    "record_chat_completions",
    "record_chat_completions_with_tools",
    "make_redactor",
    "record_chat_completions_stream",
    "record_chat_completions_stream_with_tools",
]
