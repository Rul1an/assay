#!/usr/bin/env python3
"""Public-safe artifact sanitization guard.

This check is intentionally safe for public PR logs:

- it does not carry the private sensitive-vocabulary list;
- it reports only category names and locations;
- it never prints matched text.

When a trusted workflow provides HMAC-SHA256 private-list entries plus the HMAC
key through environment variables, this script can compare normalized token and
n-gram HMACs without printing matched terms, digest values, or key material.
Without that trusted input, it is the required fork-safe structural gate.
"""

from __future__ import annotations

import argparse
import contextlib
import hashlib
import hmac
import io
import os
import re
import subprocess
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, Sequence


REPO_ROOT = Path(__file__).resolve().parents[2]

EXCLUDED_PARTS = {
    ".git",
    ".hg",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    ".tox",
    ".venv",
    "__pycache__",
    "node_modules",
    "target",
    "dist",
    "build",
    "vendor",
}

EXCLUDED_SUFFIXES = {
    ".7z",
    ".a",
    ".bin",
    ".bmp",
    ".bz2",
    ".class",
    ".dll",
    ".dylib",
    ".exe",
    ".gif",
    ".gz",
    ".ico",
    ".jar",
    ".jpeg",
    ".jpg",
    ".lockb",
    ".o",
    ".pdf",
    ".png",
    ".rlib",
    ".so",
    ".tar",
    ".tgz",
    ".wasm",
    ".webp",
    ".zip",
}

TEXT_BYTES_LIMIT = 2_000_000


@dataclass(frozen=True)
class PatternRule:
    category: str
    pattern: re.Pattern[str]


RULES: tuple[PatternRule, ...] = (
    PatternRule(
        "publication_blocker_marker",
        re.compile(
            r"\b("
            r"REDACT[\s_-]*BEFORE[\s_-]*PUBLIC|"
            r"PUBLICATION[\s_-]*BLOCKED|"
            r"PRIVATE[\s_-]*NOTES?[\s_-]*ONLY"
            r")\b",
            re.IGNORECASE,
        ),
    ),
    PatternRule(
        "secret_placeholder_marker",
        re.compile(
            r"\b("
            r"REPLACE[\s_-]*WITH[\s_-]*REAL[\s_-]*(TOKEN|SECRET|KEY)|"
            r"PASTE[\s_-]*(TOKEN|SECRET|KEY)[\s_-]*HERE"
            r")\b",
            re.IGNORECASE,
        ),
    ),
    PatternRule(
        "private_log_marker",
        re.compile(
            r"\b("
            r"RAW[\s_-]*PRIVATE[\s_-]*RUN|"
            r"PRIVATE[\s_-]*RUN[\s_-]*LOG|"
            r"UNREDACTED[\s_-]*(REQUEST|RESPONSE|TRACE)"
            r")\b",
            re.IGNORECASE,
        ),
    ),
)


@dataclass(frozen=True)
class Finding:
    path: Path
    line: int
    category: str


def git_files(root: Path) -> list[Path]:
    proc = subprocess.run(
        ["git", "ls-files", "-z"],
        cwd=root,
        check=True,
        stdout=subprocess.PIPE,
    )
    return [root / item.decode("utf-8") for item in proc.stdout.split(b"\0") if item]


def should_scan(path: Path, root: Path) -> bool:
    rel = path.relative_to(root)
    if any(part in EXCLUDED_PARTS for part in rel.parts):
        return False
    if path.suffix.lower() in EXCLUDED_SUFFIXES:
        return False
    try:
        if path.is_symlink():
            return False
        if path.stat().st_size > TEXT_BYTES_LIMIT:
            return False
    except FileNotFoundError:
        return False
    return True


def read_text(path: Path) -> str | None:
    try:
        data = path.read_bytes()
    except OSError:
        return None
    if b"\0" in data:
        return None
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError:
        return data.decode("utf-8", errors="ignore")


def scan_files(files: Iterable[Path], root: Path) -> list[Finding]:
    findings: list[Finding] = []
    for path in files:
        if not should_scan(path, root):
            continue
        text = read_text(path)
        if text is None:
            continue
        rel = path.relative_to(root)
        for line_no, line in enumerate(text.splitlines(), start=1):
            for rule in RULES:
                if rule.pattern.search(line):
                    findings.append(Finding(rel, line_no, rule.category))
    return findings


def parse_digest_set(raw: str) -> set[str]:
    digests: set[str] = set()
    invalid_count = 0
    for item in re.split(r"[\s,]+", raw.strip()):
        if not item:
            continue
        value = item.removeprefix("hmac-sha256:").lower()
        if re.fullmatch(r"[0-9a-f]{64}", value):
            digests.add(value)
        else:
            invalid_count += 1
    if invalid_count:
        raise ValueError(f"invalid HMAC denylist entries: {invalid_count}")
    return digests


