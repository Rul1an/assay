from pathlib import Path

import assay


def test_typed_package_markers_present():
    package_dir = Path(assay.__file__).resolve().parent

    expected = {
        "__init__.pyi",
        "_native.pyi",
        "client.pyi",
        "coverage.pyi",
        "explain.pyi",
        "pytest_plugin.pyi",
        "py.typed",
    }

    present = {path.name for path in package_dir.iterdir() if path.is_file()}
    missing = expected - present
    assert not missing, f"missing typing artifacts: {sorted(missing)}"


def test_validate_is_exported():
    assert callable(assay.validate)
