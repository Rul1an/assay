"""Internal dependency version declarations, at both sites they occur, read from parsed TOML.

Every crate in this workspace sets `version.workspace = true`, so each is published at the
workspace version. That fact is declared twice: in the root `[workspace.dependencies]` table, and
directly inside crate manifests as `assay-x = { path = "../assay-x", version = "..." }`. A stale
declaration at either site still resolves under caret semantics -- ^6.0.0 is satisfied by 6.1.0 --
so it costs a published requirement looser than the truth rather than a build error, which is how
it drifts for whole release lines unnoticed.

WHY ONE RULE, AND WHY IT PARSES

The two sites were previously checked by two different line-matching mechanisms, and both were
blind in the same way: each required a double quote and a literal path prefix, so
`path = 'crates/assay-evidence'` -- valid TOML that cargo accepts -- was invisible to them. The
declaration count silently shrank and a stale version went unreported. The same blindness ran the
other way and reported commented-out declarations and `[[bench]]` target paths as real findings.

So there is now one rule, `_version_problem`, applied at both sites, and it reads parsed TOML.
Quoting, indentation, multi-line inline tables, `[dependencies.x]` sub-tables, comments and CRLF
are the parser's problem, not this file's. What differs between the sites is only the base
directory a path resolves against and whether cargo would require a version there; both are
computed by the caller and passed in, so the decision itself has one implementation.

SCOPE, AND WHO DECIDES IT

`[workspace] members` and `[workspace] exclude` are authoritative, so `fuzz` is out of scope
because the workspace says so rather than because a pattern here happened to miss it. Only path
dependencies that RESOLVE to a member are checked: a vendored fork or any other relative path is
not this check's business and is ignored rather than forced to the workspace version.

Cargo decides when a version is required, not this file. It strips every dev-dependencies table
from a published manifest -- per-target ones included -- and a crate that publishes nothing emits
no requirement at all. A version that IS declared is checked in every case, because a declared
version that disagrees with the truth is misleading wherever it sits.

Output protocol, tab-separated on stdout:
    root_count\t<in-scope declarations in [workspace.dependencies]>
    crate_count\t<in-scope declarations across member manifests>
    fail\t<message>        (zero or more)
Both counts are always emitted. The caller fails closed when either is missing or zero, so a
helper that dies or goes quiet cannot read as a clean sweep.
"""

from __future__ import annotations

import sys
import tomllib
from pathlib import Path

# Cargo resolves these as dependencies of the package, paired with whether it strips the table
# from a published manifest. `[patch]` and `[replace]` are deliberately absent: they redirect
# resolution and are not the package's own requirements.
BASE_TABLES = (("dependencies", False), ("dev-dependencies", True), ("build-dependencies", False))


def _load(path: Path) -> dict:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def _member_dirs(root: Path, workspace: dict) -> list[Path]:
    """Directories named by `[workspace] members`, minus `[workspace] exclude`."""
    excluded = {(root / entry).resolve() for entry in workspace.get("exclude", [])}
    seen: set[Path] = set()
    dirs: list[Path] = []
    for entry in workspace.get("members", []):
        matches = sorted(root.glob(entry)) if any(c in entry for c in "*?[") else [root / entry]
        for match in matches:
            resolved = match.resolve()
            if resolved in excluded or resolved in seen:
                continue
            if (resolved / "Cargo.toml").is_file():
                seen.add(resolved)
                dirs.append(resolved)
    return dirs


def _dependency_tables(manifest: dict):
    """Yield (display name, table, stripped_on_publish) for every dependency table.

    `stripped_on_publish` travels with the table rather than being recovered from its name later:
    a name-based test has to know that `target.cfg(unix).dev-dependencies` ends in a dev table,
    and getting that wrong refuses a manifest cargo accepts.
    """
    for name, is_dev in BASE_TABLES:
        table = manifest.get(name)
        if isinstance(table, dict):
            yield name, table, is_dev
    targets = manifest.get("target")
    if isinstance(targets, dict):
        for cfg, spec in targets.items():
            if not isinstance(spec, dict):
                continue
            for name, is_dev in BASE_TABLES:
                table = spec.get(name)
                if isinstance(table, dict):
                    yield f"target.{cfg}.{name}", table, is_dev


def _internal_target(base_dir: Path, spec: dict, members: set[Path]) -> Path | None:
    """The member directory this spec points at, or None if it is not an internal path dependency."""
    declared = spec.get("path")
    if not isinstance(declared, str):
        return None
    resolved = (base_dir / declared).resolve()
    return resolved if resolved in members else None


def _version_problem(where: str, spec: dict, workspace_version: str, require_version: bool) -> str | None:
    """The one rule. `where` names the declaration; `require_version` is cargo's answer, passed in."""
    version = spec.get("version")
    if version is None:
        if require_version:
            return f"{where} is a path dependency on a workspace member with no version"
        return None
    if not isinstance(version, str):
        return f"{where} has a non-string version"
    # A bare requirement and its caret form mean the same thing to cargo.
    if version.removeprefix("^") != workspace_version:
        return f'{where} declares version "{version}", workspace is "{workspace_version}"'
    return None


def _publishes(manifest: dict) -> bool:
    """Cargo treats `false` and an empty registry list alike: nothing is ever published."""
    publish = manifest.get("package", {}).get("publish", True)
    return not (publish is False or publish == [])


def check(root: Path) -> tuple[int, int, list[str]]:
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

    # Site 1: the root table. Its versions are what members inherit through `workspace = true`,
    # so a version is always required here -- there is no dev table and no publish flag to consult.
    root_checked = 0
    for name, spec in workspace.get("dependencies", {}).items():
        if not isinstance(spec, dict) or _internal_target(root, spec, members) is None:
            continue
        root_checked += 1
        problem = _version_problem(
            f"Cargo.toml: {name} in [workspace.dependencies]", spec, workspace_version, True
        )
        if problem:
            problems.append(problem)

    # Site 2: each member manifest.
    crate_checked = 0
    for member_dir in member_dirs:
        manifest_path = member_dir / "Cargo.toml"
        rel = manifest_path.relative_to(root)
        try:
            manifest = _load(manifest_path)
        except tomllib.TOMLDecodeError as error:
            problems.append(f"{rel}: manifest does not parse as TOML: {error}")
            continue
        publishes = _publishes(manifest)
        for table_name, table, stripped_on_publish in _dependency_tables(manifest):
            for name, spec in table.items():
                if not isinstance(spec, dict) or _internal_target(member_dir, spec, members) is None:
                    continue
                crate_checked += 1
                problem = _version_problem(
                    f"{rel}: {name} in [{table_name}]",
                    spec,
                    workspace_version,
                    publishes and not stripped_on_publish,
                )
                if problem:
                    problems.append(problem)

    return root_checked, crate_checked, problems


def main() -> int:
    root = Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
    try:
        root_checked, crate_checked, problems = check(root)
    except (OSError, tomllib.TOMLDecodeError) as error:
        # Emitted as a fail line as well as a non-zero exit: the counts are absent either way, so
        # the caller fails closed, but the operator should see the cause and not only the guard.
        print(f"fail\tcould not read the workspace: {error}")
        return 2
    print(f"root_count\t{root_checked}")
    print(f"crate_count\t{crate_checked}")
    for problem in problems:
        print(f"fail\t{problem}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