def normalized_tokens(line: str) -> list[str]:
    return re.findall(r"[a-z0-9]+", line.lower())


def candidate_hmacs(line: str, key: bytes, max_ngram: int = 5) -> set[str]:
    tokens = normalized_tokens(line)
    digests: set[str] = set()
    for start in range(len(tokens)):
        upper = min(len(tokens), start + max_ngram)
        for end in range(start + 1, upper + 1):
            phrase = " ".join(tokens[start:end])
            digest = hmac.new(key, phrase.encode("utf-8"), hashlib.sha256).hexdigest()
            digests.add(digest)
    return digests


def scan_trusted_hmacs(
    files: Iterable[Path], root: Path, denylist_digests: set[str], key: bytes
) -> list[Finding]:
    findings: list[Finding] = []
    if not denylist_digests:
        return findings
    for path in files:
        if not should_scan(path, root):
            continue
        text = read_text(path)
        if text is None:
            continue
        rel = path.relative_to(root)
        for line_no, line in enumerate(text.splitlines(), start=1):
            if candidate_hmacs(line, key) & denylist_digests:
                findings.append(Finding(rel, line_no, "trusted_private_hash_match"))
    return findings


def trusted_hmac_canary_path(root: Path, raw_path: str) -> Path:
    path = Path(raw_path)
    if not path.is_absolute():
        path = root / path
    return path.resolve()


def split_canary_findings(
    findings: Sequence[Finding], canary_rel: Path
) -> tuple[list[Finding], list[Finding]]:
    canary_findings: list[Finding] = []
    real_findings: list[Finding] = []
    for finding in findings:
        if finding.path == canary_rel:
            canary_findings.append(finding)
        else:
            real_findings.append(finding)
    return real_findings, canary_findings


def print_findings(findings: Sequence[Finding]) -> None:
    counts: dict[str, int] = {}
    for finding in findings:
        counts[finding.category] = counts.get(finding.category, 0) + 1

    print("public-artifact-sanitization=failed")
    print(f"finding_count={len(findings)}")
    for category in sorted(counts):
        print(f"category_count {category}={counts[category]}")

    print("locations:")
    for finding in findings:
        print(f"- {finding.path}:{finding.line} category={finding.category}")


