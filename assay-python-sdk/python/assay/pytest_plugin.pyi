from typing import Any, Generator

from .client import AssayClient


def assay_client(request: Any) -> Generator[AssayClient, None, None]: ...
def pytest_configure(config: Any) -> None: ...
