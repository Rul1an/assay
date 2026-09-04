"""Crate-level internal dependency version declarations, read from parsed TOML.

Every crate in this workspace sets `version.workspace = true`, so each is published at the
workspace version. A sibling declared inside a crate manifest as
`assay-x = { path = "../assay-x", version = "..." }` never reaches the root
`[workspace.dependencies]` table, so enumerating that table alone reports a clean sweep over
declarations it has never looked at. A stale one still resolves under caret semantics -- ^6.0.0
is satisfied by 6.1.0 -- so it costs a published requirement looser than the truth rather than a
build error, which is why it can drift for several release lines unnoticed.

WHY THIS PARSES TOML RATHER THAN MATCHING LINES

The first version of this check matched a line pattern and cross-checked it against a second line
pattern. Two patterns that share a substring are one measurement with extra steps: both required a
double quote and a literal `../`, so `path = '../assay-x'` -- valid TOML that cargo accepts --
escaped both, they agreed at zero, and a stale version went unreported while the count silently
shrank. The same shape blindness cut the other way and reported a commented-out declaration, or a
`[[bench]]` whose target path happens to start with `../`, as a real finding.

A parse has no shape blindness. Quoting, indentation, multi-line inline tables,
`[dependencies.assay-x]` sub-tables, comments and CRLF are the parser's problem, not this file's.

WHAT IS IN SCOPE

The workspace member list in the root manifest is authoritative, so `fuzz` is out of scope because
`[workspace] exclude` says so, not because a pathspec here happened to miss it. Only path
dependencies that RESOLVE to a workspace member are checked; a vendored fork or any other relative
path is not this check's business and is ignored rather than forced to the workspace version.

Cargo's own rules decide when a version is required: it strips `[dev-dependencies]` from a
published manifest, and a crate with `publish = false` publishes nothing at all, so neither needs a
version on a path dependency. A version that IS declared is checked in every case, because a
declared version that disagrees with the truth is misleading wherever it sits.

Output protocol, tab-separated on stdout:
    count\t<number of in-scope declarations examined>
    fail\t<message>        (zero or more)
Exit 0 when the check ran, 2 when it could not run at all.
"""

from __future__ import annotations

import sys
import tomllib
from pathlib import Path

# Cargo resolves these tables as dependencies of the package. `[patch]` and `[replace]` are
# deliberately absent: they redirect resolution and are not the package's own requirements.
BASE_TABLES = ("dependencies", "dev-dependencies", "build-dependencies")
DEV_TABLES = frozenset({"dev-dependencies"})


def _load(path: Path) -> dict:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def _member_dirs(root: Path, workspace: dict) -> list[Path]:
    """Directories named by `[workspace] members`, minus `[workspace] exclude`.

    Members may be glob patterns; the list in this repo is literal, but a pattern must still
    resolve rather than be silently dropped.
    """
    excluded = {(root / e).resolve() for e in workspace.get("exclude", [])}
    dirs: list[Path] = []
    for entry in workspace.get("members", []):
        matches = sorted(root.glob(entry)) if any(c in entry for c in "*?[") else [root / entry]
        for match in matches:
            resolved = match.resolve()
            if resolved not in excluded and (resolved / "Cargo.toml").is_file():
                dirs.append(resolved)
    return dirs


def _dependency_tables(manifest: dict):
    """Yield (table_name, table) for every dependency table, including per-target ones."""
    for name in BASE_TABLES:
        table = manifest.get(name)
        if isinstance(table, dict):
            yield name, table
    targets = manifest.get("target")
    if isinstance(targets, dict):
        for cfg, spec in targets.items():
            if not isinstance(spec, dict):
                continue
            for name in BASE_TABLES:
                table = spec.get(name)
                if isinstance(table, dict):
                    yield f"target.{cfg}.{name}", table


def check(root: Path) -> tuple[int, list[str]]:
    problems: list[str] = []
    root_manifest = _load(root / "Cargo.toml")

    workspace = root_manifest.get("workspace")
    if not isinstance(workspace, dict):
        raise SystemExit("root Cargo.toml has no [workspace] table")
    workspace_version = workspace.get("package", {}).get("version")
    if not workspace_version:
        raise SystemExit("root Cargo.toml has no [workspace.package] version")

    member_dirs = _member_dirs(root, workspace)
    if not member_dirs:
        raise SystemExit("[workspace] members resolved to nothing; the enumeration is broken")
    members = set(member_dirs)

    checked = 0
    for member_dir in member_dirs:
        manifest_path = member_dir / "Cargo.toml"
        rel = manifest_path.relative_to(root)
        try:
            manifest = _load(manifest_path)
        except tomllib.TOMLDecodeError as error:
            problems.append(f"{rel}: manifest does not parse as TOML: {error}")
            continue

        # `publish = false` publishes nothing, so no requirement is ever emitted from it.
        publishes = manifest.get("package", {}).get("publish", True) is not False

        for table_name, table in _dependency_tables(manifest):
            for name, spec in table.items():
                if not isinstance(spec, dict):
                    continue
                declared_path = spec.get("path")
                if not isinstance(declared_path, str):
                    continue
                target_dir = (member_dir / declared_path).resolve()
                if target_dir not in members:
                    # Points outside the workspace. Not this check's business.
                    continue

                checked += 1
                version = spec.get("version")
                if version is None:
                    if publishes and table_name not in DEV_TABLES:
                        problems.append(
                            f"{rel}: {name} in [{table_name}] is a path dependency on a "
                            f"workspace member with no version"
                        )
                    continue
                if not isinstance(version, str):
                    problems.append(f"{rel}: {name} in [{table_name}] has a non-string version")
                    continue
                # A bare requirement and its caret form mean the same thing to cargo.
                if version.lstrip("^") != workspace_version:
                    problems.append(
                        f'{rel}: {name} in [{table_name}] declares version "{version}", '
                        f'workspace is "{workspace_version}"'
                    )

    return checked, problems


def main() -> int:
    root = Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
    try:
        checked, problems = check(root)
    except (OSError, tomllib.TOMLDecodeError) as error:
        print(f"could not read the workspace: {error}", file=sys.stderr)
        return 2
    print(f"count\t{checked}")
    for problem in problems:
        print(f"fail\t{problem}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