def self_test() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        marker = "".join(("REDACT", "_BEFORE_PUBLIC"))
        (root / "README.md").write_text(
            f"hello\n{marker}: placeholder\nalpha beta\n", encoding="utf-8"
        )
        (root / "target").mkdir()
        (root / "target" / "generated.txt").write_text(
            "DO NOT PUBLISH\n", encoding="utf-8"
        )
        files = [root / "README.md", root / "target" / "generated.txt"]
        outside = root.parent / "outside-marker.txt"
        outside.write_text(f"{marker}\n", encoding="utf-8")
        symlink = root / "linked-marker.txt"
        try:
            symlink.symlink_to(outside)
        except (OSError, NotImplementedError):
            pass
        else:
            files.append(symlink)
        findings = scan_files(files, root)
        assert len(findings) == 1
        assert findings[0].path == Path("README.md")
        assert findings[0].category == "publication_blocker_marker"
        trusted_key = b"trusted-test-key"
        trusted_digest = hmac.new(trusted_key, b"alpha beta", hashlib.sha256).hexdigest()
        trusted_findings = scan_trusted_hmacs(
            [root / "README.md"], root, {trusted_digest}, trusted_key
        )
        assert len(trusted_findings) == 1
        assert trusted_findings[0].path == Path("README.md")
        assert trusted_findings[0].category == "trusted_private_hash_match"
        canary_findings = [
            Finding(Path(".github/sanitizer/trusted-hmac-canary.txt"), 1, "trusted_private_hash_match"),
            Finding(Path("README.md"), 3, "trusted_private_hash_match"),
        ]
        real_findings, canary_hits = split_canary_findings(
            canary_findings, Path(".github/sanitizer/trusted-hmac-canary.txt")
        )
        assert len(real_findings) == 1
        assert len(canary_hits) == 1
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        canary_phrase = " ".join(
            (
                "public",
                "sanitizer",
                "hmac",
                "canary",
            )
        )
        (root / ".github" / "sanitizer").mkdir(parents=True)
        (root / "README.md").write_text("hello\n", encoding="utf-8")
        (root / ".github" / "sanitizer" / "trusted-hmac-canary.txt").write_text(
            f"{canary_phrase}\n", encoding="utf-8"
        )
        subprocess.run(["git", "init", "-q"], cwd=root, check=True)
        subprocess.run(["git", "add", "."], cwd=root, check=True)
        trusted_key = "trusted-test-key"
        trusted_digest = hmac.new(
            trusted_key.encode("utf-8"), canary_phrase.encode("utf-8"), hashlib.sha256
        ).hexdigest()
        old_workspace = os.environ.get("GITHUB_WORKSPACE")
        old_digests = os.environ.get("TEST_HMAC_DIGESTS")
        old_key = os.environ.get("TEST_HMAC_KEY")
        try:
            os.environ["GITHUB_WORKSPACE"] = str(root)

            def run_with_env(digests: str | None, key: str | None) -> int:
                if digests is None:
                    os.environ.pop("TEST_HMAC_DIGESTS", None)
                else:
                    os.environ["TEST_HMAC_DIGESTS"] = digests
                if key is None:
                    os.environ.pop("TEST_HMAC_KEY", None)
                else:
                    os.environ["TEST_HMAC_KEY"] = key
                with contextlib.redirect_stdout(io.StringIO()):
                    return main(
                        [
                            "--trusted-hmac-env",
                            "TEST_HMAC_DIGESTS",
                            "--trusted-hmac-key-env",
                            "TEST_HMAC_KEY",
                            "--trusted-hmac-canary-file",
                            ".github/sanitizer/trusted-hmac-canary.txt",
                        ]
                    )

            assert run_with_env(None, None) == 0
            assert run_with_env(trusted_digest, None) == 1
            assert run_with_env(None, trusted_key) == 1
            assert run_with_env("not-a-digest", trusted_key) == 1
            assert run_with_env(trusted_digest, "wrong-key") == 1
            assert run_with_env(trusted_digest, trusted_key) == 0
        finally:
            if old_workspace is None:
                os.environ.pop("GITHUB_WORKSPACE", None)
            else:
                os.environ["GITHUB_WORKSPACE"] = old_workspace
            if old_digests is None:
                os.environ.pop("TEST_HMAC_DIGESTS", None)
            else:
                os.environ["TEST_HMAC_DIGESTS"] = old_digests
            if old_key is None:
                os.environ.pop("TEST_HMAC_KEY", None)
            else:
                os.environ["TEST_HMAC_KEY"] = old_key
    print("self-test=passed")


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument(
        "--trusted-hmac-env",
        help=(
            "Environment variable containing newline/comma separated HMAC-SHA256 "
            "digests for the trusted private-list scan."
        ),
    )
    parser.add_argument(
        "--trusted-hmac-key-env",
        help="Environment variable containing the HMAC key for the trusted scan.",
    )
    parser.add_argument(
        "--trusted-hmac-canary-file",
        help=(
            "Repository-relative canary fixture that must match the trusted "
            "HMAC list when trusted config is present."
        ),
    )
    args = parser.parse_args(argv)

    if args.self_test:
        self_test()
        return 0

    root = Path(os.environ.get("GITHUB_WORKSPACE", REPO_ROOT)).resolve()
    files = git_files(root)
    findings = scan_files(files, root)
    if findings:
        print_findings(findings)
        return 1
    if args.trusted_hmac_env or args.trusted_hmac_key_env:
        raw_digests = os.environ.get(args.trusted_hmac_env or "", "")
        raw_key = os.environ.get(args.trusted_hmac_key_env or "", "")
        if not raw_digests.strip() and not raw_key.strip():
            print("trusted-private-list=skipped")
            print("trusted-private-list-reason=hmac_source_unavailable")
        elif not raw_digests.strip() or not raw_key.strip():
            print("trusted-private-list=failed")
            print("trusted-private-list-reason=hmac_configuration_incomplete")
            return 1
        else:
            try:
                denylist_digests = parse_digest_set(raw_digests)
            except ValueError as exc:
                print("trusted-private-list=failed")
                print(str(exc))
                return 1
            trusted_findings = scan_trusted_hmacs(
                files, root, denylist_digests, raw_key.encode("utf-8")
            )
            if args.trusted_hmac_canary_file:
                canary_path = trusted_hmac_canary_path(root, args.trusted_hmac_canary_file)
                try:
                    canary_rel = canary_path.relative_to(root)
                except ValueError:
                    print("trusted-private-list=failed")
                    print("trusted-private-list-reason=hmac_canary_outside_workspace")
                    return 1
                if not canary_path.exists():
                    print("trusted-private-list=failed")
                    print("trusted-private-list-reason=hmac_canary_missing")
                    return 1
                trusted_findings, canary_findings = split_canary_findings(
                    trusted_findings, canary_rel
                )
                if not canary_findings:
                    print("trusted-private-list=failed")
                    print("trusted-private-list-reason=hmac_canary_mismatch")
                    return 1
            if trusted_findings:
                print_findings(trusted_findings)
                return 1
            print("trusted-private-list=passed")
    print("public-artifact-sanitization=passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
