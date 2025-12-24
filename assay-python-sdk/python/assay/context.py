from contextvars import ContextVar, Token
from typing import Optional

from .writer import TraceWriter

_current_writer: ContextVar[Optional[TraceWriter]] = ContextVar(
    "verdict_writer", default=None
)


def install(writer: TraceWriter) -> Token:
    """
    Install a TraceWriter into the current context.
    Returns a Token that can be used to reset the context.
    """
    return _current_writer.set(writer)


def reset(token: Token) -> None:
    """
    Reset the context to the state before install() was called.
    """
    _current_writer.reset(token)


def current_writer() -> Optional[TraceWriter]:
    """
    Get the currently installed TraceWriter, or None if not set.
    """
    return _current_writer.get()
