#!/usr/bin/env python3
"""Typed loading and projection for published adequacy measurements."""

from __future__ import annotations

import hashlib
import json
import os
import re
import stat
from dataclasses import dataclass
from pathlib import Path, PurePosixPath

MAX_RESULTS_BYTES = 8 * 1024 * 1024
REPORT_SCHEMA = "corpus-adequacy.report.v0"
RESULTS_SCHEMA = "assay.conformance.adequacy.results.v0"
HEX40 = re.compile(r"[0-9a-f]{40}")
SHA256 = re.compile(r"sha256:[0-9a-f]{64}")
RUNNERS = {"module", "process", "batch"}
CONTROL = {"killed", "survived", "error", "absent-or-invalid"}
COUNT_FIELDS = (
    "killed", "survived", "silent", "equivalent",
    "unexercised_out_of_scope", "known_holes", "unproved", "declared_total",
)
ROW_FROM_REPORT = {
    "runner": "runner", "killed": "killed", "survived": "survived",
    "silent": "silent", "equivalent": "equivalent",
    "out_of_scope": "unexercised_out_of_scope", "known_holes": "known_holes",
    "unproved": "unproved", "declared_total": "declared_total",
    "score_percent": "score_percent", "adequate": "adequate",
    "diagnostic_channel_declared": "diagnostic_channel_declared",
    "control_status": "control_status", "manifest_sha256": "manifest_sha256",
    "tool_commit": "tool_commit", "tool_source_state": "tool_source_state",
    "tool_content_sha256": "tool_content_sha256", "tool_version": "tool_version",
}


@dataclass(frozen=True)
class LoadedResults:
    document: dict
    rows: tuple[dict, ...]

    def by_corpus(self) -> dict[str, dict]:
        return {row["corpus"]: row for row in self.rows}


