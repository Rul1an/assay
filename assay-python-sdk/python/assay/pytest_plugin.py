import pytest
from typing import Generator
from .client import AssayClient

@pytest.fixture
def assay_client(request) -> Generator[AssayClient, None, None]:
    """
    Pytest fixture that provides an AssayClient instance.
    Arguments can be passed via marker: @pytest.mark.assay(trace_file="foo.jsonl")
    """
    trace_file = None
    marker = request.node.get_closest_marker("assay")
    if marker:
        trace_file = marker.kwargs.get("trace_file")

    client = AssayClient(trace_file)
    yield client

def pytest_configure(config):
    """Register the assay marker."""
    config.addinivalue_line(
        "markers", "assay(trace_file): mark test to use AssayClient with specific trace file"
    )