def _digest(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()


def read_regular_file(path: Path, limit: int = MAX_RESULTS_BYTES) -> bytes:
    """Read a bounded regular file without following a symlink or blocking on a FIFO."""
    try:
        before = os.lstat(path)
    except OSError as exc:
        raise ValueError("%s is not a readable regular file: %s" % (path, exc)) from exc
    if not stat.S_ISREG(before.st_mode):
        raise ValueError("%s is not a regular file" % path)
    flags = os.O_RDONLY
    if hasattr(os, "O_CLOEXEC"):
        flags |= os.O_CLOEXEC
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    if hasattr(os, "O_NONBLOCK"):
        flags |= os.O_NONBLOCK
    try:
        fd = os.open(path, flags)
    except OSError as exc:
        raise ValueError("%s is not a readable regular file: %s" % (path, exc)) from exc
    try:
        info = os.fstat(fd)
        if not stat.S_ISREG(info.st_mode):
            raise ValueError("%s is not a regular file" % path)
        if (before.st_dev, before.st_ino) != (info.st_dev, info.st_ino):
            raise ValueError("%s changed while it was opened" % path)
        if info.st_size > limit:
            raise ValueError("%s exceeds the %d-byte limit" % (path, limit))
        data = os.read(fd, limit + 1)
        if len(data) > limit:
            raise ValueError("%s exceeds the %d-byte limit" % (path, limit))
        return data
    finally:
        os.close(fd)


def _parse_json_bytes(data: bytes, label: str) -> dict:
    try:
        value = json.loads(data.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise ValueError("%s is not readable JSON: %s" % (label, exc)) from exc
    if not isinstance(value, dict):
        raise ValueError("%s must be a JSON object" % label)
    return value


def _integer(value: object, field: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < 0:
        raise ValueError("%s must be a non-negative integer" % field)
    return value


def _sha(value: object, field: str) -> str:
    if not isinstance(value, str) or not SHA256.fullmatch(value):
        raise ValueError("%s must be a sha256 content address" % field)
    return value


def _dependencies(value: object) -> list[str]:
    if not isinstance(value, list) or not value:
        raise ValueError("malformed measured_at.depends_on: expected a non-empty list")
    clean: list[str] = []
    for item in value:
        if not isinstance(item, str) or not item or "\\" in item or item.startswith(":"):
            raise ValueError("malformed measured_at.depends_on contains a hostile path")
        pure = PurePosixPath(item)
        if pure.is_absolute() or any(part in ("", ".", "..") for part in pure.parts):
            raise ValueError("malformed measured_at.depends_on contains a non-canonical path")
        clean.append(item)
    if clean != sorted(set(clean)):
        raise ValueError("malformed measured_at.depends_on must be sorted and unique")
    return clean


def _validate_report(report: dict) -> None:
    if report.get("schema") != REPORT_SCHEMA:
        raise ValueError("successful measurement must be a %s" % REPORT_SCHEMA)
    if report.get("runner") not in RUNNERS:
        raise ValueError("runner must be module, process, or batch")
    for field in COUNT_FIELDS:
        _integer(report.get(field), field)
    total = sum(report[field] for field in COUNT_FIELDS[:-1])
    if report["declared_total"] != total:
        raise ValueError("declared_total does not match the producer counts")
    score = report.get("score_percent")
    if score is not None and (isinstance(score, bool) or not isinstance(score, (int, float))):
        raise ValueError("score_percent must be numeric or null")
    if not isinstance(report.get("adequate"), bool):
        raise ValueError("adequate must be boolean")
    if not isinstance(report.get("diagnostic_channel_declared"), bool):
        raise ValueError("diagnostic_channel_declared must be boolean")
    if report.get("control_status") not in CONTROL:
        raise ValueError("control_status is not a producer verdict")
    _sha(report.get("manifest_sha256"), "manifest_sha256")
    if report.get("tool_source_state") != "exact":
        raise ValueError("tool_source_state is %s, not exact" % report.get("tool_source_state"))
    if not isinstance(report.get("tool_commit"), str) or not HEX40.fullmatch(report["tool_commit"]):
        raise ValueError("tool_commit is unresolved")
    _sha(report.get("tool_content_sha256"), "tool_content_sha256")
    if not isinstance(report.get("tool_version"), str) or not report["tool_version"]:
        raise ValueError("tool_version must be a non-empty string")


def _validate_subject(value: object) -> dict:
    if not isinstance(value, dict) or value.get("kind") not in ("in_tree", "out_of_tree"):
        raise ValueError("subject must name an in_tree or out_of_tree source")
    return value


def project_report(
    manifest_path: Path, report: dict, encoded_report: bytes, *, corpus: str,
    manifest: str, measured_commit: str, depends_on: list[str], subject: dict,
) -> dict:
    """Project one exact producer report into the typed publication index."""
    if not isinstance(corpus, str) or not corpus:
        raise ValueError("corpus id must be a non-empty string")
    _validate_report(report)
    if _parse_json_bytes(encoded_report, "producer report") != report:
        raise ValueError("producer report bytes do not encode the supplied report")
    manifest_data = read_regular_file(manifest_path)
    declared = _parse_json_bytes(manifest_data, str(manifest_path))
    if declared.get("runner", "module") != report["runner"]:
        raise ValueError("runner differs between manifest and producer report")
    if report["manifest_sha256"] != _digest(manifest_data):
        raise ValueError("manifest_sha256 does not address the exact manifest bytes")
    if not isinstance(measured_commit, str) or not HEX40.fullmatch(measured_commit):
        raise ValueError("malformed measured_at.commit must be a full Git object id")
    dependencies = _dependencies(depends_on)
    report_sha = _digest(encoded_report)
    control_status = report["control_status"]
    control = {
        "killed": "killed", "survived": "SURVIVED", "error": "error",
        "absent-or-invalid": "none_declared",
    }[control_status]
    return {
        "corpus": corpus, "manifest": manifest, "runner": report["runner"],
        "killed": report["killed"], "survived": report["survived"],
        "silent": report["silent"], "equivalent": report["equivalent"],
        "out_of_scope": report["unexercised_out_of_scope"],
        "known_holes": report["known_holes"], "unproved": report["unproved"],
        "declared_total": report["declared_total"],
        "score_percent": report["score_percent"], "adequate": report["adequate"],
        "diagnostic_channel_declared": report["diagnostic_channel_declared"],
        "control_status": control_status, "control": control,
        "manifest_sha256": report["manifest_sha256"],
        "report_sha256": report_sha, "report_ref": "#/reports/%s" % report_sha,
        "tool_commit": report["tool_commit"],
        "tool_source_state": report["tool_source_state"],
        "tool_content_sha256": report["tool_content_sha256"],
        "tool_version": report["tool_version"], "provenance": {"kind": "measured"},
        "subject": _validate_subject(subject),
        "measured_at": {"commit": measured_commit, "depends_on": dependencies},
    }


def _validate_current_row(row: dict, reports: dict) -> None:
    for field in ("corpus", "manifest", "report_sha256", "report_ref", "measured_at"):
        if field not in row:
            raise ValueError("current row is missing %s" % field)
    digest = _sha(row["report_sha256"], "report_sha256")
    if row["report_ref"] != "#/reports/%s" % digest:
        raise ValueError("report_ref does not address report_sha256")
    encoded = reports.get(digest)
    if not isinstance(encoded, str):
        raise ValueError("report_ref does not resolve to producer report bytes")
    raw = encoded.encode("utf-8")
    if _digest(raw) != digest:
        raise ValueError("report_sha256 does not address the stored producer bytes")
    report = _parse_json_bytes(raw, "stored producer report")
    _validate_report(report)
    for row_field, report_field in ROW_FROM_REPORT.items():
        if row.get(row_field) != report.get(report_field):
            raise ValueError(
                "%s: the measurement gives %r but the indexed row gives %r"
                % (row_field, report.get(report_field), row.get(row_field)))
    measured = row.get("measured_at")
    if not isinstance(measured, dict) or not isinstance(measured.get("commit"), str):
        raise ValueError("measured_at must name a commit and dependencies")
    if not HEX40.fullmatch(measured["commit"]):
        raise ValueError("malformed measured_at.commit must be a full Git object id")
    _dependencies(measured.get("depends_on"))
    _validate_subject(row.get("subject"))


def load_results(path: Path, *, limit: int = MAX_RESULTS_BYTES) -> LoadedResults:
    document = _parse_json_bytes(read_regular_file(path, limit), "%s results JSON" % path)
    if document.get("schema") not in (None, RESULTS_SCHEMA):
        raise ValueError("results document has an unsupported schema")
    rows = document.get("corpora")
    if not isinstance(rows, list):
        raise ValueError("results document has no corpora list")
    reports = document.get("reports")
    current = reports is not None
    if current and not isinstance(reports, dict):
        raise ValueError("results reports must be an object")
    if not current and any(isinstance(row, dict) and "report_sha256" in row for row in rows):
        raise ValueError("current rows cannot be downgraded by removing the reports object")
    unmeasured = document.get("unmeasured", [])
    if (not isinstance(unmeasured, list)
            or any(not isinstance(item, str) or not item for item in unmeasured)
            or unmeasured != sorted(set(unmeasured))):
        raise ValueError("unmeasured must be a sorted unique list of corpus ids")
    seen: set[str] = set()
    addressed: set[str] = set()
    for row in rows:
        if not isinstance(row, dict) or not isinstance(row.get("corpus"), str) or not row["corpus"]:
            raise ValueError("every results row needs a non-empty string corpus id")
        name = row["corpus"]
        if name in seen:
            raise ValueError("duplicate corpus id: %s" % name)
        seen.add(name)
        if current:
            _validate_current_row(row, reports)
            addressed.add(row["report_sha256"])
    if current and set(reports) != addressed:
        raise ValueError("results reports must contain exactly the addressed producer reports")
    return LoadedResults(document=document, rows=tuple(rows))
