#!/usr/bin/env python3
"""Windows AppContainer feasibility probe.

The verdict is a pure function of parsed receipts. ``--self-test`` is a POSIX
check: it does not load Windows DLLs or change the host. The checker and this
test were written together. No failing run was recorded before the checker
existed. The hosted path is Windows-only and was not executed on the machine
that wrote this file.

Assumptions, untested on a Windows host:
- PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES is 0x20009.
- PROC_THREAD_ATTRIBUTE_HANDLE_LIST is 0x20002. The child inherits only NUL,
  the stdout pipe, and the stderr pipe.
- TokenIsAppContainer, TokenCapabilities, and TokenAppContainerSid are 29, 30, 31.
- internetClient's capability SID is S-1-15-3-1. DeriveCapabilitySidsFromName
  is resolved from KernelBase.dll and returns BOOL. FALSE keeps GetLastError.
  A missing export is setup failure. The caller frees every returned SID and
  both arrays on every path. Each SID is passed to ConvertSidToStringSidW
  and to LocalFree as a pointer, not as a bare integer, so an address above
  4GB is not narrowed. The
  documented SID is not substituted for that
  call. A name is recorded only when the token SID equals the SID that call
  returns.
- Event 5157 Direction is the documented word Outbound or Inbound. The
  official sample XML contains %%14592 and does not define that token. The
  collector keeps that raw XML and asks EvtFormatMessage, with the event's own
  provider and the %% message id from that record, for a bounded parameter
  string. Only an exact Inbound or Outbound result becomes a direction. Any
  other result, including a sentence or a failed render, stays unknown, so a
  hosted run may stay INCONCLUSIVE. The token is not given a numeric meaning.
  FilterRTID is the
  documented Filter Run-Time ID. netsh wfp show filters file=- verbose=on
  writes the inventory to stdout. The parent reads that stream through the
  shared supervisor, up to FILTER_ACQUIRE_BYTES. That ceiling is not
  COMMAND_STDOUT_BYTES, and it does not bound netsh or BFE memory. One unread
  byte, a missing EOF, a timeout, or a nonzero exit makes the capture
  incomplete, even when every cited id was already in the bytes read. The
  retained file keeps only the cited items and stays at most 65536 bytes.
  Strict UTF-8, UTF-8 with a BOM, and UTF-16 with a BOM are decoded. A decode
  error is not replaced. ElementTree skips exactly one leading U+FEFF and
  no other prefix; the declaration is read from that same string. A second
  U+FEFF is not a missing declaration. A leading XML declaration must name
  that same encoding, compared case-insensitively, or be absent. UTF-16LE,
  UTF-16BE, UTF-32, and a legacy code page stay unsupported. A conflicting declaration
  is incomplete and is not removed. An exit first observed at or after the
  cutoff is incomplete. The poll pause does not extend the deadline, and
  cleanup keeps its own budget. Non-XML console text, malformed XML, a missing
  id, or a missing receipt stays incomplete, and wfp_filters_incomplete still
  keeps a pass from completing.
  Command, PowerShell, and grandchild pipes are counted while they are read.
  Each read is a fixed-size chunk, both pipes are counted, and one deadline
  covers the process. Output that arrives after that deadline is not success.
  Closing that child is a separate bounded step; an unknown cleanup is not
  success. The download ceiling and the archive decode ceiling are different
  limits even when both are 32 MiB. Declared zip sizes are not that counter.
  A failed extract does not leave assay.exe. The AppContainer launch drain is
  a different contract: it reads the handles it created with ReadFile, and
  its cleanup is the acquired ledger.
- ProcessID may be hexadecimal. Match times use a fixed 2 second slack, not an
  error code learned from a leg.
- Sockets go through the stdlib, which calls ws2_32. WinError is recorded and
  is not a denial category.
- ACE edits use icacls with an explicit SID. A hand-packed TRUSTEE can name
  the wrong principal, so that path is not used.
- DeleteAppContainerProfile success is not proof the registration is gone.
- JOBOBJECT_EXTENDED_LIMIT_INFORMATION is the 64-bit layout, and
  JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE is 0x2000. A rejected job call is setup.
- The Linux consumer verifier (docker, then unshare/network-none) is not run.
  Signature verification is a connected cosign verify-blob on the host.
  The AppContainer arm only runs the already checked assay.exe.
"""

from __future__ import annotations

import ast
import hashlib
import json
import os
import re
import sys
import xml.etree.ElementTree as ET
from datetime import datetime, timedelta, timezone
from pathlib import Path

RELEASE_TAG = "v6.6.2"
REPOSITORY = "Rul1an/assay"
CERTIFICATE_IDENTITY = (
    "https://github.com/"
    + REPOSITORY
    + "/.github/workflows/release.yml@refs/tags/"
    + RELEASE_TAG
)
CERTIFICATE_OIDC_ISSUER = "https://token.actions.githubusercontent.com"
# The workflow reads this assignment. Keep it a single string literal.
COSIGN_RELEASE = "v3.1.3"
ARCHIVE_NAME = "assay-" + RELEASE_TAG + "-x86_64-pc-windows-msvc.zip"
INTERNET_CLIENT_SID = "S-1-15-3-1"
ALL_APPLICATION_PACKAGES = "S-1-15-2-1"
EVENT_WINDOW_SLACK_SECONDS = 2
FILTER_SELECTED_BYTES = 65536
# Parent read of `netsh wfp show filters file=- verbose=on`. Not COMMAND_STDOUT_BYTES.
# The matching integers are not the same limit, and neither bounds netsh or BFE memory.
FILTER_ACQUIRE_BYTES = 262144
DIRECTION_RENDER_CHARS = 32
RELEASE_DOWNLOAD_BYTES = 32 * 1024 * 1024
READ_CHUNK_BYTES = 65536
COMMAND_STDOUT_BYTES = 262144
COMMAND_STDERR_BYTES = 65536
POWERSHELL_STDOUT_BYTES = 32768
GRANDCHILD_CAPTURE_BYTES = 65536
# Decoded archive bytes, not the compressed download. The integers match; the limits do not.
ARCHIVE_DECODE_BYTES = 32 * 1024 * 1024
ZIP_MEMBER_LIMIT = 16
CLEANUP_BUDGET_SECONDS = 1
BUNDLE_RELATIVE = (
    "conformance/privileged-mcp-action-v0/vectors/"
    "ok-001-deny-bound-observation.bundle.tar.gz"
)
REPORT_SCHEMA = "assay.privileged_mcp_action.verify.report.v0"
PROFILE_ID = "privileged-mcp-action/v0"
NON_CLAIMS = [
    "allow does not prove upstream delivery",
    "deny does not establish maliciousness",
    "caller-visible denial does not prove external side-effect absence",
    "bundle integrity does not upgrade source class",
]
EXPECTED_CLAIMS = {
    "policy_decision_recorded": {
        "status": "confirmed",
        "source_class": "producer_reported",
    },
    "caller_visible_denial": {
        "status": "confirmed",
        "source_class": "producer_reported",
    },
    "upstream_delivery": {"status": "incomplete"},
    "external_side_effect": {"status": "incomplete"},
}
MANDATORY_LEGS = ("tcp_loopback", "tcp_external")
OPTIONAL_LEGS = ("udp_loopback", "udp_dns", "name_resolution")
ROOT = Path(__file__).resolve().parents[2]
BUNDLE_PATH = ROOT / BUNDLE_RELATIVE
_LOADED_WIN32 = False
_SID_RE = re.compile(r"S-1-\d+(?:-\d+)+")
_ISO_RE = re.compile(
    r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})"
)


class SetupError(Exception):
    pass


def plan_document():
    return {
        "experiment": "windows-appcontainer-offline-feasibility",
        "mandatory_legs": list(MANDATORY_LEGS),
        "optional_legs": list(OPTIONAL_LEGS),
        "verdicts": {
            "PASS": (
                "Mandatory host controls connected, tokens match the one shared "
                "profile, both mandatory C0 legs and the grandchild external leg "
                "are policy-denied by a matching event 5157, the connected "
                "signature preflight succeeded, and the offline verifier reports "
                "match ok-001 and each other."
            ),
            "INCONCLUSIVE": (
                "A required observation is missing or does not meet the denial "
                "rule. A missing 5157 is never a new pass errno."
            ),
            "MECHANISM_FAILS": (
                "A mandatory C0 leg or the grandchild external leg connected."
            ),
            "SETUP": "Harness or verifier authentication failed.",
        },
        "completed": "PASS and cleanup status clean",
        "five_platform_acceptance": False,
        "non_claims": [
            "not five-platform acceptance",
            "name resolution is not proof of an external DNS query",
            "profile registration removal is not inspectable",
            "a WFP filter id is not identified as the AppContainer filter",
            "the Linux unshare verifier is not this arm",
        ],
    }


def canonical_address(value):
    text = str(value).strip().lower()
    if text.startswith("::ffff:"):
        text = text[7:]
    return text


def parse_pid(value):
    if isinstance(value, bool) or value is None:
        return None
    if isinstance(value, int):
        return value
    text = str(value).strip()
    try:
        return int(text, 16) if text.lower().startswith("0x") else int(text)
    except ValueError:
        return None


def parse_time(value):
    if not isinstance(value, str):
        return None
    text = value.strip().replace("Z", "+00:00")
    if "." in text:
        head, tail = text.split(".", 1)
        suffix = ""
        for index, char in enumerate(tail):
            if char in "+-":
                suffix = tail[index:]
                tail = tail[:index]
                break
        text = head + "." + tail[:6].ljust(6, "0") + suffix
    try:
        parsed = datetime.fromisoformat(text)
    except ValueError:
        return None
    if parsed.tzinfo is None:
        return None
    return parsed.astimezone(timezone.utc)


def window_bounds(start, end):
    slack = timedelta(seconds=EVENT_WINDOW_SLACK_SECONDS)
    return start - slack, end + slack


def query_window_iso(start_text, end_text):
    start = parse_time(start_text)
    end = parse_time(end_text)
    if start is None or end is None or not _ISO_RE.fullmatch(start_text):
        return None
    if not _ISO_RE.fullmatch(end_text):
        return None
    opened, closed = window_bounds(start, end)
    pattern = "%Y-%m-%dT%H:%M:%SZ"
    return (
        opened.strftime(pattern),
        closed.strftime(pattern),
    )


def _local(tag):
    return tag.rsplit("}", 1)[-1]


def parse_event_xml(xml_text):
    if not isinstance(xml_text, str) or len(xml_text) > 8192:
        return None
    try:
        root = ET.fromstring(xml_text)
    except ET.ParseError:
        return None
    event_id = None
    created = None
    provider = None
    record_id = None
    fields = {}
    for node in root.iter():
        name = _local(node.tag)
        if name == "EventID" and node.text:
            event_id = node.text.strip()
        elif name == "Provider" and not provider:
            provider = node.attrib.get("Name")
        elif name == "EventRecordID" and node.text and record_id is None:
            record_id = node.text.strip()
        elif name == "TimeCreated":
            created = node.attrib.get("SystemTime")
        elif name == "Data":
            key = node.attrib.get("Name")
            if key and node.text is not None:
                fields[key] = node.text.strip()
    if event_id != "5157":
        return None
    pid = parse_pid(fields.get("ProcessID") or fields.get("ProcessId"))
    destination = fields.get("DestAddress") or fields.get("DestinationAddress")
    port = parse_pid(fields.get("DestPort") or fields.get("DestinationPort"))
    protocol_raw = fields.get("Protocol")
    when = parse_time(created) if created else None
    if pid is None or destination is None or port is None or when is None:
        return None
    protocol_code = parse_pid(protocol_raw)
    if protocol_code == 6 or (protocol_raw or "").lower() == "tcp":
        protocol = "tcp"
    elif protocol_code == 17 or (protocol_raw or "").lower() == "udp":
        protocol = "udp"
    else:
        protocol = (protocol_raw or "").lower()
    direction_raw = (fields.get("Direction") or "").strip()
    folded = direction_raw.casefold()
    if folded == "outbound":
        direction = "outbound"
    elif folded == "inbound":
        direction = "inbound"
    else:
        direction = None
    filter_id = fields.get("FilterRTID")
    if not (isinstance(filter_id, str) and filter_id.isdigit()):
        filter_id = None
    return {
        "id": 5157,
        "pid": pid,
        "protocol": protocol,
        "destination": canonical_address(destination),
        "port": port,
        "direction": direction,
        "raw_direction": direction_raw,
        "filter_runtime_id": filter_id,
        "raw_xml": xml_text,
        "provider": provider,
        "record_id": record_id,
        "time": when.isoformat(),
    }


def records_for_leg(events, leg, pid):
    if not isinstance(events, list):
        return []
    kept = []
    for event in events:
        if event_correlates(event, leg, pid):
            kept.append(event)
        if len(kept) >= 40:
            break
    return kept


def event_correlates(event, leg, pid):
    if not isinstance(event, dict) or not isinstance(leg, dict):
        return False
    if event.get("id") != 5157:
        return False
    if parse_pid(event.get("pid")) != parse_pid(pid):
        return False
    if event.get("protocol") != leg.get("protocol"):
        return False
    if canonical_address(event.get("destination")) != canonical_address(
        leg.get("target")
    ):
        return False
    if parse_pid(event.get("port")) != parse_pid(leg.get("port")):
        return False
    start = parse_time(leg.get("start"))
    end = parse_time(leg.get("end"))
    moment = parse_time(event.get("time"))
    if start is None or end is None or moment is None:
        return False
    opened, closed = window_bounds(start, end)
    return opened <= moment <= closed


def event_matches(event, leg, pid):
    return event_correlates(event, leg, pid) and event.get("direction") == "outbound"


def capability_names(sid_strings, internet_client_sid):
    sids = [str(item) for item in sid_strings]
    if sids == [internet_client_sid]:
        return ["internetClient"]
    return sids


def valid_sid(value):
    return isinstance(value, str) and bool(_SID_RE.fullmatch(value))


def child_environment(source):
    blocked = ("TOKEN", "SECRET", "PASSWORD", "PASSWD", "CREDENTIAL")
    kept = {}
    for key, value in source.items():
        if not isinstance(key, str) or not isinstance(value, str):
            continue
        if "\0" in key or "\0" in value or "=" in key:
            continue
        upper = key.upper()
        if any(part in upper for part in blocked):
            continue
        kept[key] = value
    return kept


def checksum_digest(text, archive_name):
    if not isinstance(text, str) or not isinstance(archive_name, str):
        return None
    matches = []
    for line in text.splitlines():
        if len(line) < 66 or line[64:66] != "  ":
            continue
        digest, name = line[:64], line[66:]
        if name != archive_name:
            continue
        if len(digest) != 64 or any(char not in "0123456789abcdef" for char in digest):
            return None
        matches.append(digest)
    if len(matches) != 1:
        return None
    return matches[0]


def sha256_file(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def cosign_version_ok(output):
    return isinstance(output, str) and COSIGN_RELEASE in output


def safe_zip_member(name):
    if not isinstance(name, str) or not name:
        return False
    if name.startswith("/") or name.startswith("\\"):
        return False
    parts = name.replace("\\", "/").split("/")
    return not any(part in ("", ".", "..") for part in parts)


def report_ok(stdout, exit_code):
    if exit_code != 0 or not isinstance(stdout, str) or not stdout.strip():
        return False
    try:
        report = json.loads(stdout)
    except json.JSONDecodeError:
        return False
    if not isinstance(report, dict):
        return False
    if report.get("schema") != REPORT_SCHEMA or report.get("profile") != PROFILE_ID:
        return False
    if report.get("bundle_integrity") != "pass" or report.get("verdict") != "valid":
        return False
    if "reason_code" in report or "next_step" in report:
        return False
    if report.get("non_claims") != NON_CLAIMS or report.get("claims") != EXPECTED_CLAIMS:
        return False
    return True


def _leg(arm, name):
    if not isinstance(arm, dict):
        return None
    legs = arm.get("legs")
    if not isinstance(legs, dict):
        return None
    leg = legs.get(name)
    return leg if isinstance(leg, dict) else None


def _endpoint_ok(leg, pinned):
    if not leg or not isinstance(pinned, dict):
        return False
    return canonical_address(leg.get("target")) == canonical_address(
        pinned.get("address")
    ) and parse_pid(leg.get("port")) == parse_pid(pinned.get("port"))


def _policy_state(leg, events, pid, flags):
    if not leg:
        return "missing"
    result = leg.get("result")
    if result == "connected":
        if "ignore_connect" in flags:
            return "denied"
        return "connected"
    if result != "failed":
        return "inconclusive"
    if "ignore_5157" in flags:
        return "denied"
    found = any(_event_proof(event, leg, pid) for event in events or [])
    return "denied" if found else "inconclusive"


def _event_proof(event, leg, pid):
    if not event_matches(event, leg, pid):
        return False
    raw = event.get("raw_xml")
    if event.get("truncated") or not isinstance(raw, str) or not raw.strip() or len(raw) > 8192:
        return False
    if not str(event.get("filter_runtime_id") or "").isdigit():
        return False
    raw_direction = event.get("raw_direction") or ""
    if raw_direction.startswith("%%"):
        if not raw_direction[2:].isdigit():
            return False
        if event.get("render_message_id") != int(raw_direction[2:]):
            return False
        if not event.get("provider") or event.get("record_id") in (None, ""):
            return False
    return True


def _token_ok(token, sid, names, arm_pid, flags, ignore_name):
    if ignore_name in flags:
        return True
    if not isinstance(token, dict) or not token.get("is_app_container"):
        return False
    if token.get("sid") != sid or not sid:
        return False
    if set(token.get("capabilities") or []) != set(names):
        return False
    if parse_pid(token.get("pid")) != parse_pid(arm_pid):
        return False
    return True


def _cleanup_status(receipts):
    status = (receipts.get("cleanup") or {}).get("status") if isinstance(receipts, dict) else None
    if status in ("clean", "dirty", "unknown"):
        return status
    return "unknown"


def _optional_hits(receipts):
    hits = []
    c0 = receipts.get("c0") if isinstance(receipts, dict) else None
    for name in ("udp_loopback", "udp_dns"):
        leg = _leg(c0, name)
        if leg and leg.get("result") == "connected":
            hits.append(name)
    observed = receipts.get("name_resolution") if isinstance(receipts, dict) else None
    if isinstance(observed, dict) and observed.get("result") in (
        "observed",
        "connected",
        "success",
    ):
        hits.append("name_resolution")
    return hits


def _retained_filter_ids(receipts):
    found = []
    if not isinstance(receipts, dict):
        return found
    for label in ("c1_before", "c0", "c1_after"):
        arm = receipts.get(label)
        if not isinstance(arm, dict):
            continue
        groups = [arm.get("events")]
        grandchild = arm.get("grandchild")
        if isinstance(grandchild, dict):
            groups.append(grandchild.get("events"))
        for events in groups:
            if not isinstance(events, list):
                continue
            for event in events:
                if not isinstance(event, dict):
                    continue
                filter_id = event.get("filter_runtime_id")
                if isinstance(filter_id, str) and filter_id.isdigit():
                    found.append(filter_id)
    return found


def _item_filter_id(item_xml):
    if "<!DOCTYPE" in item_xml or "<!ENTITY" in item_xml:
        return None
    if len(item_xml.encode("utf-8")) > FILTER_SELECTED_BYTES:
        return None
    try:
        root = ET.fromstring(item_xml)
    except ET.ParseError:
        return None
    for child in root.iter():
        if _local(child.tag) != "filterId" or not isinstance(child.text, str):
            continue
        candidate = child.text.strip()
        if candidate.isdigit():
            return candidate
    return None


def _item_slices(text):
    start = 0
    while start < len(text):
        open_at = text.find("<item", start)
        if open_at < 0:
            return
        close_at = text.find("</item>", open_at)
        if close_at < 0:
            return
        end = close_at + len("</item>")
        yield text[open_at:end]
        start = end


def select_filter_evidence(xml_text, runtime_ids, hit_ceiling=False):
    wanted = []
    for item in runtime_ids or []:
        text = str(item)
        if text.isdigit() and text not in wanted:
            wanted.append(text)
    if not isinstance(xml_text, str):
        return {"text": "", "truncated": False, "failed": True}
    encoded_len = len(xml_text.encode("utf-8"))
    if encoded_len > FILTER_ACQUIRE_BYTES or hit_ceiling:
        return {"text": "", "truncated": True, "failed": False}
    if not wanted:
        return {"text": "", "truncated": False, "failed": True}
    kept = []
    seen = set()
    for item_xml in _item_slices(xml_text):
        filter_id = _item_filter_id(item_xml)
        if filter_id in wanted and filter_id not in seen:
            kept.append(item_xml)
            seen.add(filter_id)
    if seen != set(wanted):
        return {"text": "", "truncated": False, "failed": True}
    body = "<filters>" + "".join(kept) + "</filters>"
    if len(body.encode("utf-8")) > FILTER_SELECTED_BYTES:
        return {"text": "", "truncated": True, "failed": False}
    return {"text": body, "truncated": False, "failed": False}


def _capture_reasons(receipts):
    reasons = []
    filters = receipts.get("wfp_filters")
    text = filters.get("text") if isinstance(filters, dict) else None
    if (
        not isinstance(filters, dict)
        or filters.get("failed")
        or filters.get("truncated")
        or not isinstance(text, str)
        or not text.strip()
        or len(text) > FILTER_SELECTED_BYTES
    ):
        reasons.append("wfp_filters_incomplete")
    elif any(filter_id not in text for filter_id in _retained_filter_ids(receipts)):
        reasons.append("wfp_filters_incomplete")
    capture = receipts.get("event_capture")
    if not isinstance(capture, dict) or capture.get("failed") or capture.get("truncated"):
        reasons.append("event_capture_incomplete")
    return reasons


def _claim(hits, _flags):
    parts = []
    if "udp_loopback" in hits:
        parts.append("udp_loopback observed succeeding")
    if "udp_dns" in hits:
        parts.append("udp_dns observed succeeding")
    if "name_resolution" in hits:
        parts.append(
            "name resolution observed; not proof of an external DNS query"
        )
    claim = "TCP only" if not parts else "TCP only, with " + "; ".join(parts)
    return claim, False, False


def _verifier_reasons(receipts):
    verify = receipts.get("verify") if isinstance(receipts, dict) else None
    if not isinstance(verify, dict):
        return ["setup:verifier_unauthenticated"]
    preflight = verify.get("preflight")
    if not isinstance(preflight, dict):
        return ["setup:verifier_unauthenticated"]
    if (
        preflight.get("verified") is not True
        or preflight.get("connected") is not True
        or preflight.get("archive_digest_ok") is not True
        or preflight.get("offline_arm") is not False
        or preflight.get("identity") != CERTIFICATE_IDENTITY
        or preflight.get("issuer") != CERTIFICATE_OIDC_ISSUER
    ):
        return ["setup:verifier_unauthenticated"]
    reasons = []
    outside = verify.get("outside") if isinstance(verify.get("outside"), dict) else {}
    inside = verify.get("inside") if isinstance(verify.get("inside"), dict) else {}
    if outside.get("truncated") or inside.get("truncated"):
        reasons.append("verifier_truncated")
    if outside.get("stdout") != inside.get("stdout"):
        reasons.append("verifier_bytes")
    if outside.get("exit") != 0 or inside.get("exit") != 0:
        reasons.append("verifier_exit")
    if not report_ok(outside.get("stdout"), outside.get("exit")):
        reasons.append("verifier_semantics")
    if not report_ok(inside.get("stdout"), inside.get("exit")):
        reasons.append("verifier_semantics")
    return reasons


def _result(verdict, completed, claim, unqualified, five, reasons, cleanup, hits):
    return {
        "verdict": verdict,
        "completed": completed,
        "claim": claim,
        "unqualified_offline": unqualified,
        "five_platform_acceptance": five,
        "dns_external_query_proven": False,
        "reasons": reasons,
        "cleanup": cleanup,
        "optional_observed": hits,
    }


def evaluate(receipts, weaken=()):
    """Return the verdict for parsed receipts. ``weaken`` is the self-test seam."""
    flags = frozenset(weaken or ())
    if not isinstance(receipts, dict):
        receipts = {}
    cleanup = _cleanup_status(receipts)
    hits = _optional_hits(receipts)
    reasons = []
    profile = receipts.get("profile") if isinstance(receipts.get("profile"), dict) else {}
    if profile.get("created_once") is not True:
        reasons.append("setup:profile_not_once")
    sid = profile.get("sid")
    pinned = receipts.get("resolved_external")
    if not isinstance(pinned, dict) or not pinned.get("address"):
        reasons.append("endpoint_unpinned")
        pinned = {}
    if receipts.get("setup_error"):
        reasons.append("setup:harness")
    reasons.extend(_capture_reasons(receipts))

    def host_ok(name):
        arm = receipts.get(name)
        for leg_name in MANDATORY_LEGS:
            leg = _leg(arm, leg_name)
            if not leg or leg.get("result") != "connected":
                reasons.append(name + ":" + leg_name)
                continue
            if leg_name == "tcp_external" and not _endpoint_ok(leg, pinned):
                reasons.append("endpoint_changed")

    host_ok("h_before")
    host_ok("h_after")
    for label, names in (
        ("c1_before", {"internetClient"}),
        ("c1_after", {"internetClient"}),
    ):
        arm = receipts.get(label) if isinstance(receipts.get(label), dict) else {}
        if not _token_ok(arm.get("token"), sid, names, arm.get("pid"), flags, "ignore_token"):
            reasons.append(label + ":token")
        leg = _leg(arm, "tcp_external")
        connected = bool(leg and leg.get("result") == "connected" and _endpoint_ok(leg, pinned))
        if not connected and "ignore_c1" not in flags:
            reasons.append(label + ":tcp_external")
        elif leg and leg.get("result") == "connected" and not _endpoint_ok(leg, pinned):
            reasons.append("endpoint_changed")

    c0 = receipts.get("c0") if isinstance(receipts.get("c0"), dict) else {}
    if not _token_ok(c0.get("token"), sid, set(), c0.get("pid"), flags, "ignore_token"):
        reasons.append("c0:token")
    mechanism = False
    events = c0.get("events") if isinstance(c0.get("events"), list) else []
    for leg_name in MANDATORY_LEGS:
        state = _policy_state(_leg(c0, leg_name), events, c0.get("pid"), flags)
        leg = _leg(c0, leg_name)
        if leg and leg_name == "tcp_external" and not _endpoint_ok(leg, pinned):
            reasons.append("endpoint_changed")
            state = "inconclusive"
        if state == "connected":
            mechanism = True
            reasons.append("c0:" + leg_name + ":connected")
        elif state != "denied":
            reasons.append("c0:" + leg_name + ":" + state)

    grandchild = c0.get("grandchild") if isinstance(c0.get("grandchild"), dict) else {}
    if "ignore_grandchild" not in flags:
        if grandchild.get("spawn") != "inherited":
            reasons.append("grandchild_not_inherited")
        if not _token_ok(
            grandchild.get("token"),
            sid,
            set(),
            grandchild.get("pid"),
            flags,
            "ignore_grandchild",
        ):
            reasons.append("grandchild:token")
    g_events = grandchild.get("events") if isinstance(grandchild.get("events"), list) else []
    g_state = _policy_state(
        _leg(grandchild, "tcp_external"), g_events, grandchild.get("pid"), flags
    )
    g_leg = _leg(grandchild, "tcp_external")
    if g_leg and not _endpoint_ok(g_leg, pinned):
        reasons.append("endpoint_changed")
        g_state = "inconclusive"
    if g_state == "connected":
        mechanism = True
        reasons.append("grandchild:tcp_external:connected")
    elif g_state != "denied":
        reasons.append("grandchild:tcp_external:" + g_state)

    if "ignore_verifier" not in flags:
        reasons.extend(_verifier_reasons(receipts))

    setup = any(item.startswith("setup:") for item in reasons)
    other = [item for item in reasons if not item.startswith("setup:")]
    if mechanism:
        verdict = "MECHANISM_FAILS"
    elif setup:
        verdict = "SETUP"
    elif other:
        verdict = "INCONCLUSIVE"
    else:
        verdict = "PASS"
    claim, unqualified, five = ("", False, False)
    if verdict == "PASS":
        claim, unqualified, five = _claim(hits, flags)
    completed = verdict == "PASS" and cleanup == "clean"
    if "ignore_cleanup" in flags and verdict == "PASS":
        completed = True
    return _result(verdict, completed, claim, unqualified, five, reasons, cleanup, hits)


def _success_stdout():
    report = {
        "schema": REPORT_SCHEMA,
        "profile": PROFILE_ID,
        "bundle_integrity": "pass",
        "verdict": "valid",
        "claims": EXPECTED_CLAIMS,
        "findings": [],
        "non_claims": NON_CLAIMS,
        "profile_selection": "v0",
    }
    return json.dumps(report, sort_keys=True)


def _error_stdout():
    report = {
        "schema": REPORT_SCHEMA,
        "profile": PROFILE_ID,
        "bundle_integrity": "fail",
        "findings": [{"detail": "tamper"}],
        "non_claims": NON_CLAIMS,
        "reason_code": "E_EVIDENCE_INTEGRITY",
    }
    return json.dumps(report, sort_keys=True)


def _xml_event(pid, address, port, when):
    return (
        "<Event>"
        "<System><EventID>5157</EventID>"
        '<TimeCreated SystemTime="{when}"/>'
        "</System><EventData>"
        '<Data Name="ProcessID">{pid}</Data>'
        '<Data Name="Direction">Outbound</Data>'
        '<Data Name="DestAddress">{address}</Data>'
        '<Data Name="DestPort">{port}</Data>'
        '<Data Name="Protocol">6</Data>'
        '<Data Name="FilterRTID">110398</Data>'
        "</EventData></Event>"
    ).format(pid=pid, address=address, port=port, when=when)


def _denied_leg(target, port, start, end):
    return {
        "protocol": "tcp",
        "target": target,
        "port": port,
        "result": "failed",
        "winerror": 10013,
        "start": start,
        "end": end,
    }


def _connected_leg(protocol, target, port, start, end):
    return {
        "protocol": protocol,
        "target": target,
        "port": port,
        "result": "connected",
        "winerror": None,
        "start": start,
        "end": end,
    }


def pass_receipt():
    start = "2026-09-24T12:00:00+00:00"
    end = "2026-09-24T12:00:01+00:00"
    when = "2026-09-24T12:00:00.5000000Z"
    address = "140.82.121.4"
    loop = "127.0.0.1"
    sid = "S-1-15-2-999"
    stdout = _success_stdout()
    c0_loop = parse_event_xml(_xml_event("0x1068", loop, 9, when))
    c0_ext = parse_event_xml(_xml_event("0x1068", address, 443, when))
    g_ext = parse_event_xml(_xml_event("0x1069", address, 443, when))
    token = {
        "is_app_container": True,
        "sid": sid,
        "capabilities": [],
        "pid": 4200,
    }
    return {
        "profile": {"sid": sid, "created_once": True},
        "resolved_external": {"address": address, "port": 443},
        "h_before": {
            "legs": {
                "tcp_loopback": _connected_leg("tcp", loop, 9, start, end),
                "tcp_external": _connected_leg("tcp", address, 443, start, end),
            }
        },
        "h_after": {
            "legs": {
                "tcp_loopback": _connected_leg("tcp", loop, 9, start, end),
                "tcp_external": _connected_leg("tcp", address, 443, start, end),
            }
        },
        "c1_before": {
            "pid": 4100,
            "token": {
                "is_app_container": True,
                "sid": sid,
                "capabilities": ["internetClient"],
                "pid": 4100,
            },
            "legs": {
                "tcp_external": _connected_leg("tcp", address, 443, start, end),
            },
        },
        "c1_after": {
            "pid": 4102,
            "token": {
                "is_app_container": True,
                "sid": sid,
                "capabilities": ["internetClient"],
                "pid": 4102,
            },
            "legs": {
                "tcp_external": _connected_leg("tcp", address, 443, start, end),
            },
        },
        "c0": {
            "pid": 4200,
            "token": token,
            "events": [c0_loop, c0_ext],
            "legs": {
                "tcp_loopback": _denied_leg(loop, 9, start, end),
                "tcp_external": _denied_leg(address, 443, start, end),
                "udp_loopback": {
                    "protocol": "udp",
                    "target": loop,
                    "port": 9,
                    "result": "not_measurable",
                    "winerror": None,
                    "start": start,
                    "end": end,
                },
                "udp_dns": {
                    "protocol": "udp",
                    "target": "1.1.1.1",
                    "port": 53,
                    "result": "not_measurable",
                    "winerror": None,
                    "start": start,
                    "end": end,
                },
            },
            "grandchild": {
                "spawn": "inherited",
                "pid": 4201,
                "token": {
                    "is_app_container": True,
                    "sid": sid,
                    "capabilities": [],
                    "pid": 4201,
                },
                "events": [g_ext],
                "legs": {"tcp_external": _denied_leg(address, 443, start, end)},
            },
        },
        "name_resolution": {
            "result": "not_measurable",
            "external_query_proven": False,
        },
        "verify": {
            "preflight": {
                "verified": True,
                "connected": True,
                "offline_arm": False,
                "archive_digest_ok": True,
                "identity": CERTIFICATE_IDENTITY,
                "issuer": CERTIFICATE_OIDC_ISSUER,
            },
            "outside": {"exit": 0, "stdout": stdout, "truncated": False},
            "inside": {"exit": 0, "stdout": stdout, "truncated": False},
        },
        "cleanup": {"status": "clean"},
        "wfp_filters": {
            "text": "<filters><item><filterId>110398</filterId></item></filters>",
            "truncated": False,
            "failed": False,
        },
        "event_capture": {"truncated": False, "failed": False},
    }


def _copy(mutator):
    receipt = json.loads(json.dumps(pass_receipt()))
    mutator(receipt)
    return receipt


def _cases():
    def c1_failed(receipt):
        receipt["c1_before"]["legs"]["tcp_external"]["result"] = "failed"

    def c0_connected(receipt):
        receipt["c0"]["legs"]["tcp_external"]["result"] = "connected"

    def no_5157(receipt):
        receipt["c0"]["events"] = []

    def wrong_pid(receipt):
        receipt["c0"]["events"][1]["pid"] = 1

    def outside(receipt):
        receipt["c0"]["events"][1]["time"] = "2026-09-24T12:00:31+00:00"

    def caps(receipt):
        receipt["c1_before"]["token"]["capabilities"] = ["privateNetworkClientServer"]

    def grandchild_token(receipt):
        receipt["c0"]["grandchild"]["token"]["is_app_container"] = False

    def constructed(receipt):
        receipt["c0"]["grandchild"]["spawn"] = "constructed"

    def grandchild_connected(receipt):
        receipt["c0"]["grandchild"]["legs"]["tcp_external"]["result"] = "connected"

    def empty(receipt):
        receipt["verify"]["outside"]["stdout"] = ""
        receipt["verify"]["inside"]["stdout"] = ""

    def error(receipt):
        body = _error_stdout()
        receipt["verify"]["outside"] = {"exit": 2, "stdout": body, "truncated": False}
        receipt["verify"]["inside"] = {"exit": 2, "stdout": body, "truncated": False}

    def mismatch(receipt):
        receipt["verify"]["inside"]["stdout"] += " "

    def unknown(receipt):
        receipt["cleanup"] = {"status": "unknown"}

    def dirty(receipt):
        receipt["cleanup"] = {"status": "dirty"}

    def udp(receipt):
        receipt["c0"]["legs"]["udp_dns"]["result"] = "connected"

    def names(receipt):
        receipt["name_resolution"] = {
            "result": "observed",
            "external_query_proven": True,
        }

    def signature(receipt):
        receipt["verify"]["preflight"]["verified"] = False

    def profile(receipt):
        receipt["profile"]["created_once"] = False

    def endpoint(receipt):
        receipt["c0"]["legs"]["tcp_external"]["target"] = "203.0.113.5"

    def h_before(receipt):
        receipt["h_before"]["legs"]["tcp_loopback"]["result"] = "failed"

    restricted = {
        "verdict": "PASS",
        "completed": True,
        "five_platform_acceptance": False,
        "unqualified_offline": False,
        "dns_external_query_proven": False,
    }
    inconclusive = {"verdict": "INCONCLUSIVE", "completed": False}
    return [
        ("c1_external_failed", c1_failed, dict(inconclusive)),
        ("c0_connected", c0_connected, {"verdict": "MECHANISM_FAILS", "completed": False}),
        ("c0_no_5157", no_5157, dict(inconclusive)),
        ("event_wrong_pid", wrong_pid, dict(inconclusive)),
        ("event_outside_window", outside, dict(inconclusive)),
        ("wrong_capabilities", caps, dict(inconclusive)),
        ("grandchild_not_container", grandchild_token, dict(inconclusive)),
        ("grandchild_constructed", constructed, dict(inconclusive)),
        ("grandchild_connected", grandchild_connected, {"verdict": "MECHANISM_FAILS", "completed": False}),
        ("equal_empty_report", empty, dict(inconclusive)),
        ("equal_error_report", error, dict(inconclusive)),
        ("byte_mismatch", mismatch, dict(inconclusive)),
        ("cleanup_unknown", unknown, {"verdict": "PASS", "completed": False, "cleanup": "unknown"}),
        ("cleanup_dirty", dirty, {"verdict": "PASS", "completed": False, "cleanup": "dirty"}),
        ("optional_udp_success", udp, dict(restricted, claim_contains="udp_dns observed succeeding")),
        (
            "name_resolution_observed",
            names,
            dict(
                restricted,
                claim_contains="not proof of an external DNS query",
            ),
        ),
        ("signature_absent", signature, {"verdict": "SETUP", "completed": False}),
        ("profile_not_once", profile, {"verdict": "SETUP", "completed": False}),
        ("endpoint_changed", endpoint, dict(inconclusive)),
        ("h_before_failed", h_before, dict(inconclusive)),
    ]


def _matches(result, expect):
    for key, value in expect.items():
        if key == "claim_contains":
            if value not in result.get("claim", ""):
                return False
        elif result.get(key) != value:
            return False
    return True


def _workflow_contract():
    workflow = (ROOT / ".github/workflows/experiment-windows-appcontainer.yml").read_text(
        encoding="utf-8"
    )
    release = (ROOT / ".github/workflows/release.yml").read_text(encoding="utf-8")
    problems = []
    if "cursor/windows-offline-feasibility" not in workflow:
        problems.append("branch filter")
    if "experiment/windows-appcontainer-feasibility" in workflow:
        problems.append("old branch filter")
    block = workflow.split("permissions:", 1)[1].split("jobs:", 1)[0]
    if "actions:" in block or "contents: read" not in block:
        problems.append("permissions")
    if "continue-on-error" in workflow:
        problems.append("continue-on-error")
    marker = "sigstore/cosign-installer@"
    if marker not in release or marker not in workflow:
        problems.append("cosign installer pin")
    else:
        release_pin = release[release.index(marker) : release.index(marker) + len(marker) + 40]
        workflow_pin = workflow[workflow.index(marker) : workflow.index(marker) + len(marker) + 40]
        if release_pin != workflow_pin:
            problems.append("cosign installer drift")
    if "cosign-release: " + COSIGN_RELEASE not in release:
        problems.append("cosign release drift")
    if "if-no-files-found: error" not in workflow:
        problems.append("artifact failure mode")
    tree = ast.parse(Path(__file__).read_text(encoding="utf-8"))
    found = [
        node.value.value
        for node in tree.body
        if isinstance(node, ast.Assign)
        for target in node.targets
        if isinstance(target, ast.Name) and target.id == "COSIGN_RELEASE"
    ]
    if found != [COSIGN_RELEASE]:
        problems.append("cosign literal")
    if "@refs/heads/" in CERTIFICATE_IDENTITY:
        problems.append("branch identity")
    if not CERTIFICATE_IDENTITY.endswith("@refs/tags/" + RELEASE_TAG):
        problems.append("tag identity")
    if CERTIFICATE_OIDC_ISSUER != "https://token.actions.githubusercontent.com":
        problems.append("issuer")
    if not BUNDLE_PATH.is_file():
        problems.append("bundle missing")
    return problems


def _helper_problems():
    problems = []
    if canonical_address("::ffff:140.82.121.4") != "140.82.121.4":
        problems.append("address")
    if parse_pid("0x1068") != 4200:
        problems.append("pid")
    if capability_names([INTERNET_CLIENT_SID], INTERNET_CLIENT_SID) != ["internetClient"]:
        problems.append("capability name")
    if capability_names(["S-1-15-3-9"], INTERNET_CLIENT_SID) == ["internetClient"]:
        problems.append("capability relabel")
    if not valid_sid("S-1-15-2-999") or valid_sid("Everyone"):
        problems.append("sid")
    env = child_environment(
        {"PATH": "C:\\Windows", "GH_TOKEN": "x", "GITHUB_TOKEN": "y", "SECRET": "z"}
    )
    if "PATH" not in env or "GH_TOKEN" in env or "GITHUB_TOKEN" in env or "SECRET" in env:
        problems.append("environment")
    body = "abc\n" + ("a" * 64) + "  " + ARCHIVE_NAME + "\n"
    if checksum_digest(body, ARCHIVE_NAME) != "a" * 64:
        problems.append("checksum")
    if checksum_digest(body + ("b" * 64) + "  " + ARCHIVE_NAME + "\n", ARCHIVE_NAME):
        problems.append("duplicate checksum")
    if not cosign_version_ok("cosign version " + COSIGN_RELEASE):
        problems.append("cosign text")
    if safe_zip_member("../assay.exe") or not safe_zip_member("assay.exe"):
        problems.append("zip member")
    start = "2026-09-24T12:00:00+00:00"
    end = "2026-09-24T12:00:01+00:00"
    leg = _denied_leg("140.82.121.4", 443, start, end)
    near = dict(parse_event_xml(_xml_event("0x10", "140.82.121.4", 443, "2026-09-24T12:00:03Z")))
    near["pid"] = 16
    far = dict(near)
    far["time"] = "2026-09-24T12:00:04+00:00"
    if not event_matches(near, leg, 16) or event_matches(far, leg, 16):
        problems.append("window slack")
    namespaced = (
        '<Event xmlns="http://schemas.microsoft.com/win/2004/08/events/event">'
        "<System><EventID>5157</EventID>"
        '<TimeCreated SystemTime="2026-09-24T12:00:00.5000000Z"/>'
        "</System><EventData>"
        '<Data Name="ProcessID">0x10</Data>'
        '<Data Name="DestAddress">140.82.121.4</Data>'
        '<Data Name="DestPort">443</Data>'
        '<Data Name="Protocol">6</Data>'
        "</EventData></Event>"
    )
    parsed = parse_event_xml(namespaced)
    if not parsed or parsed["direction"] is not None or parsed["protocol"] != "tcp":
        problems.append("namespaced event")
    return problems


def _repair_gaps():
    gaps = []
    start = "2026-09-24T12:00:00+00:00"
    end = "2026-09-24T12:00:01+00:00"
    when = "2026-09-24T12:00:00.5000000Z"
    leg = _denied_leg("140.82.121.4", 443, start, end)
    missing = _xml_event("0x10", "140.82.121.4", 443, when).replace(
        '<Data Name="Direction">Outbound</Data>', ""
    )
    parsed_missing = parse_event_xml(missing)
    if parsed_missing and event_matches(parsed_missing, leg, parsed_missing["pid"]):
        gaps.append("missing_direction")
    token_xml = _xml_event("0x10", "140.82.121.4", 443, when).replace(
        '<Data Name="Direction">Outbound</Data>',
        '<Data Name="Direction">%%14592</Data>',
    )
    parsed_token = parse_event_xml(token_xml)
    if parsed_token and event_matches(parsed_token, leg, parsed_token["pid"]):
        gaps.append("unmapped_direction_token")
    parsed_proof = parse_event_xml(_xml_event("0x10", "140.82.121.4", 443, when))
    if not parsed_proof or "raw_xml" not in parsed_proof or not parsed_proof.get("filter_runtime_id"):
        gaps.append("retention_fields_missing")
    bare = pass_receipt()
    bare.pop("wfp_filters", None)
    bare.pop("event_capture", None)
    for event in bare["c0"]["events"] + bare["c0"]["grandchild"]["events"]:
        event.pop("raw_xml", None)
        event.pop("filter_runtime_id", None)
    if evaluate(bare)["completed"]:
        gaps.append("evidence_absent_completed")
    truncated = pass_receipt()
    truncated["wfp_filters"] = {"text": "filters", "truncated": True, "failed": False}
    if evaluate(truncated)["completed"]:
        gaps.append("truncated_capture_completed")
    unrelated = records_for_leg(
        [
            {"id": 5157, "pid": 16, "direction": "outbound"},
            {"id": 5157, "pid": 99, "direction": "outbound"},
        ],
        leg,
        16,
    )
    if any(item.get("pid") == 99 for item in unrelated):
        gaps.append("unrelated_event_retained")
    held = cleanup(
        {"acquired": [{"kind": "job", "open": True}, {"kind": "app_sid", "open": True}]}
    )
    if held["steps"]["job_closed"] is not False or "app_sid" not in held.get("open", []):
        gaps.append("hardcoded_job_closed")
    items = [
        {"kind": "app_sid", "open": True},
        {"kind": "attribute_list", "open": True},
        {"kind": "stdout_read", "open": True},
    ]

    def close(item):
        return item["kind"] != "attribute_list"

    partial = release_acquired(items, close)
    if partial["open"] != ["attribute_list"]:
        gaps.append("partial_release")
    dump = (
        "<filters><item><filterId>110398</filterId><name>probe</name></item>"
        "<item><filterId>999999</filterId><name>other-host</name></item></filters>"
    )
    selected = select_filter_evidence(dump, ["110398"])
    if (
        selected.get("failed")
        or selected.get("truncated")
        or "999999" in selected.get("text", "")
        or "110398" not in selected.get("text", "")
    ):
        gaps.append("unrelated_filter_retained")
    absent = select_filter_evidence(dump, ["110398", "42"])
    if not absent.get("failed") or absent.get("text"):
        gaps.append("missing_filter_kept")
    return gaps


def _closer_gaps():
    """Call the real launch closer with a fake DLL. No Win32 is loaded."""
    gaps = []
    sentinel = object()
    sid_keep = object()

    class Api:
        def __init__(self, impl):
            self.impl = impl
            self.restype = "unset"
            self.argtypes = "unset"
            self.calls = []

        def __call__(self, value):
            self.calls.append(value)
            self.seen_restype = self.restype
            self.seen_argtypes = self.argtypes
            return self.impl(value)

    delete = Api(lambda _value: None)
    close = Api(lambda _value: 1)
    free = Api(lambda _value: None)
    kernel = type("Kernel", (), {})()
    kernel.DeleteProcThreadAttributeList = delete
    kernel.CloseHandle = close
    advapi = type("Adv", (), {})()
    advapi.FreeSid = free

    def load():
        return (None, None, kernel, advapi, None, None)

    global _load_win32
    original = _load_win32
    _load_win32 = load
    try:
        outcome = _close_launch_item(
            {"kind": "attribute_list", "open": True, "value": sentinel}
        )
        if (
            not isinstance(outcome, dict)
            or outcome.get("invoked") is not True
            or outcome.get("verified_absent") is not None
        ):
            gaps.append("void_return_read_as_bool")
        if getattr(delete, "seen_restype", "unset") is not None or delete.calls != [sentinel]:
            gaps.append("void_restype")
        if not (isinstance(getattr(delete, "seen_argtypes", None), tuple) and len(delete.argtypes) == 1):
            gaps.append("void_argtypes")
        fresh = {"kind": "attribute_list", "open": True, "value": sentinel}
        receipt = cleanup({"acquired": [fresh], "closer": _close_launch_item})
        rows = [
            row
            for row in receipt.get("releases") or []
            if row.get("kind") == "attribute_list"
        ]
        if (
            fresh.get("open") is not False
            or "attribute_list" in receipt.get("open", [])
            or len(rows) != 1
            or rows[0].get("invoked") is not True
            or rows[0].get("verified_absent") is not None
        ):
            gaps.append("cleanup_void_receipt")

        class Boom(Api):
            def __call__(self, value):
                raise OSError("delete failed")

        kernel.DeleteProcThreadAttributeList = Boom(lambda _value: None)
        exploded = {"kind": "attribute_list", "open": True, "value": sentinel}
        release_acquired([exploded], _close_launch_item)
        release = exploded.get("release") or {}
        if (
            exploded.get("open") is not True
            or release.get("invoked") is not False
            or release.get("verified_absent") is True
        ):
            gaps.append("void_exception_closed")
        kernel.DeleteProcThreadAttributeList = delete
        sid_out = _close_launch_item({"kind": "app_sid", "open": True, "value": sentinel})
        if not (isinstance(sid_out, dict) and sid_out.get("verified_absent") is True):
            gaps.append("freesid_null")
        free.impl = lambda _value: sid_keep
        sid_bad = _close_launch_item({"kind": "app_sid", "open": True, "value": sentinel})
        if not (isinstance(sid_bad, dict) and sid_bad.get("verified_absent") is False):
            gaps.append("freesid_pointer")
        close.impl = lambda _value: 0
        handle_bad = _close_launch_item({"kind": "job", "open": True, "value": sentinel})
        if not (isinstance(handle_bad, dict) and handle_bad.get("verified_absent") is False):
            gaps.append("closehandle_zero")
        close.impl = lambda _value: 1
        handle_out = _close_launch_item({"kind": "job", "open": True, "value": sentinel})
        if not (isinstance(handle_out, dict) and handle_out.get("verified_absent") is True):
            gaps.append("closehandle_nonzero")
    finally:
        _load_win32 = original
    if _LOADED_WIN32:
        gaps.append("loaded_win32")
    return gaps


def _documented_5157(direction_value, record_id="4412"):
    return (
        '<Event xmlns="http://schemas.microsoft.com/win/2004/08/events/event">'
        "<System>"
        '<Provider Name="Microsoft-Windows-Security-Auditing" '
        'Guid="{54849625-5478-4994-A5BA-3E3B0328C30D}"/>'
        "<EventID>5157</EventID>"
        "<EventRecordID>" + record_id + "</EventRecordID>"
        '<TimeCreated SystemTime="2026-09-24T12:00:00.5000000Z"/>'
        "</System><EventData>"
        '<Data Name="ProcessID">0x10</Data>'
        '<Data Name="Direction">' + direction_value + "</Data>"
        '<Data Name="DestAddress">140.82.121.4</Data>'
        '<Data Name="DestPort">443</Data>'
        '<Data Name="Protocol">6</Data>'
        '<Data Name="FilterRTID">110398</Data>'
        "</EventData></Event>"
    )


def _producer_gaps():
    """Exercise collect_events and collect_filter_evidence with injected producers."""
    import tempfile

    gaps = []
    leg = _denied_leg(
        "140.82.121.4",
        443,
        "2026-09-24T12:00:00+00:00",
        "2026-09-24T12:00:01+00:00",
    )
    token_xml = _documented_5157("%%14592")

    def producer_for(xml):
        def producer(_script, _env, _timeout):
            return 0, "---EVENT---\n" + xml + "\n", False

        return producer

    def render_outbound(provider, record_id, message_id, buffer_chars):
        if (
            provider == "Microsoft-Windows-Security-Auditing"
            and str(record_id) == "4412"
            and message_id == 14592
            and buffer_chars == DIRECTION_RENDER_CHARS
        ):
            return "Outbound"
        return None

    found, truncated, failed = collect_events(
        leg, 16, producer=producer_for(token_xml), renderer=render_outbound
    )
    if (
        failed
        or truncated
        or len(found) != 1
        or found[0].get("direction") != "outbound"
        or "%%14592" not in found[0].get("raw_xml", "")
        or found[0].get("provider") != "Microsoft-Windows-Security-Auditing"
        or str(found[0].get("record_id")) != "4412"
        or found[0].get("render_message_id") != 14592
    ):
        gaps.append("documented_token_not_rendered")
    unknown, _unknown_truncated, _unknown_failed = collect_events(
        leg,
        16,
        producer=producer_for(token_xml),
        renderer=lambda *_args: "The Windows Filtering Platform blocked an outbound packet",
    )
    if (
        len(unknown) != 1
        or unknown[0].get("direction") is not None
        or "%%14592" not in unknown[0].get("raw_xml", "")
        or event_matches(unknown[0], leg, 16)
    ):
        gaps.append("unknown_direction_not_retained")
    noop, _noop_truncated, _noop_failed = collect_events(
        leg, 16, producer=producer_for(token_xml), renderer=lambda *_args: None
    )
    if (
        len(noop) != 1
        or noop[0].get("direction") is not None
        or "%%14592" not in noop[0].get("raw_xml", "")
    ):
        gaps.append("noop_renderer_dropped")

    def refuse_render(*_args):
        raise AssertionError("literal direction must not be rendered")

    literal, _literal_truncated, literal_failed = collect_events(
        leg,
        16,
        producer=producer_for(_documented_5157("Outbound")),
        renderer=refuse_render,
    )
    if literal_failed or len(literal) != 1 or literal[0].get("direction") != "outbound":
        gaps.append("literal_direction_needs_renderer")
    dishonest = pass_receipt()
    for event in dishonest["c0"]["events"] + dishonest["c0"]["grandchild"]["events"]:
        event["direction"] = "outbound"
        event["raw_direction"] = "%%14592"
        event["raw_xml"] = event["raw_xml"].replace("Outbound", "%%14592")
    if evaluate(dishonest)["completed"]:
        gaps.append("token_fixture_completed")
    cited = "<item><filterId>110398</filterId><name>probe</name></item>"
    other = "<item><filterId>999999</filterId><name>" + ("h" * 70000) + "</name></item>"
    inventory = "<filters>" + cited + other + "</filters>"
    if len(inventory.encode("utf-8")) <= FILTER_SELECTED_BYTES:
        gaps.append("inventory_fixture_too_small")
    expected_argv = ["netsh", "wfp", "show", "filters", "file=-", "verbose=on"]
    cited_bytes = cited.encode("utf-8")
    small_xml = b"<filters>" + cited_bytes + b"</filters>"
    calls = []

    def shown(payload, **over):
        captured = {
            "exit": 0,
            "stdout": "",
            "stderr": "",
            "stdout_bytes": payload,
            "truncated": False,
            "late": False,
            "cleanup": "clean",
            "accepted": True,
        }
        captured.update(over)
        if captured.get("truncated") or captured.get("late") or captured.get("exit") != 0:
            captured["accepted"] = False
        if "accepted" in over:
            captured["accepted"] = over["accepted"]
        return captured

    def run(destination, payload, trap=None, ids=("110398",), **over):
        def runner(argv, _timeout, env=None):
            del env
            calls.append(list(argv))
            if trap is not None:
                destination.write_bytes(trap)
            return shown(payload, **over)

        return collect_filter_evidence(destination, list(ids), runner=runner)

    def leaves_incomplete(result):
        swapped = pass_receipt()
        swapped["wfp_filters"] = result
        verdict = evaluate(swapped)
        return (
            not verdict["completed"]
            and "wfp_filters_incomplete" in verdict["reasons"]
        )

    with tempfile.TemporaryDirectory(prefix="assay-wfp-") as temporary:
        root = Path(temporary)
        selected = run(root / "positive.xml", inventory.encode("utf-8"))
        retained = (root / "positive.xml").read_bytes() if (root / "positive.xml").is_file() else b""
        if (
            selected.get("failed")
            or selected.get("truncated")
            or "110398" not in selected.get("text", "")
            or "999999" in selected.get("text", "")
            or len(selected.get("text", "").encode("utf-8")) > FILTER_SELECTED_BYTES
            or b"999999" in retained
            or b"110398" not in retained
            or not leaves_incomplete(selected) is False
        ):
            gaps.append("positive_stdout")
        else:
            swapped = pass_receipt()
            swapped["wfp_filters"] = selected
            if not evaluate(swapped)["completed"] or "wfp_filters_incomplete" in evaluate(swapped)["reasons"]:
                gaps.append("positive_stdout")
        prefix = cited_bytes + b"z" * (FILTER_ACQUIRE_BYTES - len(cited_bytes))
        overflow = prefix + b"MORE"
        if cited_bytes not in overflow[:FILTER_ACQUIRE_BYTES] or len(overflow) <= FILTER_ACQUIRE_BYTES:
            gaps.append("cited_before_overflow")
        blocked = run(root / "overflow.xml", overflow, trap=small_xml, truncated=True)
        if (
            blocked.get("truncated") is not True
            or blocked.get("failed")
            or blocked.get("text")
            or "110398" in (blocked.get("text") or "")
            or not leaves_incomplete(blocked)
        ):
            gaps.append("cited_before_overflow")
        missing = run(
            root / "missing.xml",
            b"<filters><item><filterId>999999</filterId></item></filters>",
            trap=small_xml,
        )
        if missing.get("text") or missing.get("truncated") or missing.get("failed") is not True or not leaves_incomplete(missing):
            gaps.append("missing_id")
        chatter = run(root / "chatter.xml", b"\nOk.\n" + small_xml, trap=small_xml)
        if chatter.get("text") or chatter.get("failed") is not True or not leaves_incomplete(chatter):
            gaps.append("stdout_chatter")
        malformed = run(root / "malformed.xml", b"<filters><item><filterId>110398</filterId>", trap=small_xml)
        if malformed.get("text") or malformed.get("failed") is not True or not leaves_incomplete(malformed):
            gaps.append("malformed_stdout")
        encoded = run(root / "encoded.xml", b"\xff" + small_xml, trap=small_xml)
        if encoded.get("text") or encoded.get("truncated") or encoded.get("failed") is not True or not leaves_incomplete(encoded):
            gaps.append("stdout_encoding")
        utf16 = b"\xff\xfe" + small_xml.decode("utf-8").encode("utf-16-le")
        wide = run(root / "utf16.xml", utf16)
        if (
            wide.get("failed")
            or wide.get("truncated")
            or "110398" not in wide.get("text", "")
            or "999999" in wide.get("text", "")
        ):
            gaps.append("utf16_stdout")

        def with_decl(name, bom=None):
            document = (
                '<?xml version="1.0" encoding="'
                + name
                + '"?><filters><item><filterId>110398</filterId></item></filters>'
            )
            if bom == "le":
                return b"\xff\xfe" + document.encode("utf-16-le")
            if bom == "utf8":
                return b"\xef\xbb\xbf" + document.encode("utf-8")
            return document.encode("utf-8")

        for label, payload in (
            ("decl_utf8", with_decl("UTF-8")),
            ("decl_utf8_lower", with_decl("utf-8")),
            ("decl_utf8_bom", with_decl("UTF-8", "utf8")),
            ("decl_utf16", with_decl("UTF-16", "le")),
        ):
            kept = run(root / (label + ".xml"), payload)
            if kept.get("failed") or kept.get("truncated") or "110398" not in kept.get("text", ""):
                gaps.append(label)
        for label, payload in (
            ("decl_mismatch_utf16", with_decl("UTF-16")),
            ("decl_mismatch_utf16le", with_decl("UTF-16LE")),
            ("decl_mismatch_utf32", with_decl("UTF-32")),
            ("decl_mismatch_latin1", with_decl("ISO-8859-1")),
            ("decl_mismatch_utf8_on_utf16", with_decl("UTF-8", "le")),
        ):
            rejected = run(root / (label + ".xml"), payload, trap=small_xml)
            retained_decl = (root / (label + ".xml")).read_bytes()
            if (
                rejected.get("text")
                or rejected.get("truncated")
                or rejected.get("failed") is not True
                or not leaves_incomplete(rejected)
                or b"110398" in retained_decl
            ):
                gaps.append(label)
        preseed = b"TRAP-110398"
        double_bom = b"\xef\xbb\xbf\xef\xbb\xbf" + with_decl("UTF-16")
        extra_feff = b"\xff\xfe" + "\ufeff".encode("utf-16-le") + with_decl("UTF-8", "le")[2:]
        for label, payload in (
            ("double_bom_mismatch", double_bom),
            ("utf16_extra_feff_mismatch", extra_feff),
        ):
            rejected = run(root / (label + ".xml"), payload, trap=preseed)
            retained_decl = (root / (label + ".xml")).read_bytes()
            if (
                rejected.get("text")
                or rejected.get("truncated")
                or rejected.get("failed") is not True
                or not leaves_incomplete(rejected)
                or preseed in retained_decl
                or b"110398" in retained_decl
            ):
                gaps.append(label)
        noisy = run(root / "exit.xml", small_xml, trap=small_xml, exit=3)
        if noisy.get("text") or noisy.get("truncated") or noisy.get("failed") is not True or not leaves_incomplete(noisy):
            gaps.append("nonzero_exit")
        timed_out = run(root / "timeout.xml", small_xml, trap=small_xml, late=True)
        if timed_out.get("text") or timed_out.get("failed") is not True or not leaves_incomplete(timed_out):
            gaps.append("timeout")
        # The bytes already contain the cited id. EOF still has not arrived.
        unread = run(root / "eof.xml", small_xml, trap=small_xml, late=True, accepted=False)
        if unread.get("text") or unread.get("truncated") or unread.get("failed") is not True or not leaves_incomplete(unread):
            gaps.append("missing_eof")
        if any(call != expected_argv for call in calls):
            gaps.append("file_export_argv")
        import inspect

        source = inspect.getsource(collect_filter_evidence)
        signature = inspect.signature(collect_filter_evidence)
        if (
            "file=-" not in source
            or "verbose=on" not in source
            or "supervise_owned" not in source
            or "FILTER_ACQUIRE_BYTES" not in source
            or "COMMAND_STDOUT_BYTES" in source
            or "+ str(destination)" in source
            or "spawn" not in signature.parameters
        ):
            gaps.append("file_export_argv")
        if "spawn" in signature.parameters and "deadline" in signature.parameters:
            import time

            class FilterPipe:
                owned = True

                def __init__(self, stdout, hold_exit=False, block_eof=False):
                    self._out = stdout
                    self.hold_exit = hold_exit
                    self.block_eof = block_eof
                    self.terminated = False

                def read_stdout(self, n):
                    if self._out:
                        data = self._out[:n]
                        self._out = self._out[n:]
                        return data
                    if self.block_eof and not self.terminated:
                        time.sleep(30)
                        return b"x"
                    return b""

                def read_stderr(self, _n):
                    return b""

                def poll(self):
                    if self.terminated:
                        return 0
                    if self.hold_exit or (self.block_eof and self._out == b""):
                        return None
                    if self._out == b"":
                        return 0
                    return None

                def terminate(self):
                    self.terminated = True

                def reap(self, _timeout):
                    if self.terminated or (self._out == b"" and not self.hold_exit and not self.block_eof):
                        return 0
                    return None

                def cleanup(self):
                    return True

            def through_supervisor(destination, pipe, limit_ids=("110398",)):
                seen = []

                def spawn(argv, _env):
                    seen.append(list(argv))
                    return pipe

                result = collect_filter_evidence(
                    destination, list(limit_ids), spawn=spawn, deadline=0.25
                )
                return result, seen

            supervised, seen_argv = through_supervisor(root / "supervised.xml", FilterPipe(small_xml))
            if (
                seen_argv != [expected_argv]
                or supervised.get("failed")
                or "110398" not in supervised.get("text", "")
            ):
                gaps.append("supervisor_positive")
            over_pipe, _seen = through_supervisor(
                root / "supervised-over.xml",
                FilterPipe(cited_bytes + b"z" * FILTER_ACQUIRE_BYTES),
            )
            if over_pipe.get("truncated") is not True or over_pipe.get("text") or not leaves_incomplete(over_pipe):
                gaps.append("supervisor_overflow")
            late_pipe, _seen = through_supervisor(
                root / "supervised-late.xml", FilterPipe(small_xml, hold_exit=True)
            )
            if late_pipe.get("text") or late_pipe.get("failed") is not True or not leaves_incomplete(late_pipe):
                gaps.append("supervisor_timeout")
            eof_pipe, _seen = through_supervisor(
                root / "supervised-eof.xml",
                FilterPipe(small_xml, block_eof=True),
            )
            if eof_pipe.get("text") or eof_pipe.get("failed") is not True or not leaves_incomplete(eof_pipe):
                gaps.append("supervisor_missing_eof")
    class Response:
        def read(self, _size=-1):
            return b"x" * (RELEASE_DOWNLOAD_BYTES + 8)

    body, over = read_bounded_http(Response(), RELEASE_DOWNLOAD_BYTES)
    if over is not True or body:
        gaps.append("unbounded_response_read")
    return gaps


def _capture_gaps():
    """Injected pipes and local archives. No host child, network, or downloaded binary."""
    import io
    import tempfile
    import threading
    import time
    import zipfile

    gaps = []

    class Pipe:
        def __init__(self, stdout=b"", stderr=b"", cleanup=True, owned=True, hold_exit=False, delay_eof=False, sync=False):
            self.owned = owned
            self._out = stdout
            self._err = stderr
            self._cleanup = cleanup
            self.hold_exit = hold_exit
            self.delay_eof = delay_eof
            self.sync = sync
            self._delayed = False
            self.stdout_requests = []
            self.stderr_requests = []
            self.stdout_given = 0
            self.stderr_given = 0
            self.out_eof = False
            self.err_eof = False
            self.terminated = False
            self.terminate_calls = 0
            self.cleanup_calls = 0
            self.out_entered = threading.Event()
            self.err_entered = threading.Event()
            self.overlap = False
            self.serial = False

        def _sync(self, which):
            if not self.sync:
                return
            if which == "out":
                self.out_entered.set()
                if self.err_entered.wait(0.4):
                    self.overlap = True
                else:
                    self.serial = True
            else:
                self.err_entered.set()
                if self.out_entered.wait(0.4):
                    self.overlap = True
                else:
                    self.serial = True

        def read_stdout(self, n):
            self.stdout_requests.append(n)
            self._sync("out")
            if self.delay_eof:
                if not self._delayed:
                    self._delayed = True
                    return self._out
                time.sleep(0.25)
                self.out_eof = True
                self._out = b""
                return b""
            return self._take("out", n)

        def read_stderr(self, n):
            self.stderr_requests.append(n)
            self._sync("err")
            return self._take("err", n)

        def _take(self, which, n):
            buf = self._out if which == "out" else self._err
            if not isinstance(n, int) or n < 1 or n > READ_CHUNK_BYTES:
                data = buf
                rest = b""
            else:
                data = buf[:n]
                rest = buf[n:]
            if which == "out":
                self._out = rest
                self.stdout_given += len(data)
                self.out_eof = rest == b""
            else:
                self._err = rest
                self.stderr_given += len(data)
                self.err_eof = rest == b""
            return data

        def poll(self):
            if self.terminated:
                return 0
            if self.hold_exit:
                return None
            if self.out_eof and self.err_eof:
                return 0
            return None

        def terminate(self):
            self.terminate_calls += 1
            self.terminated = True

        def reap(self, _timeout):
            if self.terminated or (self.out_eof and self.err_eof and not self.hold_exit):
                return 0
            return None

        def cleanup(self):
            self.cleanup_calls += 1
            if self._cleanup == "raise":
                raise OSError("cleanup unknown")
            if self._cleanup == "hang":
                time.sleep(30)
            return True if self._cleanup == "hang" else self._cleanup

    def bounded_reads(child, limit):
        requests = child.stdout_requests + child.stderr_requests
        return all(isinstance(n, int) and 1 <= n <= READ_CHUNK_BYTES for n in requests) and child.stdout_given <= limit + 1 and child.stderr_given <= limit + 1

    over = Pipe(b"A" * 32 + b"TAILMARKER", b"E" * 16 + b"TAIL")
    over_result = supervise_owned(over, 32, 16, 2)
    if (
        not bounded_reads(over, 32)
        or over.stderr_given > 17
        or over_result["accepted"]
        or over_result["stdout"]
        or over_result["stderr"]
        or not over_result["truncated"]
        or over_result["late"]
        or over.terminate_calls != 1
    ):
        gaps.append("over_cap_stream")
    both = Pipe(b"OUT", b"ERR", sync=True)
    both_result = supervise_owned(both, 100, 100, 2)
    if not both.overlap or both.serial or both_result["stdout"] != "OUT" or both_result["stderr"] != "ERR" or not both_result["accepted"]:
        gaps.append("streams_not_concurrent")
    delayed = Pipe(b"pong", b"", delay_eof=True)
    delayed_result = supervise_owned(delayed, 100, 100, 2)
    if not delayed.out_eof or not delayed_result["accepted"] or delayed_result["stdout"] != "pong" or delayed_result["late"]:
        gaps.append("delayed_eof_missed")
    late = Pipe(b'{"ok":true}', b"", hold_exit=True)
    late_result = supervise_owned(late, 100, 100, 0.15)
    if late_result["accepted"] or late_result["stdout"] or not late_result["late"] or late.terminate_calls != 1:
        gaps.append("late_receipt_accepted")

    class CutoffClock:
        def __init__(self):
            self.t = 0.0

        def monotonic(self):
            return self.t

        def sleep(self, seconds):
            self.t += seconds

    class CutoffChild:
        owned = True

        def __init__(self, clock, span):
            self._out = b"ok"
            self.clock = clock
            self.span = span
            self.terminated = False
            self.terminate_calls = 0

        def read_stdout(self, n):
            data = self._out[:n]
            self._out = self._out[n:]
            return data

        def read_stderr(self, _n):
            return b""

        def poll(self):
            if self.terminated:
                return 0
            if self.clock.t >= self.span:
                return 0
            return None

        def terminate(self):
            self.terminate_calls += 1
            self.terminated = True

        def reap(self, _timeout):
            if self.terminated or self.clock.t >= self.span:
                return 0
            return None

        def cleanup(self):
            return True

    cutoff_clock = CutoffClock()
    cutoff_child = CutoffChild(cutoff_clock, 0.03)
    cutoff = supervise_owned(cutoff_child, 100, 100, 0.03, clock=cutoff_clock)
    if (
        cutoff["accepted"]
        or cutoff["stdout"]
        or cutoff["stderr"]
        or cutoff.get("stdout_bytes")
        or cutoff["exit"] == 0
        or not cutoff["late"]
        or cutoff["cleanup"] != "clean"
    ):
        gaps.append("exit_after_cutoff")
    dirty = Pipe(b"ok", b"", cleanup=False)
    dirty_result = supervise_owned(dirty, 100, 100, 2)
    if dirty_result["accepted"] or dirty_result["stdout"] or dirty_result["cleanup"] != "failed":
        gaps.append("cleanup_failure_accepted")
    unknown = Pipe(b"ok", b"", cleanup="raise")
    unknown_result = supervise_owned(unknown, 100, 100, 2)
    hanging = Pipe(b"ok", b"", cleanup="hang")
    started = time.monotonic()
    hanging_result = supervise_owned(hanging, 100, 100, 2)
    if (
        unknown_result["accepted"]
        or unknown_result["cleanup"] != "unknown"
        or hanging_result["accepted"]
        or hanging_result["cleanup"] != "unknown"
        or time.monotonic() - started > CLEANUP_BUDGET_SECONDS + 1.5
    ):
        gaps.append("cleanup_unknown_accepted")
    unowned = Pipe(b"ok", b"", owned=False)
    unowned_result = supervise_owned(unowned, 100, 100, 2)
    if unowned_result["accepted"] or unowned.terminate_calls or unowned.stdout_requests:
        gaps.append("unowned_accepted")
    ordinary = Pipe(b"hello", b"warn")
    ordinary_result = supervise_owned(ordinary, 100, 100, 2)
    if (
        not ordinary_result["accepted"]
        or ordinary_result["stdout"] != "hello"
        or ordinary_result["stderr"] != "warn"
        or ordinary_result["truncated"]
        or ordinary_result["late"]
        or ordinary_result["cleanup"] != "clean"
        or ordinary.terminate_calls
        or ordinary.cleanup_calls != 1
    ):
        gaps.append("ordinary_failed")
    noop = Pipe(b"", b"")
    noop_result = supervise_owned(noop, 100, 100, 2)
    if (
        not noop_result["accepted"]
        or noop_result["stdout"]
        or noop_result["stderr"]
        or noop_result["truncated"]
        or noop_result["late"]
        or noop.terminate_calls
        or noop.cleanup_calls != 1
    ):
        gaps.append("noop_failed")

    def check_caller(name, limit, invoke):
        child = Pipe(b"A" * (limit + 8), b"")
        invoke(child)
        if not bounded_reads(child, limit) or child.stdout_given > limit + 1:
            gaps.append(name)

    check_caller(
        "command_unbounded_read",
        COMMAND_STDOUT_BYTES,
        lambda child: _run_command(["probe"], 2, env={}, spawn=lambda _argv, _env: child),
    )
    check_caller(
        "powershell_unbounded_read",
        POWERSHELL_STDOUT_BYTES,
        lambda child: _powershell("Get-Date", {}, 2, spawn=lambda _argv, _env: child),
    )
    grandchild = Pipe(b'{"spawn":"inherited","ok":true}' + (b"X" * (GRANDCHILD_CAPTURE_BYTES + 8)), b"")
    captured = _capture_grandchild(["probe"], {}, 2, spawn=lambda _argv, _env: grandchild)
    record = _grandchild_from_capture(captured)
    if not bounded_reads(grandchild, GRANDCHILD_CAPTURE_BYTES) or record.get("parse_error") is not True or record.get("ok") is True:
        gaps.append("grandchild_unbounded_read")

    class Member:
        def __init__(self, name, data, file_size=None, external_attr=0):
            self.filename = name
            self.file_size = len(data) if file_size is None else file_size
            self.external_attr = external_attr
            self.data = data

        def is_dir(self):
            return self.filename.endswith("/")

    class Archive:
        def __init__(self, members):
            self.members = members
            self.read_names = []

        def infolist(self):
            return list(self.members)

        def read(self, info):
            self.read_names.append(info.filename)
            return info.data

        def open(self, info):
            self.read_names.append(info.filename)
            return io.BytesIO(info.data)

    def rejected(archive, limit):
        with tempfile.TemporaryDirectory(prefix="assay-zip-") as temporary:
            dest = Path(temporary) / "out"
            try:
                extract_bounded_archive(archive, dest, limit)
            except SetupError:
                return not dest.exists()
            return False

    declared = Archive([Member("assay.exe", b"MZ", file_size=ARCHIVE_DECODE_BYTES + 1)])
    if not rejected(declared, ARCHIVE_DECODE_BYTES) or declared.read_names:
        gaps.append("zip_declared_size_ignored")
    lied = Archive(
        [
            Member("assay.exe", b"MZ-partial"),
            Member("pad.bin", b"y" * 50, file_size=10),
        ]
    )
    if not rejected(lied, 20):
        gaps.append("zip_content_over_ceiling")
    partial = Archive(
        [
            Member("assay.exe", b"MZ-partial"),
            Member("pad.bin", b"y" * 80, file_size=10),
        ]
    )
    with tempfile.TemporaryDirectory(prefix="assay-zip-") as temporary:
        dest = Path(temporary) / "out"
        try:
            extract_bounded_archive(partial, dest, 30)
        except SetupError:
            if dest.exists() and any(dest.rglob("assay.exe")):
                gaps.append("zip_partial_executable")
        else:
            gaps.append("zip_partial_executable")
    crowded = Archive([Member("assay.exe" if index == 0 else "f" + str(index), b"x") for index in range(ZIP_MEMBER_LIMIT + 1)])
    if not rejected(crowded, ARCHIVE_DECODE_BYTES):
        gaps.append("zip_member_count")
    dupes = Archive([Member("Assay.exe", b"MZ"), Member("assay.exe", b"MZ2")])
    if not rejected(dupes, ARCHIVE_DECODE_BYTES):
        gaps.append("zip_duplicate")
    link = Archive([Member("assay.exe", b"MZ", external_attr=(0o120777) << 16)])
    if not rejected(link, ARCHIVE_DECODE_BYTES):
        gaps.append("zip_link_extracted")
    escaped = Archive([Member("../assay.exe", b"MZ")])
    with tempfile.TemporaryDirectory(prefix="assay-zip-") as temporary:
        root = Path(temporary)
        try:
            extract_bounded_archive(escaped, root / "out", ARCHIVE_DECODE_BYTES)
        except SetupError:
            if (root / "assay.exe").exists():
                gaps.append("zip_path_escaped")
        else:
            gaps.append("zip_path_escaped")
    with tempfile.TemporaryDirectory(prefix="assay-zip-") as temporary:
        root = Path(temporary)
        buffer = io.BytesIO()
        with zipfile.ZipFile(buffer, "w") as archive:
            archive.writestr("readme.txt", b"ok")
            archive.writestr("assay.exe", b"MZ-not-real")
        buffer.seek(0)
        with zipfile.ZipFile(buffer) as archive:
            try:
                assay = extract_bounded_archive(archive, root / "out", ARCHIVE_DECODE_BYTES)
            except SetupError:
                gaps.append("zip_small_rejected")
            else:
                if not assay.is_file() or assay.read_bytes() != b"MZ-not-real" or not (root / "out" / "readme.txt").is_file():
                    gaps.append("zip_small_rejected")
    return gaps


def _pointer_value(value):
    import ctypes

    if value is None:
        return None
    if isinstance(value, int):
        return value
    raw = getattr(value, "value", None)
    if isinstance(raw, int):
        return raw
    return ctypes.cast(value, ctypes.c_void_p).value


def _sid_binding(mode):
    """Synthetic KernelBase binding. No Windows DLL is loaded."""
    import ctypes
    from ctypes import wintypes

    wide = mode in ("wide_false", "wide_success")
    group_sid = 0x100000001 if wide else 0x101
    cap_sid = 0x100000002 if wide else 0x202
    group_items = (ctypes.c_void_p * 1)(ctypes.c_void_p(group_sid))
    cap_items = (ctypes.c_void_p * 1)(ctypes.c_void_p(cap_sid))
    arrays = {
        "group": ctypes.addressof(group_items),
        "cap": ctypes.addressof(cap_items),
        "group_sid": group_sid,
        "cap_sid": cap_sid,
    }
    state = {
        "group_items": group_items,
        "cap_items": cap_items,
        "calls": 0,
        "name": None,
        "restype": "unset",
        "argtypes": "unset",
        "convert_calls": 0,
        "convert_sid": None,
        "convert_pointer": False,
        "text_ptr": None,
        "frees": [],
        "free_was_pointer": [],
        "last_error_reads": [],
        "requested": [],
        "userenv": [],
    }

    def derive(name, group_sids, group_count, cap_sids, cap_count):
        state["calls"] += 1
        state["name"] = name
        state["restype"] = getattr(derive, "restype", "unset")
        state["argtypes"] = getattr(derive, "argtypes", "unset")
        if mode == "empty":
            group_count._obj.value = 0
            cap_count._obj.value = 0
            group_sids._obj.value = arrays["group"]
            cap_sids._obj.value = arrays["cap"]
        else:
            group_count._obj.value = 1
            cap_count._obj.value = 1
            group_sids._obj.value = arrays["group"]
            cap_sids._obj.value = arrays["cap"]
        return 0 if mode in ("false", "wide_false") else 1

    derive.restype = "unset"
    derive.argtypes = "unset"

    class KernelBase:
        def __getattr__(self, name):
            if name == "DeriveCapabilitySidsFromName" and mode != "missing":
                return derive
            raise AttributeError(name)

    class Userenv:
        def __getattr__(self, name):
            state["userenv"].append(name)
            if name == "DeriveCapabilitySidsFromName" and mode != "missing":
                return derive
            raise AttributeError(name)

    class CtypesProxy:
        def __getattr__(self, name):
            return getattr(ctypes, name)

        def WinDLL(self, name, use_last_error=False):
            state["requested"].append(name)
            if name == "KernelBase":
                return KernelBase()
            raise AttributeError(name)

    def convert(sid, out):
        state["convert_calls"] += 1
        argtypes = getattr(convert, "argtypes", None)
        typed = isinstance(argtypes, tuple) and len(argtypes) >= 1 and argtypes[0] is ctypes.c_void_p
        state["convert_pointer"] = (not isinstance(sid, int)) or typed
        state["convert_sid"] = sid if isinstance(sid, int) else _pointer_value(sid)
        if isinstance(sid, int) and sid > 0xFFFFFFFF and not typed:
            raise OverflowError("int too long to convert")
        if mode == "convert_fail":
            return 0
        out._obj.value = "S-1-15-3-9" if mode == "mismatch" else INTERNET_CLIENT_SID
        state["text_ptr"] = ctypes.cast(out._obj, ctypes.c_void_p).value
        return 1

    def local_free(value):
        # Windows x64 long is 32 bits. A bare int above that raises in ctypes.
        if isinstance(value, int):
            if value > 0xFFFFFFFF:
                raise OverflowError("int too long to convert")
            state["frees"].append(value)
            state["free_was_pointer"].append(False)
            return None
        state["frees"].append(_pointer_value(value))
        state["free_was_pointer"].append(True)
        return None

    def get_last_error():
        state["last_error_reads"].append(5)
        return 5

    kernel = type("Kernel", (), {})()
    kernel.LocalFree = local_free
    kernel.GetLastError = get_last_error
    advapi = type("Adv", (), {})()
    advapi.ConvertSidToStringSidW = convert

    def load():
        return (CtypesProxy(), wintypes, kernel, advapi, Userenv(), None)

    return state, arrays, load


def _call_derive(load):
    global _load_win32
    original = _load_win32
    _load_win32 = load
    try:
        try:
            return ("return", derive_internet_client_sid())
        except SetupError as exc:
            return ("setup", str(exc))
        except AttributeError as exc:
            return ("attribute", str(exc))
        except OverflowError as exc:
            return ("overflow", str(exc))
    finally:
        _load_win32 = original


def _signature_ready(state, wintypes):
    argtypes = state["argtypes"]
    return state["restype"] == wintypes.BOOL and isinstance(argtypes, tuple) and len(argtypes) == 5


def _freed_pair(state, arrays):
    freed = set(state["frees"])
    return {arrays["group"], arrays["cap"], arrays["group_sid"], arrays["cap_sid"]} <= freed


def _native_binding_gaps():
    """KernelBase BOOL binding and hosted receipt retention. No Win32 and no network."""
    import ctypes
    import os
    import socket
    import tempfile
    from ctypes import wintypes

    global _load_win32, record_context, read_audit, set_audit_failure, _listeners, run_leg, create_profile_once
    gaps = []

    def require_kernelbase(state):
        if state["requested"] != ["KernelBase"] or state["userenv"]:
            gaps.append("kernelbase_export")
        if state["calls"] != 1 or state["name"] != "internetClient":
            gaps.append("sid_constant_bypass")
        if not _signature_ready(state, wintypes):
            gaps.append("missing_signature")

    false_state, false_arrays, false_load = _sid_binding("false")
    false_kind, false_value = _call_derive(false_load)
    require_kernelbase(false_state)
    if false_kind != "setup" or false_value != "DeriveCapabilitySidsFromName failed: 5":
        gaps.append("false_returned_sid")
    if false_state["last_error_reads"] != [5] or false_state["convert_calls"] != 0:
        gaps.append("get_last_error")
    if not _freed_pair(false_state, false_arrays):
        gaps.append("leaked_on_false")

    wide_state, wide_arrays, wide_load = _sid_binding("wide_false")
    wide_kind, wide_value = _call_derive(wide_load)
    require_kernelbase(wide_state)
    wide_sids = {wide_arrays["group_sid"], wide_arrays["cap_sid"]}
    wide_pointers = {
        value
        for value, was_pointer in zip(wide_state["frees"], wide_state["free_was_pointer"])
        if was_pointer
    }
    if (
        wide_kind != "setup"
        or wide_value != "DeriveCapabilitySidsFromName failed: 5"
        or wide_state["convert_calls"] != 0
        or not wide_sids <= wide_pointers
        or not _freed_pair(wide_state, wide_arrays)
    ):
        gaps.append("wide_pointer_localfree")

    missing_state, _missing_arrays, missing_load = _sid_binding("missing")
    missing_kind, missing_value = _call_derive(missing_load)
    if missing_state["requested"] != ["KernelBase"] or missing_state["userenv"]:
        gaps.append("kernelbase_export")
    if missing_kind != "setup" or "export missing" not in missing_value:
        gaps.append("missing_export_not_setup")
    if missing_state["frees"] or missing_state["convert_calls"]:
        gaps.append("missing_export_used_buffers")

    success_state, success_arrays, success_load = _sid_binding("success")
    success_kind, success_value = _call_derive(success_load)
    require_kernelbase(success_state)
    if success_kind != "return" or success_value != INTERNET_CLIENT_SID:
        gaps.append("documented_sid_rejected")
    if success_state["text_ptr"] not in success_state["frees"] or not _freed_pair(success_state, success_arrays):
        gaps.append("leaked_on_success")

    wide_ok_state, wide_ok_arrays, wide_ok_load = _sid_binding("wide_success")
    wide_ok_kind, wide_ok_value = _call_derive(wide_ok_load)
    require_kernelbase(wide_ok_state)
    wide_ok_pointers = {
        value
        for value, was_pointer in zip(wide_ok_state["frees"], wide_ok_state["free_was_pointer"])
        if was_pointer
    }
    if (
        wide_ok_kind != "return"
        or wide_ok_value != INTERNET_CLIENT_SID
        or wide_ok_state["convert_calls"] != 1
        or wide_ok_state["convert_pointer"] is not True
        or wide_ok_state["convert_sid"] != wide_ok_arrays["cap_sid"]
        or not {wide_ok_arrays["group"], wide_ok_arrays["cap"]} <= wide_ok_pointers
        or not _freed_pair(wide_ok_state, wide_ok_arrays)
    ):
        gaps.append("wide_pointer_convert")

    mismatch_state, mismatch_arrays, mismatch_load = _sid_binding("mismatch")
    mismatch_kind, mismatch_value = _call_derive(mismatch_load)
    require_kernelbase(mismatch_state)
    if mismatch_kind != "setup" or mismatch_value == INTERNET_CLIENT_SID:
        gaps.append("sid_constant_bypass")
    if mismatch_state["text_ptr"] not in mismatch_state["frees"] or not _freed_pair(
        mismatch_state, mismatch_arrays
    ):
        gaps.append("leaked_on_mismatch")

    empty_state, empty_arrays, empty_load = _sid_binding("empty")
    empty_kind, _empty_value = _call_derive(empty_load)
    require_kernelbase(empty_state)
    if empty_kind != "setup" or empty_state["convert_calls"] != 0:
        gaps.append("empty_deref")
    if empty_arrays["group_sid"] in empty_state["frees"] or empty_arrays["cap_sid"] in empty_state["frees"]:
        gaps.append("empty_deref")
    if empty_arrays["group"] not in empty_state["frees"] or empty_arrays["cap"] not in empty_state["frees"]:
        gaps.append("leaked_on_empty")

    convert_state, convert_arrays, convert_load = _sid_binding("convert_fail")
    convert_kind, _convert_value = _call_derive(convert_load)
    require_kernelbase(convert_state)
    if convert_kind != "setup" or convert_state["convert_calls"] != 1:
        gaps.append("convert_failure")
    if not _freed_pair(convert_state, convert_arrays):
        gaps.append("leaked_on_convert")

    hosted_state, _hosted_arrays, hosted_load = _sid_binding("missing")
    original_load = _load_win32
    original_context = record_context
    original_audit = read_audit
    original_set_audit = set_audit_failure
    original_listeners = _listeners
    original_leg = run_leg
    original_profile = create_profile_once
    original_lookup = socket.getaddrinfo
    cwd = os.getcwd()

    def quiet_context():
        return {
            "image_os": "synthetic",
            "image_version": "synthetic",
            "os_build": "0",
            "command_exit": 0,
            "truncated": False,
            "body": "",
        }

    def quiet_audit():
        return "No Auditing", "synthetic\n"

    def quiet_set_audit():
        return None

    class QuietSocket:
        def getsockname(self):
            return ("127.0.0.1", 9)

        def close(self):
            return None

    def quiet_listeners():
        return QuietSocket(), QuietSocket(), {"flag": True}

    def quiet_leg(protocol, target, port, timeout=5):
        return {
            "protocol": protocol,
            "target": target,
            "port": int(port),
            "result": "connected",
            "winerror": None,
            "start": "2026-09-24T00:00:00+00:00",
            "end": "2026-09-24T00:00:01+00:00",
        }

    def quiet_profile():
        return {"name": "synthetic", "sid": "S-1-15-2-9", "folder": None}

    def quiet_lookup(_host, _port, *args, **kwargs):
        return [(socket.AF_INET, socket.SOCK_STREAM, 0, "", ("203.0.113.5", 443))]

    try:
        with tempfile.TemporaryDirectory() as folder:
            os.chdir(folder)
            _load_win32 = hosted_load
            record_context = quiet_context
            read_audit = quiet_audit
            set_audit_failure = quiet_set_audit
            _listeners = quiet_listeners
            run_leg = quiet_leg
            create_profile_once = quiet_profile
            socket.getaddrinfo = quiet_lookup
            escaped = None
            code = None
            try:
                code = run_hosted()
            except (AttributeError, OSError) as exc:
                escaped = exc
            results = Path("results")
            receipts_path = results / "receipts.json"
            verdict_path = results / "verdict.json"
            cleanup_path = results / "cleanup.json"
            if escaped is not None or not receipts_path.is_file() or not verdict_path.is_file() or not cleanup_path.is_file():
                gaps.append("binding_failure_dropped_receipts")
            else:
                receipts = json.loads(receipts_path.read_text(encoding="utf-8"))
                verdict = json.loads(verdict_path.read_text(encoding="utf-8"))
                cleanup_body = json.loads(cleanup_path.read_text(encoding="utf-8"))
                expected = evaluate(receipts)
                setup_error = receipts.get("setup_error")
                cleanup_receipt = receipts.get("cleanup")
                if (
                    code != 4
                    or not isinstance(setup_error, str)
                    or "export missing" not in setup_error
                    or verdict.get("verdict") != "SETUP"
                    or verdict.get("completed") is not False
                    or verdict.get("verdict") != expected["verdict"]
                    or verdict.get("completed") != expected["completed"]
                    or not isinstance(cleanup_receipt, dict)
                    or cleanup_receipt.get("status") == "clean"
                    or cleanup_body != cleanup_receipt
                    or hosted_state["requested"] != ["KernelBase"]
                    or "DeriveCapabilitySidsFromName" in hosted_state["userenv"]
                ):
                    gaps.append("binding_failure_dropped_receipts")
    finally:
        os.chdir(cwd)
        _load_win32 = original_load
        record_context = original_context
        read_audit = original_audit
        set_audit_failure = original_set_audit
        _listeners = original_listeners
        run_leg = original_leg
        create_profile_once = original_profile
        socket.getaddrinfo = original_lookup
    if _LOADED_WIN32:
        gaps.append("loaded_win32")
    return gaps


def self_test():
    results_dir = ROOT / "results"
    before = None
    if results_dir.exists():
        before = sorted(path.relative_to(results_dir).as_posix() for path in results_dir.rglob("*"))
    red_failures = []
    green_failures = []
    gaps = _repair_gaps()
    if gaps:
        print("RED repair " + ",".join(gaps))
        green_failures.extend(gaps)
    else:
        print("GREEN repair")
    closer_gaps = _closer_gaps()
    if closer_gaps:
        print("RED closer " + ",".join(closer_gaps))
        green_failures.extend(closer_gaps)
    else:
        print("GREEN closer")
    producer_gaps = _producer_gaps()
    if producer_gaps:
        print("RED producer " + ",".join(producer_gaps))
        green_failures.extend(producer_gaps)
    else:
        print("GREEN producer")
    capture_gaps = _capture_gaps()
    if capture_gaps:
        print("RED capture " + ",".join(capture_gaps))
        green_failures.extend(capture_gaps)
    else:
        print("GREEN capture")
    native_gaps = _native_binding_gaps()
    if native_gaps:
        print("RED native " + ",".join(native_gaps))
        green_failures.extend(native_gaps)
    else:
        print("GREEN native")
    contract = _workflow_contract() + _helper_problems()
    if contract:
        print("RED contract " + ",".join(contract))
        green_failures.append("contract")
    else:
        print("GREEN contract")
    cases = _cases()
    mutations = [
        ("ignore_5157", {"ignore_5157"}, ["c0_no_5157", "event_wrong_pid", "event_outside_window"]),
        ("ignore_connect", {"ignore_connect"}, ["c0_connected", "grandchild_connected"]),
        ("ignore_token", {"ignore_token"}, ["wrong_capabilities"]),
        ("ignore_grandchild", {"ignore_grandchild"}, ["grandchild_not_container", "grandchild_constructed"]),
        ("ignore_verifier", {"ignore_verifier"}, ["equal_empty_report", "equal_error_report"]),
        ("ignore_cleanup", {"ignore_cleanup"}, ["cleanup_unknown", "cleanup_dirty"]),
        ("ignore_c1", {"ignore_c1"}, ["c1_external_failed"]),
    ]
    by_name = {name: (fn, expect) for name, fn, expect in cases}
    control = pass_receipt()
    control_result = evaluate(control)
    control_expect = {
        "verdict": "PASS",
        "completed": True,
        "claim": "TCP only",
        "unqualified_offline": False,
        "five_platform_acceptance": False,
        "dns_external_query_proven": False,
        "cleanup": "clean",
    }
    for mutation, flags, names in mutations:
        for name in names:
            fn, expect = by_name[name]
            weakened = evaluate(_copy(fn), weaken=flags)
            if _matches(weakened, expect):
                print(
                    "RED-MISSING "
                    + mutation
                    + " "
                    + name
                    + " got="
                    + weakened["verdict"]
                    + " completed="
                    + str(weakened["completed"])
                )
                red_failures.append(mutation + ":" + name)
            else:
                print(
                    "RED "
                    + mutation
                    + " "
                    + name
                    + " got="
                    + weakened["verdict"]
                    + " completed="
                    + str(weakened["completed"])
                    + " want="
                    + expect["verdict"]
                )
            held = evaluate(control, weaken=flags)
            if held["verdict"] == "PASS" and held["completed"] is True:
                print("GREEN " + mutation + " pass-control")
            else:
                print("RED-CONTROL " + mutation + " " + held["verdict"])
                green_failures.append(mutation + ":pass-control")
            restored = evaluate(_copy(fn))
            if _matches(restored, expect):
                print("GREEN restored " + name + " " + restored["verdict"])
            else:
                print(
                    "RED-RESTORED "
                    + name
                    + " got="
                    + restored["verdict"]
                    + " completed="
                    + str(restored["completed"])
                    + " claim="
                    + restored["claim"]
                )
                green_failures.append(name)
    covered = {name for _mutation, _flags, names in mutations for name in names}
    for name, fn, expect in cases:
        if name in covered:
            continue
        restored = evaluate(_copy(fn))
        if _matches(restored, expect):
            print("GREEN case " + name + " " + restored["verdict"])
        else:
            print(
                "RED-RESTORED case "
                + name
                + " got="
                + restored["verdict"]
                + " completed="
                + str(restored["completed"])
                + " claim="
                + restored["claim"]
            )
            green_failures.append("case:" + name)
    if _matches(control_result, control_expect):
        print("GREEN restored pass-control")
    else:
        print("RED-RESTORED pass-control " + json.dumps(control_result, sort_keys=True))
        green_failures.append("pass-control")
    timed = _copy(lambda receipt: receipt["c0"]["legs"]["tcp_external"].update(result="timeout"))
    if evaluate(timed, weaken={"ignore_5157"})["verdict"] != "INCONCLUSIVE":
        print("RED-CONTROL ignore_5157 accepted timeout")
        green_failures.append("timeout")
    else:
        print("GREEN ignore_5157 rejects timeout")
    lied = _copy(lambda receipt: receipt.__setitem__("weaken", ["noop"]))
    if evaluate(lied)["verdict"] != "PASS":
        print("RED-RESTORED weaken key changed a pass receipt")
        green_failures.append("weaken-key")
    else:
        bad = _copy(lambda receipt: receipt["c1_before"]["legs"]["tcp_external"].update(result="failed"))
        bad["weaken"] = ["noop"]
        if evaluate(bad)["verdict"] != "INCONCLUSIVE":
            print("RED-RESTORED receipt weaken key was honored")
            green_failures.append("weaken-key")
        else:
            print("GREEN receipt cannot disable checks")
    plan = plan_document()
    if plan["mandatory_legs"] != list(MANDATORY_LEGS) or plan["five_platform_acceptance"] is not False:
        green_failures.append("plan")
        print("RED-RESTORED plan")
    else:
        print("GREEN plan")
    after = None
    if results_dir.exists():
        after = sorted(path.relative_to(results_dir).as_posix() for path in results_dir.rglob("*"))
    if before != after or _LOADED_WIN32:
        print("RED-RESTORED host change or windows dll")
        green_failures.append("host")
    else:
        print("GREEN no host change")
    if green_failures:
        print("self-test exit 1")
        return 1
    if red_failures:
        print("self-test exit 2")
        return 2
    print("self-test exit 0")
    return 0


def _load_win32():
    global _LOADED_WIN32
    if sys.platform != "win32":
        raise SetupError("Windows DLLs are not loaded on this host")
    import ctypes
    from ctypes import wintypes

    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    advapi32 = ctypes.WinDLL("advapi32", use_last_error=True)
    userenv = ctypes.WinDLL("userenv", use_last_error=True)
    ole32 = ctypes.WinDLL("ole32", use_last_error=True)
    _LOADED_WIN32 = True
    return ctypes, wintypes, kernel32, advapi32, userenv, ole32


def _bounded(text, limit):
    if text is None:
        return "", False
    if isinstance(text, bytes):
        text = text.decode("utf-8", "replace")
    if len(text) <= limit:
        return text, False
    return text[:limit], True


def _blank_capture(cleanup):
    return {
        "exit": None,
        "stdout": "",
        "stderr": "",
        "truncated": False,
        "late": False,
        "cleanup": cleanup,
        "accepted": False,
    }


def read_counted(read, limit):
    """Count fixed-size chunks from read(n). Does not close, kill, or reap."""
    if not isinstance(limit, int) or limit < 1:
        return b"", True
    chunks = []
    total = 0
    while total < limit:
        want = min(READ_CHUNK_BYTES, limit - total)
        block = read(want)
        if not block:
            return b"".join(chunks), False
        if not isinstance(block, (bytes, bytearray)) or len(block) > want:
            return b"", True
        chunks.append(bytes(block))
        total += len(block)
    if read(1):
        return b"", True
    return b"".join(chunks), False


def _bounded_cleanup(child):
    import threading

    box = {}

    def run():
        try:
            box["value"] = child.cleanup()
        except Exception:
            box["error"] = True

    thread = threading.Thread(target=run, daemon=True)
    thread.start()
    thread.join(CLEANUP_BUDGET_SECONDS)
    if thread.is_alive() or box.get("error") or "value" not in box:
        return "unknown"
    if box["value"] is True:
        return "clean"
    if box["value"] is False:
        return "failed"
    return "unknown"


def _release_owned(child, threads):
    import time

    try:
        if child.poll() is None:
            child.terminate()
    except Exception:
        pass
    end = time.monotonic() + CLEANUP_BUDGET_SECONDS
    for thread in threads:
        remaining = end - time.monotonic()
        if remaining > 0:
            thread.join(remaining)
    reaped = None
    try:
        reaped = child.reap(max(0.0, end - time.monotonic()))
    except Exception:
        reaped = None
    status = _bounded_cleanup(child)
    if status != "clean":
        return status
    if any(thread.is_alive() for thread in threads):
        return "unknown"
    if reaped is None and child.poll() is None:
        return "unknown"
    return "clean"


def supervise_owned(child, stdout_limit, stderr_limit, deadline_seconds, clock=None):
    """Count both pipes until the deadline. Threads, not select: Windows anonymous pipes are not selectable."""
    import threading
    import time

    now = time.monotonic if clock is None else clock.monotonic
    pause = time.sleep if clock is None else clock.sleep
    if getattr(child, "owned", False) is not True:
        return _blank_capture("unknown")
    if isinstance(deadline_seconds, bool) or not isinstance(deadline_seconds, (int, float)) or deadline_seconds <= 0:
        return _blank_capture("unknown")
    slots = {"out": {}, "err": {}}

    def pump(read, limit, key):
        try:
            body, over = read_counted(read, limit)
            slots[key]["body"] = body
            slots[key]["over"] = over
        except Exception:
            slots[key]["body"] = b""
            slots[key]["over"] = True

    threads = [
        threading.Thread(target=pump, args=(child.read_stdout, stdout_limit, "out"), daemon=True),
        threading.Thread(target=pump, args=(child.read_stderr, stderr_limit, "err"), daemon=True),
    ]
    deadline = now() + float(deadline_seconds)
    for thread in threads:
        thread.start()
    late = False
    for thread in threads:
        remaining = deadline - now()
        if remaining <= 0:
            late = True
            break
        thread.join(remaining)
        if thread.is_alive():
            late = True
            break
    truncated = bool(slots["out"].get("over") or slots["err"].get("over"))
    if not late and not truncated:
        while child.poll() is None and now() < deadline:
            pause(0.02)
        # A zero status first seen after the cutoff is not success. The pause
        # does not extend the deadline.
        if child.poll() is None or now() >= deadline:
            late = True
    status = _release_owned(child, threads)
    stdout_bytes = slots["out"].get("body", b"")
    if not isinstance(stdout_bytes, bytes) or late:
        stdout_bytes = b""
    if late or truncated or status != "clean" or child.poll() is None:
        return {
            "exit": None,
            "stdout": "",
            "stderr": "",
            "stdout_bytes": stdout_bytes,
            "truncated": truncated,
            "late": late,
            "cleanup": status,
            "accepted": False,
        }
    code = child.poll()
    return {
        "exit": code,
        "stdout": stdout_bytes.decode("utf-8", "replace"),
        "stderr": slots["err"].get("body", b"").decode("utf-8", "replace"),
        "stdout_bytes": stdout_bytes,
        "truncated": False,
        "late": False,
        "cleanup": status,
        "accepted": code == 0,
    }


def _zip_member_key(name):
    return name.replace("\\", "/").rstrip("/").casefold()


def _zip_member_rejected(info):
    name = getattr(info, "filename", None)
    if not safe_zip_member(name) or ":" in name or "\x00" in name:
        return True
    mode = (int(getattr(info, "external_attr", 0) or 0) >> 16) & 0o170000
    return mode not in (0, 0o100000, 0o040000)


def _zip_destination(dest, name):
    relative = name.replace("\\", "/").rstrip("/")
    target = dest.joinpath(*[part for part in relative.split("/") if part])
    resolved_dest = dest.resolve()
    resolved = target.resolve()
    if resolved != resolved_dest and resolved_dest not in resolved.parents:
        raise SetupError("archive member rejected")
    return target


def extract_bounded_archive(archive, dest, limit):
    """Stream members and count bytes written. Delete the tree on any failure."""
    import shutil

    dest = Path(dest)
    if isinstance(limit, bool) or not isinstance(limit, int) or limit < 1 or limit > ARCHIVE_DECODE_BYTES:
        raise SetupError("archive decode ceiling rejected")
    if dest.exists():
        shutil.rmtree(dest)
    dest.mkdir(parents=True)
    try:
        infos = list(archive.infolist())
        if len(infos) > ZIP_MEMBER_LIMIT:
            raise SetupError("archive member count")
        seen = set()
        declared = 0
        for info in infos:
            if _zip_member_rejected(info):
                raise SetupError("archive member rejected")
            key = _zip_member_key(info.filename)
            if not key or key in seen:
                raise SetupError("archive duplicate member")
            seen.add(key)
            if info.filename.endswith("/") or info.is_dir():
                continue
            size = getattr(info, "file_size", None)
            if not isinstance(size, int) or isinstance(size, bool) or size < 0 or size > limit - declared:
                raise SetupError("archive declared size")
            declared += size
        written = 0
        for info in infos:
            target = _zip_destination(dest, info.filename)
            if info.filename.endswith("/") or info.is_dir():
                target.mkdir(parents=True, exist_ok=True)
                continue
            target.parent.mkdir(parents=True, exist_ok=True)
            with archive.open(info) as source, target.open("wb") as sink:
                while True:
                    want = min(READ_CHUNK_BYTES, limit - written + 1)
                    if want < 1:
                        raise SetupError("archive decoded size")
                    block = source.read(want)
                    if not block:
                        break
                    if not isinstance(block, (bytes, bytearray)) or len(block) > want or written + len(block) > limit:
                        raise SetupError("archive decoded size")
                    sink.write(block)
                    written += len(block)
    except Exception:
        shutil.rmtree(dest, ignore_errors=True)
        raise
    assay = next((path for path in dest.rglob("assay.exe") if path.is_file()), None)
    if assay is None:
        shutil.rmtree(dest, ignore_errors=True)
        raise SetupError("assay.exe missing after digest check")
    return assay


class _OwnedPopen:
    owned = True

    def __init__(self, process):
        self.process = process

    def read_stdout(self, n):
        return self.process.stdout.read(n)

    def read_stderr(self, n):
        return self.process.stderr.read(n)

    def poll(self):
        return self.process.poll()

    def terminate(self):
        if self.process.poll() is None:
            self.process.terminate()

    def reap(self, timeout):
        import subprocess

        if self.process.poll() is not None:
            return self.process.poll()
        try:
            return self.process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            self.process.kill()
            try:
                return self.process.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                return None

    def cleanup(self):
        closed = True
        for pipe in (self.process.stdout, self.process.stderr):
            if pipe is None:
                continue
            try:
                pipe.close()
            except OSError:
                closed = False
        if self.process.poll() is None:
            try:
                self.process.kill()
            except OSError:
                return False
            try:
                self.process.wait(timeout=CLEANUP_BUDGET_SECONDS)
            except Exception:
                return False
        return closed and self.process.poll() is not None


def _spawn_owned(argv, env):
    import subprocess

    process = subprocess.Popen(
        argv,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        stdin=subprocess.DEVNULL,
        env=env,
    )
    return _OwnedPopen(process)


def _public_command(result):
    discard = result["truncated"] or result["late"] or result["cleanup"] != "clean"
    exit_code = result["exit"]
    if exit_code is None or (discard and exit_code == 0):
        exit_code = 1
    return {
        "exit": exit_code,
        "stdout": "" if discard else result["stdout"],
        "stderr": "" if discard else result["stderr"],
        "truncated": discard,
    }


def _run_command(argv, timeout, env=None, spawn=None):
    child = spawn(argv, env) if spawn else _spawn_owned(argv, env)
    return _public_command(
        supervise_owned(child, COMMAND_STDOUT_BYTES, COMMAND_STDERR_BYTES, timeout)
    )


def _powershell(script, env, timeout, spawn=None):
    argv = ["powershell.exe", "-NoProfile", "-NonInteractive", "-Command", script]
    child = spawn(argv, env) if spawn else _spawn_owned(argv, env)
    view = _public_command(
        supervise_owned(child, POWERSHELL_STDOUT_BYTES, COMMAND_STDERR_BYTES, timeout)
    )
    return view["exit"], view["stdout"], view["truncated"]


def _capture_grandchild(argv, env, timeout, spawn=None):
    child = spawn(argv, env) if spawn else _spawn_owned(argv, env)
    return supervise_owned(child, GRANDCHILD_CAPTURE_BYTES, COMMAND_STDERR_BYTES, timeout)


def _grandchild_from_capture(captured):
    if not isinstance(captured, dict) or not captured.get("accepted"):
        return {"spawn": "inherited", "parse_error": True}
    try:
        parsed = json.loads(captured.get("stdout") or "")
    except json.JSONDecodeError:
        return {"spawn": "inherited", "parse_error": True}
    if not isinstance(parsed, dict):
        return {"spawn": "inherited", "parse_error": True}
    return parsed


def record_context():
    script = (
        "$fw = Get-NetFirewallProfile | Select-Object Name, Enabled | ConvertTo-Json -Compress; "
        "$svc = Get-Service mpssvc,BFE,Dnscache | Select-Object Name, Status | ConvertTo-Json -Compress; "
        "Write-Output $fw; Write-Output '---'; Write-Output $svc"
    )
    try:
        code, stdout, truncated = _powershell(script, os.environ.copy(), 30)
    except (OSError, TimeoutError):
        code, stdout, truncated = 1, "", False
    return {
        "image_os": os.environ.get("ImageOS"),
        "image_version": os.environ.get("ImageVersion"),
        "os_build": str(sys.getwindowsversion().build) if hasattr(sys, "getwindowsversion") else None,
        "command_exit": code,
        "truncated": truncated,
        "body": stdout,
    }


def _audit_inclusion(stdout):
    for line in stdout.splitlines():
        if "Filtering Platform Connection" not in line:
            continue
        cells = [cell.strip() for cell in line.split(",")]
        if len(cells) >= 5:
            return cells[4]
    return None


def read_audit():
    result = _run_command(
        ["auditpol", "/get", "/subcategory:Filtering Platform Connection", "/r"],
        30,
    )
    return _audit_inclusion(result["stdout"]), result["stdout"]


def set_audit_failure():
    result = _run_command(
        ["auditpol", "/set", "/subcategory:Filtering Platform Connection", "/failure:enable"],
        30,
    )
    if result["exit"] != 0:
        raise SetupError("audit failure enable failed")


def restore_audit(inclusion):
    mapping = {
        "No Auditing": ["/success:disable", "/failure:disable"],
        "Success": ["/success:enable", "/failure:disable"],
        "Failure": ["/success:disable", "/failure:enable"],
        "Success and Failure": ["/success:enable", "/failure:enable"],
    }
    flags = mapping.get(inclusion)
    if not flags:
        return False
    result = _run_command(
        ["auditpol", "/set", "/subcategory:Filtering Platform Connection", *flags],
        30,
    )
    return result["exit"] == 0


def _icacls(args):
    return _run_command(["icacls", *args], 30)


def _dacl_has_sid(path, sid):
    result = _icacls([str(path)])
    if result["exit"] != 0 or result["truncated"]:
        return None
    return sid in result["stdout"]


def grant_paths(sid, paths, recorded):
    if not valid_sid(sid) or sid in (ALL_APPLICATION_PACKAGES, "S-1-15-2-2"):
        raise SetupError("refusing grant for this SID")
    for path, directory in paths:
        if _dacl_has_sid(path, sid) is not False:
            raise SetupError("SID already present or DACL unreadable")
        rights = "(OI)(CI)(RX)" if directory else "(RX)"
        spec = "*" + sid + ":" + rights
        result = _icacls([str(path), "/grant", spec])
        recorded.append({"path": str(path), "spec": spec})
        if result["exit"] != 0:
            raise SetupError("grant failed")


def revoke_paths(sid, recorded):
    outcomes = []
    for item in recorded:
        result = _icacls([item["path"], "/remove:g", "*" + sid])
        present = _dacl_has_sid(item["path"], sid)
        outcomes.append(result["exit"] == 0 and present is False)
    return outcomes


def apply_direction_rendering(event, renderer):
    """Render a %% parameter through the event's own provider. Unknown stays unknown."""
    if not isinstance(event, dict) or event.get("direction") in ("outbound", "inbound"):
        return event
    raw = event.get("raw_direction") or ""
    if not raw.startswith("%%") or not raw[2:].isdigit():
        return event
    provider = event.get("provider")
    record_id = event.get("record_id")
    if not isinstance(provider, str) or not provider or record_id in (None, ""):
        return event
    message_id = int(raw[2:])
    try:
        rendered = renderer(provider, record_id, message_id, DIRECTION_RENDER_CHARS)
    except Exception:
        return event
    if not isinstance(rendered, str) or len(rendered) > DIRECTION_RENDER_CHARS:
        return event
    folded = rendered.strip().casefold()
    if folded not in ("outbound", "inbound"):
        return event
    event["direction"] = folded
    event["rendered_direction"] = rendered.strip()
    event["render_message_id"] = message_id
    return event


def _wevt_render_parameter(provider, record_id, message_id, buffer_chars):
    """EvtFormatMessageId for one %% parameter. The record id binds the call; the API takes no event handle."""
    del record_id
    if sys.platform != "win32":
        raise SetupError("Windows DLLs are not loaded on this host")
    import ctypes
    from ctypes import wintypes

    # EvtFormatMessageEvent is 1. EvtFormatMessageId is the eighth enumerator.
    evt_format_message_id = 8
    wevtapi = ctypes.WinDLL("wevtapi", use_last_error=True)
    wevtapi.EvtOpenPublisherMetadata.argtypes = (
        ctypes.c_void_p,
        ctypes.c_wchar_p,
        ctypes.c_wchar_p,
        wintypes.DWORD,
        wintypes.DWORD,
    )
    wevtapi.EvtOpenPublisherMetadata.restype = ctypes.c_void_p
    wevtapi.EvtFormatMessage.argtypes = (
        ctypes.c_void_p,
        ctypes.c_void_p,
        wintypes.DWORD,
        wintypes.DWORD,
        ctypes.c_void_p,
        wintypes.DWORD,
        wintypes.DWORD,
        ctypes.c_wchar_p,
        ctypes.POINTER(wintypes.DWORD),
    )
    wevtapi.EvtFormatMessage.restype = wintypes.BOOL
    wevtapi.EvtClose.argtypes = (ctypes.c_void_p,)
    wevtapi.EvtClose.restype = wintypes.BOOL
    metadata = wevtapi.EvtOpenPublisherMetadata(None, provider, None, 0, 0)
    if not metadata:
        return None
    try:
        if buffer_chars > DIRECTION_RENDER_CHARS:
            buffer_chars = DIRECTION_RENDER_CHARS
        buffer = ctypes.create_unicode_buffer(buffer_chars)
        used = wintypes.DWORD()
        ok = wevtapi.EvtFormatMessage(
            metadata,
            None,
            int(message_id),
            0,
            None,
            evt_format_message_id,
            buffer_chars,
            buffer,
            ctypes.byref(used),
        )
        if not ok:
            return None
        return buffer.value
    finally:
        wevtapi.EvtClose(metadata)


def collect_events(leg, pid, producer=None, renderer=None):
    window = query_window_iso(leg.get("start") if isinstance(leg, dict) else None, leg.get("end") if isinstance(leg, dict) else None)
    if window is None:
        return [], False, True
    script = (
        "$ErrorActionPreference = 'Continue'; "
        "$start = [datetime]::Parse($env:PROBE_WINDOW_START); "
        "$end = [datetime]::Parse($env:PROBE_WINDOW_END); "
        "$events = @(Get-WinEvent -FilterHashtable "
        "@{LogName='Security'; Id=5157; StartTime=$start; EndTime=$end} "
        "-MaxEvents 40 -ErrorAction SilentlyContinue); "
        "foreach ($event in $events) { Write-Output '---EVENT---'; Write-Output $event.ToXml() }"
    )
    env = child_environment(os.environ)
    env["PROBE_WINDOW_START"] = window[0]
    env["PROBE_WINDOW_END"] = window[1]
    producer = producer or _powershell
    renderer = renderer or _wevt_render_parameter
    try:
        code, stdout, truncated = producer(script, env, 30)
    except (OSError, TimeoutError):
        return [], False, True
    if code not in (0, 1):
        return [], False, True
    parsed = []
    for chunk in stdout.split("---EVENT---"):
        piece = chunk.strip()
        if not piece:
            continue
        if len(piece) > 8192:
            truncated = True
            continue
        event = parse_event_xml(piece)
        if event:
            apply_direction_rendering(event, renderer)
            parsed.append(event)
    return records_for_leg(parsed, leg, pid), truncated, False


def read_bounded_http(response, limit):
    return read_counted(response.read, limit)


def _retain_filter_text(destination, text):
    destination.parent.mkdir(parents=True, exist_ok=True)
    if text:
        destination.write_text(text, encoding="utf-8")
    else:
        destination.write_bytes(b"")


def _filter_stdout_status(shown):
    """Ready only after EOF, exit 0, and no timeout or truncation."""
    if not isinstance(shown, dict):
        return "failed"
    if shown.get("truncated") is True:
        return "truncated"
    if (
        shown.get("late")
        or shown.get("accepted") is not True
        or shown.get("exit") != 0
        or shown.get("cleanup") != "clean"
    ):
        return "failed"
    return "ready"


def _xml_parser_text(text):
    """Text ElementTree.fromstring parses.

    fromstring skips exactly one leading U+FEFF and no other prefix. A second
    leading U+FEFF is outside that boundary.
    """
    if not isinstance(text, str):
        return None
    if text.startswith("\ufeff"):
        text = text[1:]
        if text.startswith("\ufeff"):
            return None
    return text


def _declared_xml_encoding(text):
    """Encoding token in a leading XML declaration, '' when absent, None when unreadable."""
    if not isinstance(text, str) or not text.startswith("<?xml"):
        return ""
    end = text.find("?>")
    if end < 0:
        return None
    prologue = text[5:end]
    folded = prologue.casefold()
    at = folded.find("encoding")
    if at < 0:
        return ""
    if folded.find("encoding", at + len("encoding")) >= 0:
        return None
    if at > 0 and (prologue[at - 1].isalnum() or prologue[at - 1] in "._-"):
        return None
    rest = prologue[at + len("encoding") :].lstrip(" \t\r\n")
    if not rest.startswith("="):
        return None
    rest = rest[1:].lstrip(" \t\r\n")
    if len(rest) < 2 or rest[0] not in "\"'":
        return None
    stop = rest.find(rest[0], 1)
    if stop < 1:
        return None
    name = rest[1:stop]
    if not name or not name[0].isalpha():
        return None
    allowed = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789._-"
    if any(char not in allowed for char in name):
        return None
    return name


def _decode_filter_stdout(payload):
    """Strict UTF-8, UTF-8 with a BOM, or UTF-16 with a BOM. Never replace errors.

    The declaration has to name that decoder. A mismatch is refused whole.
    """
    if not isinstance(payload, (bytes, bytearray)):
        return None
    data = bytes(payload)
    if data.startswith((b"\xff\xfe", b"\xfe\xff")):
        encoding = "utf-16"
        family = "utf-16"
    elif data.startswith(b"\xef\xbb\xbf"):
        encoding = "utf-8-sig"
        family = "utf-8"
    else:
        encoding = "utf-8"
        family = "utf-8"
    try:
        text = data.decode(encoding)
    except UnicodeError:
        return None
    visible = _xml_parser_text(text)
    if visible is None:
        return None
    declared = _declared_xml_encoding(visible)
    if declared is None:
        return None
    if declared != "" and declared.casefold() != family:
        return None
    return visible


def _complete_filter_xml(text):
    visible = _xml_parser_text(text)
    if not isinstance(visible, str) or not visible.strip():
        return False
    if "<!DOCTYPE" in visible or "<!ENTITY" in visible:
        return False
    try:
        ET.fromstring(visible)
    except (ET.ParseError, ValueError):
        return False
    return True


def collect_filter_evidence(destination, runtime_ids, runner=None, spawn=None, deadline=60):
    """Capture cited filters from `netsh wfp show filters file=- verbose=on`.

    The supervisor's stdout ceiling is FILTER_ACQUIRE_BYTES. That parent read
    is not a bound on netsh or BFE memory. Cited items are selected only after
    a complete, successful capture. The file retains those items and nothing else.
    """
    argv = ["netsh", "wfp", "show", "filters", "file=-", "verbose=on"]
    destination.parent.mkdir(parents=True, exist_ok=True)
    if runner is not None:
        shown = runner(argv, deadline)
    else:
        child = (spawn or _spawn_owned)(argv, None)
        shown = supervise_owned(
            child, FILTER_ACQUIRE_BYTES, COMMAND_STDERR_BYTES, deadline
        )
    status = _filter_stdout_status(shown)
    if status != "ready":
        _retain_filter_text(destination, "")
        if status == "truncated":
            return {"text": "", "truncated": True, "failed": False}
        return {"text": "", "truncated": False, "failed": True}
    text = _decode_filter_stdout(shown.get("stdout_bytes"))
    if text is None or not _complete_filter_xml(text):
        _retain_filter_text(destination, "")
        return {"text": "", "truncated": False, "failed": True}
    selected = select_filter_evidence(text, runtime_ids, hit_ceiling=False)
    kept = ""
    if not selected["failed"] and not selected["truncated"] and selected["text"]:
        kept = selected["text"]
    _retain_filter_text(destination, kept)
    return selected


def _attach_events(arm):
    if not isinstance(arm, dict):
        return {"truncated": False, "failed": True}
    truncated = False
    failed = False
    events = []
    for name in MANDATORY_LEGS:
        leg = _leg(arm, name)
        if not leg:
            continue
        found, one_truncated, one_failed = collect_events(leg, arm.get("pid"))
        events.extend(found)
        truncated = truncated or one_truncated
        failed = failed or one_failed
    arm["events"] = events[:40]
    grandchild = arm.get("grandchild")
    if isinstance(grandchild, dict):
        leg = _leg(grandchild, "tcp_external")
        if leg:
            found, one_truncated, one_failed = collect_events(leg, grandchild.get("pid"))
            grandchild["events"] = found
            truncated = truncated or one_truncated
            failed = failed or one_failed
        else:
            grandchild["events"] = []
            failed = True
    return {"truncated": truncated, "failed": failed}


def run_leg(protocol, target, port, timeout=5):
    import socket

    start = datetime.now(timezone.utc)
    result = "failed"
    winerror = None
    try:
        if protocol == "tcp":
            sock = socket.create_connection((target, int(port)), timeout)
            sock.close()
            result = "connected"
        else:
            sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
            sock.settimeout(timeout)
            sock.sendto(b"\x12\x34\x01\x00\x00\x01\x00\x00\x00\x00\x00\x00\x00\x00\x01\x00\x01", (target, int(port)))
            sock.recvfrom(512)
            sock.close()
            result = "connected"
    except socket.timeout:
        result = "timeout"
    except OSError as exc:
        winerror = getattr(exc, "winerror", None)
        result = "failed"
    end = datetime.now(timezone.utc)
    return {
        "protocol": protocol,
        "target": target,
        "port": int(port),
        "result": result,
        "winerror": winerror,
        "start": start.isoformat(),
        "end": end.isoformat(),
    }


def read_own_token(internet_client_sid):
    ctypes, wintypes, kernel32, advapi32, _userenv, _ole32 = _load_win32()
    token = wintypes.HANDLE()
    if not advapi32.OpenProcessToken(kernel32.GetCurrentProcess(), 0x0008, ctypes.byref(token)):
        raise SetupError("OpenProcessToken failed")
    try:
        is_container = wintypes.DWORD()
        length = wintypes.DWORD()
        if not advapi32.GetTokenInformation(
            token, 29, ctypes.byref(is_container), ctypes.sizeof(is_container), ctypes.byref(length)
        ):
            raise SetupError("TokenIsAppContainer failed")
        sid_info = (ctypes.c_void_p * 1)()
        if not advapi32.GetTokenInformation(
            token, 31, ctypes.byref(sid_info), ctypes.sizeof(sid_info), ctypes.byref(length)
        ):
            raise SetupError("TokenAppContainerSid failed")
        sid_text = ctypes.c_wchar_p()
        if not advapi32.ConvertSidToStringSidW(sid_info[0], ctypes.byref(sid_text)):
            raise SetupError("ConvertSidToStringSidW failed")
        app_sid = sid_text.value
        kernel32.LocalFree(sid_text)
        needed = wintypes.DWORD()
        advapi32.GetTokenInformation(token, 30, None, 0, ctypes.byref(needed))
        buffer = ctypes.create_string_buffer(max(needed.value, 4))
        if not advapi32.GetTokenInformation(
            token, 30, buffer, ctypes.sizeof(buffer), ctypes.byref(needed)
        ):
            raise SetupError("TokenCapabilities failed")
        count = ctypes.cast(buffer, ctypes.POINTER(wintypes.DWORD)).contents.value
        class SidAttr(ctypes.Structure):
            _fields_ = [("Sid", ctypes.c_void_p), ("Attributes", wintypes.DWORD)]
        base = ctypes.sizeof(wintypes.DWORD)
        # Align the array the way the header does on this process.
        if base % ctypes.sizeof(ctypes.c_void_p):
            base += ctypes.sizeof(ctypes.c_void_p) - (base % ctypes.sizeof(ctypes.c_void_p))
        sids = []
        for index in range(count):
            entry = SidAttr.from_buffer(buffer, base + index * ctypes.sizeof(SidAttr))
            text = ctypes.c_wchar_p()
            if advapi32.ConvertSidToStringSidW(entry.Sid, ctypes.byref(text)):
                sids.append(text.value)
                kernel32.LocalFree(text)
        return {
            "is_app_container": bool(is_container.value),
            "sid": app_sid,
            "capabilities": capability_names(sids, internet_client_sid),
            "pid": os.getpid(),
        }
    finally:
        kernel32.CloseHandle(token)


def probe_role(argv):
    def option(name):
        if name not in argv:
            return None
        return argv[argv.index(name) + 1]

    role = option("--probe-role")
    external = option("--external-address")
    port = int(option("--external-port") or "443")
    loop_address = option("--loopback-address") or "127.0.0.1"
    loop_port = int(option("--loopback-port") or "9")
    udp_port = int(option("--udp-port") or "9")
    internet = option("--internet-client-sid") or INTERNET_CLIENT_SID
    body = {
        "role": role,
        "pid": os.getpid(),
        "spawn": "inherited" if role == "grandchild" else "launcher",
        "token": None,
        "legs": {},
    }
    try:
        body["token"] = read_own_token(internet)
    except SetupError as exc:
        body["token_error"] = str(exc)
    if role == "grandchild":
        body["legs"]["tcp_external"] = run_leg("tcp", external, port)
    elif role == "c1":
        body["legs"]["tcp_external"] = run_leg("tcp", external, port)
    elif role == "c0":
        body["legs"]["tcp_loopback"] = run_leg("tcp", loop_address, loop_port)
        body["legs"]["tcp_external"] = run_leg("tcp", external, port)
        body["legs"]["udp_loopback"] = run_leg("udp", loop_address, udp_port)
        body["legs"]["udp_dns"] = run_leg("udp", "1.1.1.1", 53)
        import socket

        try:
            infos = socket.getaddrinfo("github.com", 443, type=socket.SOCK_STREAM)
            addresses = []
            for info in infos[:8]:
                addresses.append(info[4][0])
            body["name_resolution"] = {
                "result": "observed" if addresses else "failed",
                "addresses": addresses,
                "external_query_proven": False,
            }
        except OSError:
            body["name_resolution"] = {
                "result": "failed",
                "addresses": [],
                "external_query_proven": False,
            }
        captured = _capture_grandchild(
            [
                sys.executable,
                str(Path(__file__).resolve()),
                "--probe-role",
                "grandchild",
                "--external-address",
                external,
                "--external-port",
                str(port),
                "--internet-client-sid",
                internet,
            ],
            child_environment(os.environ),
            20,
        )
        body["grandchild"] = _grandchild_from_capture(captured)
    json.dump(body, sys.stdout)
    return 0


def create_profile_once():
    ctypes, wintypes, kernel32, _advapi32, userenv, _ole32 = _load_win32()
    import uuid

    name = "a" + format(os.getpid(), "x") + uuid.uuid4().hex[:8]
    sid = ctypes.c_void_p()
    hr = userenv.CreateAppContainerProfile(
        name,
        name,
        "Assay offline feasibility",
        None,
        0,
        ctypes.byref(sid),
    )
    code = hr & 0xFFFFFFFF
    if code == 0x800700B7:
        raise SetupError("profile already exists; not reused")
    if code >= 0x80000000:
        raise SetupError("CreateAppContainerProfile failed")
    text = ctypes.c_wchar_p()
    advapi = ctypes.WinDLL("advapi32", use_last_error=True)
    if not advapi.ConvertSidToStringSidW(sid, ctypes.byref(text)):
        raise SetupError("profile SID conversion failed")
    sid_text = text.value
    kernel32.LocalFree(text)
    path_ptr = ctypes.c_wchar_p()
    folder_hr = userenv.GetAppContainerFolderPath(sid_text, ctypes.byref(path_ptr))
    folder = path_ptr.value if (folder_hr & 0xFFFFFFFF) < 0x80000000 else None
    if path_ptr:
        ole32 = ctypes.WinDLL("ole32", use_last_error=True)
        ole32.CoTaskMemFree(path_ptr)
    advapi.FreeSid(sid)
    return {"name": name, "sid": sid_text, "folder": folder}


def _free_sid_array(kernel32, ctypes, array_ptr, count):
    if not getattr(array_ptr, "value", None):
        return
    if count > 0:
        slots = ctypes.cast(array_ptr, ctypes.POINTER(ctypes.c_void_p))
        for index in range(count):
            item = slots[index]
            if item:
                kernel32.LocalFree(ctypes.c_void_p(item))
    kernel32.LocalFree(array_ptr)


def derive_internet_client_sid():
    ctypes, wintypes, kernel32, advapi32, _userenv, _ole32 = _load_win32()
    try:
        kernelbase = ctypes.WinDLL("KernelBase", use_last_error=True)
        derive = kernelbase.DeriveCapabilitySidsFromName
    except AttributeError as exc:
        raise SetupError("DeriveCapabilitySidsFromName export missing") from exc
    derive.argtypes = (
        ctypes.c_wchar_p,
        ctypes.POINTER(ctypes.c_void_p),
        ctypes.POINTER(wintypes.DWORD),
        ctypes.POINTER(ctypes.c_void_p),
        ctypes.POINTER(wintypes.DWORD),
    )
    derive.restype = wintypes.BOOL
    group_sids = ctypes.c_void_p()
    group_count = wintypes.DWORD()
    cap_sids = ctypes.c_void_p()
    cap_count = wintypes.DWORD()
    ok = derive(
        "internetClient",
        ctypes.byref(group_sids),
        ctypes.byref(group_count),
        ctypes.byref(cap_sids),
        ctypes.byref(cap_count),
    )
    text = ctypes.c_wchar_p()
    try:
        if not ok:
            code = int(kernel32.GetLastError())
            raise SetupError("DeriveCapabilitySidsFromName failed: " + str(code))
        if int(cap_count.value) < 1 or not cap_sids.value:
            raise SetupError("DeriveCapabilitySidsFromName returned no capability SID")
        first = ctypes.cast(cap_sids, ctypes.POINTER(ctypes.c_void_p))[0]
        if not first:
            raise SetupError("DeriveCapabilitySidsFromName returned no capability SID")
        if not advapi32.ConvertSidToStringSidW(ctypes.c_void_p(first), ctypes.byref(text)):
            raise SetupError("capability SID conversion failed")
        value = text.value
        if value != INTERNET_CLIENT_SID:
            raise SetupError("internetClient SID was not the documented value")
        return value
    finally:
        if text.value:
            kernel32.LocalFree(text)
        _free_sid_array(kernel32, ctypes, group_sids, int(group_count.value))
        _free_sid_array(kernel32, ctypes, cap_sids, int(cap_count.value))


def delete_profile(name):
    _ctypes, _wintypes, _kernel32, _advapi32, userenv, _ole32 = _load_win32()
    last = None
    for _attempt in range(2):
        hr = userenv.DeleteAppContainerProfile(name)
        last = hr & 0xFFFFFFFF
        if last < 0x80000000:
            return True
    return False


def _close_launch_item(item):
    import ctypes

    _ctypes, _wintypes, kernel32, advapi32, _userenv, _ole32 = _load_win32()
    kind = item.get("kind")
    value = item.get("value")
    if kind in ("app_sid", "cap_sid"):
        # FreeSid returns NULL on success and the SID pointer on failure.
        return {"invoked": True, "verified_absent": not advapi32.FreeSid(value)}
    if kind == "attribute_list":
        # DeleteProcThreadAttributeList is VOID. Calling it is not an OS report
        # that the attribute list is gone.
        delete = kernel32.DeleteProcThreadAttributeList
        delete.argtypes = (ctypes.c_void_p,)
        delete.restype = None
        delete(value)
        return {"invoked": True, "verified_absent": None}
    # CloseHandle returns BOOL: nonzero means the handle was closed.
    return {"invoked": True, "verified_absent": bool(kernel32.CloseHandle(value))}


def launch_in_profile(sid, capability_sid, argv, env, timeout, acquired=None):
    if not isinstance(acquired, list):
        acquired = []
    try:
        return _open_in_profile(sid, capability_sid, argv, env, timeout, acquired)
    finally:
        release_acquired(acquired, _close_launch_item)


def _open_in_profile(sid, capability_sid, argv, env, timeout, acquired):
    ctypes, wintypes, kernel32, advapi32, _userenv, _ole32 = _load_win32()
    if not valid_sid(sid):
        raise SetupError("launch SID rejected")
    app_sid = ctypes.c_void_p()
    if not advapi32.ConvertStringSidToSidW(sid, ctypes.byref(app_sid)):
        raise SetupError("ConvertStringSidToSidW failed")
    acquired.append({"kind": "app_sid", "open": True, "value": app_sid})
    cap_buffer = None
    cap_sid = None
    cap_count = 0
    if capability_sid:
        cap_sid = ctypes.c_void_p()
        if not advapi32.ConvertStringSidToSidW(capability_sid, ctypes.byref(cap_sid)):
            raise SetupError("capability conversion failed")
        acquired.append({"kind": "cap_sid", "open": True, "value": cap_sid})
        class SidAttr(ctypes.Structure):
            _fields_ = [("Sid", ctypes.c_void_p), ("Attributes", wintypes.DWORD)]
        cap_buffer = SidAttr(cap_sid, 0x4)
        cap_count = 1
    class SecurityCapabilities(ctypes.Structure):
        _fields_ = [
            ("AppContainerSid", ctypes.c_void_p),
            ("Capabilities", ctypes.c_void_p),
            ("CapabilityCount", wintypes.DWORD),
            ("Reserved", wintypes.DWORD),
        ]
    security = SecurityCapabilities(
        app_sid,
        ctypes.addressof(cap_buffer) if cap_buffer else None,
        cap_count,
        0,
    )
    size = ctypes.c_size_t(0)
    kernel32.InitializeProcThreadAttributeList(None, 2, 0, ctypes.byref(size))
    attribute_list = ctypes.create_string_buffer(size.value)
    if not kernel32.InitializeProcThreadAttributeList(attribute_list, 2, 0, ctypes.byref(size)):
        raise SetupError("InitializeProcThreadAttributeList failed")
    acquired.append({"kind": "attribute_list", "open": True, "value": attribute_list})
    if not kernel32.UpdateProcThreadAttribute(
        attribute_list,
        0,
        0x20009,
        ctypes.byref(security),
        ctypes.sizeof(security),
        None,
        None,
    ):
        raise SetupError("UpdateProcThreadAttribute failed")
    class StartupInfoW(ctypes.Structure):
        _fields_ = [
            ("cb", wintypes.DWORD),
            ("lpReserved", wintypes.LPWSTR),
            ("lpDesktop", wintypes.LPWSTR),
            ("lpTitle", wintypes.LPWSTR),
            ("dwX", wintypes.DWORD),
            ("dwY", wintypes.DWORD),
            ("dwXSize", wintypes.DWORD),
            ("dwYSize", wintypes.DWORD),
            ("dwXCountChars", wintypes.DWORD),
            ("dwYCountChars", wintypes.DWORD),
            ("dwFillAttribute", wintypes.DWORD),
            ("dwFlags", wintypes.DWORD),
            ("wShowWindow", wintypes.WORD),
            ("cbReserved2", wintypes.WORD),
            ("lpReserved2", ctypes.c_void_p),
            ("hStdInput", wintypes.HANDLE),
            ("hStdOutput", wintypes.HANDLE),
            ("hStdError", wintypes.HANDLE),
        ]
    class StartupInfoExW(ctypes.Structure):
        _fields_ = [("StartupInfo", StartupInfoW), ("lpAttributeList", ctypes.c_void_p)]
    class ProcessInformation(ctypes.Structure):
        _fields_ = [
            ("hProcess", wintypes.HANDLE),
            ("hThread", wintypes.HANDLE),
            ("dwProcessId", wintypes.DWORD),
            ("dwThreadId", wintypes.DWORD),
        ]
    import subprocess
    import threading

    command = subprocess.list2cmdline(argv)
    command_buf = ctypes.create_unicode_buffer(command)
    chars = []
    for key, value in env.items():
        chars.extend(key + "=" + value)
        chars.append("\0")
    chars.append("\0")
    environment = (ctypes.c_wchar * len(chars))(*chars)

    class SecurityAttributes(ctypes.Structure):
        _fields_ = [
            ("nLength", wintypes.DWORD),
            ("lpSecurityDescriptor", ctypes.c_void_p),
            ("bInheritHandle", wintypes.BOOL),
        ]

    def pipe():
        read = wintypes.HANDLE()
        write = wintypes.HANDLE()
        attributes = SecurityAttributes(ctypes.sizeof(SecurityAttributes), None, True)
        if not kernel32.CreatePipe(ctypes.byref(read), ctypes.byref(write), ctypes.byref(attributes), 0):
            raise SetupError("CreatePipe failed")
        # The parent keeps the read end. The child must not inherit it.
        kernel32.SetHandleInformation(read, 1, 0)
        return read, write

    stdout_read, stdout_write = pipe()
    acquired.append({"kind": "stdout_read", "open": True, "value": stdout_read})
    acquired.append({"kind": "stdout_write", "open": True, "value": stdout_write})
    stderr_read, stderr_write = pipe()
    acquired.append({"kind": "stderr_read", "open": True, "value": stderr_read})
    acquired.append({"kind": "stderr_write", "open": True, "value": stderr_write})
    inherit = SecurityAttributes(ctypes.sizeof(SecurityAttributes), None, True)
    nul = kernel32.CreateFileW("NUL", 0x80000000, 7, ctypes.byref(inherit), 3, 0, None)
    nul_value = getattr(nul, "value", nul)
    nul_bad = nul_value in (None, 0, -1) or (
        isinstance(nul_value, int) and nul_value & 0xFFFFFFFFFFFFFFFF == 0xFFFFFFFFFFFFFFFF
    )
    if nul_bad:
        raise SetupError("NUL open failed")
    acquired.append({"kind": "nul", "open": True, "value": nul})
    inherited = (wintypes.HANDLE * 3)(nul, stdout_write, stderr_write)
    # 0x20002 is PROC_THREAD_ATTRIBUTE_HANDLE_LIST. The child inherits these
    # three handles and no other handle this process already had open.
    listed = kernel32.UpdateProcThreadAttribute(
        attribute_list,
        0,
        0x20002,
        ctypes.cast(inherited, ctypes.c_void_p),
        ctypes.sizeof(inherited),
        None,
        None,
    )
    if not listed:
        raise SetupError("handle list rejected")
    startup = StartupInfoExW()
    startup.StartupInfo.cb = ctypes.sizeof(StartupInfoExW)
    startup.StartupInfo.dwFlags = 0x00000100
    startup.StartupInfo.hStdInput = nul
    startup.StartupInfo.hStdOutput = stdout_write
    startup.StartupInfo.hStdError = stderr_write
    startup.lpAttributeList = ctypes.cast(attribute_list, ctypes.c_void_p)
    process = ProcessInformation()
    flags = 0x00080000 | 0x00000004 | 0x00000400 | 0x08000000
    job = kernel32.CreateJobObjectW(None, None)
    if not job:
        raise SetupError("CreateJobObjectW failed")
    acquired.append({"kind": "job", "open": True, "value": job})
    # Layout of JOBOBJECT_EXTENDED_LIMIT_INFORMATION on 64-bit Windows.
    # LimitFlags is JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE. A rejected call is setup.
    class BasicLimit(ctypes.Structure):
        _fields_ = [
            ("PerProcessUserTimeLimit", ctypes.c_longlong),
            ("PerJobUserTimeLimit", ctypes.c_longlong),
            ("LimitFlags", wintypes.DWORD),
            ("MinimumWorkingSetSize", ctypes.c_size_t),
            ("MaximumWorkingSetSize", ctypes.c_size_t),
            ("ActiveProcessLimit", wintypes.DWORD),
            ("Affinity", ctypes.c_size_t),
            ("PriorityClass", wintypes.DWORD),
            ("SchedulingClass", wintypes.DWORD),
        ]

    class IoCounters(ctypes.Structure):
        _fields_ = [
            ("ReadOperationCount", ctypes.c_ulonglong),
            ("WriteOperationCount", ctypes.c_ulonglong),
            ("OtherOperationCount", ctypes.c_ulonglong),
            ("ReadTransferCount", ctypes.c_ulonglong),
            ("WriteTransferCount", ctypes.c_ulonglong),
            ("OtherTransferCount", ctypes.c_ulonglong),
        ]

    class ExtendedLimit(ctypes.Structure):
        _fields_ = [
            ("BasicLimitInformation", BasicLimit),
            ("IoInfo", IoCounters),
            ("ProcessMemoryLimit", ctypes.c_size_t),
            ("JobMemoryLimit", ctypes.c_size_t),
            ("PeakProcessMemoryUsed", ctypes.c_size_t),
            ("PeakJobMemoryUsed", ctypes.c_size_t),
        ]

    limit = ExtendedLimit()
    limit.BasicLimitInformation.LimitFlags = 0x2000
    limited = kernel32.SetInformationJobObject(job, 9, ctypes.byref(limit), ctypes.sizeof(limit))
    created = kernel32.CreateProcessW(
        None,
        command_buf,
        None,
        None,
        True,
        flags,
        ctypes.cast(environment, ctypes.c_void_p),
        None,
        ctypes.byref(startup),
        ctypes.byref(process),
    )
    if created:
        acquired.append({"kind": "thread", "open": True, "value": process.hThread})
        acquired.append({"kind": "process", "open": True, "value": process.hProcess})
    release_acquired(
        [
            item
            for item in acquired
            if item.get("kind") in ("nul", "stdout_write", "stderr_write") and item.get("open")
        ],
        _close_launch_item,
    )
    if not created or not limited:
        if created:
            kernel32.TerminateProcess(process.hProcess, 1)
        raise SetupError("CreateProcessW or job limit failed")
    if not kernel32.AssignProcessToJobObject(job, process.hProcess):
        kernel32.TerminateProcess(process.hProcess, 1)
        raise SetupError("AssignProcessToJobObject failed")
    chunks = {"out": [], "err": []}

    def drain(handle, key):
        buf = ctypes.create_string_buffer(4096)
        total = 0
        while total < 262144:
            got = wintypes.DWORD()
            ok = kernel32.ReadFile(handle, buf, 4096, ctypes.byref(got), None)
            if not ok or got.value == 0:
                break
            chunks[key].append(buf.raw[: got.value])
            total += got.value

    readers = [
        threading.Thread(target=drain, args=(stdout_read, "out")),
        threading.Thread(target=drain, args=(stderr_read, "err")),
    ]
    for reader in readers:
        reader.start()
    kernel32.ResumeThread(process.hThread)
    waited = kernel32.WaitForSingleObject(process.hProcess, int(timeout * 1000))
    if waited != 0:
        kernel32.TerminateJobObject(job, 1)
        kernel32.WaitForSingleObject(process.hProcess, 5000)
    for reader in readers:
        reader.join(timeout=5)
    exit_code = wintypes.DWORD()
    kernel32.GetExitCodeProcess(process.hProcess, ctypes.byref(exit_code))
    class Accounting(ctypes.Structure):
        _fields_ = [
            ("TotalUserTime", ctypes.c_longlong),
            ("TotalKernelTime", ctypes.c_longlong),
            ("ThisPeriodTotalUserTime", ctypes.c_longlong),
            ("ThisPeriodTotalKernelTime", ctypes.c_longlong),
            ("TotalPageFaultCount", wintypes.DWORD),
            ("TotalProcesses", wintypes.DWORD),
            ("ActiveProcesses", wintypes.DWORD),
            ("TotalTerminatedProcesses", wintypes.DWORD),
        ]
    accounting = Accounting()
    returned = wintypes.DWORD()
    total = None
    if kernel32.QueryInformationJobObject(
        job, 1, ctypes.byref(accounting), ctypes.sizeof(accounting), ctypes.byref(returned)
    ):
        total = int(accounting.TotalProcesses)
    stdout = b"".join(chunks["out"])
    stderr = b"".join(chunks["err"])
    return {
        "pid": int(process.dwProcessId),
        "exit": int(exit_code.value),
        "job_total_processes": total,
        "stdout": stdout,
        "stderr": stderr,
        "truncated": len(stdout) >= 262144 or len(stderr) >= 65536,
    }


def signature_preflight(work):
    import urllib.request
    import zipfile

    print("cosign release " + COSIGN_RELEASE)
    version = _run_command(["cosign", "version"], 30)
    if version["exit"] != 0 or not cosign_version_ok(version["stdout"] + version["stderr"]):
        raise SetupError("pinned cosign is absent")
    work.mkdir(parents=True, exist_ok=True)
    names = ["checksums.txt", "checksums.txt.sigstore.json", ARCHIVE_NAME]
    paths = {}
    for name in names:
        url = "https://github.com/" + REPOSITORY + "/releases/download/" + RELEASE_TAG + "/" + name
        if not url.startswith("https://"):
            raise SetupError("release URL rejected")
        destination = work / name
        try:
            with urllib.request.urlopen(url, timeout=60) as response:
                body, over = read_bounded_http(response, RELEASE_DOWNLOAD_BYTES)
            if over:
                raise SetupError("release download exceeded the acquisition ceiling")
            destination.write_bytes(body)
        except OSError as exc:
            raise SetupError("release download failed") from exc
        paths[name] = destination
    verified = _run_command(
        [
            "cosign",
            "verify-blob",
            "--bundle",
            str(paths["checksums.txt.sigstore.json"]),
            "--certificate-identity",
            CERTIFICATE_IDENTITY,
            "--certificate-oidc-issuer",
            CERTIFICATE_OIDC_ISSUER,
            str(paths["checksums.txt"]),
        ],
        120,
    )
    if verified["exit"] != 0:
        raise SetupError("signed checksum verification failed")
    digest = checksum_digest(paths["checksums.txt"].read_text(encoding="utf-8"), ARCHIVE_NAME)
    actual = sha256_file(paths[ARCHIVE_NAME])
    if digest is None or digest != actual:
        raise SetupError("archive digest mismatch")
    with zipfile.ZipFile(paths[ARCHIVE_NAME]) as archive:
        assay = extract_bounded_archive(archive, work / "unpacked", ARCHIVE_DECODE_BYTES)
    return {
        "verified": True,
        "connected": True,
        "offline_arm": False,
        "archive_digest_ok": True,
        "identity": CERTIFICATE_IDENTITY,
        "issuer": CERTIFICATE_OIDC_ISSUER,
        "assay": assay,
    }


def run_verifier(assay, bundle, sid=None, capability_sid=None, acquired=None):
    argv = [
        str(assay),
        "evidence",
        "verify-privileged-mcp-action",
        str(bundle),
        "--format",
        "json",
    ]
    if sid is None:
        result = _run_command(argv, 60, env=child_environment(os.environ))
        return {
            "exit": result["exit"],
            "stdout": result["stdout"],
            "stderr": result["stderr"],
            "truncated": result["truncated"],
        }
    launched = launch_in_profile(
        sid, capability_sid, argv, child_environment(os.environ), 60, acquired
    )
    # Distinct contract: these bytes were already counted by ReadFile on the
    # handles launch_in_profile created. Their deadline is WaitForSingleObject
    # and their cleanup is the acquired ledger, not the pipe supervisor.
    stdout, truncated = _bounded(launched["stdout"], 262144)
    stderr, stderr_truncated = _bounded(launched["stderr"], 65536)
    return {
        "exit": launched["exit"],
        "stdout": stdout,
        "stderr": stderr,
        "truncated": truncated or stderr_truncated or launched["truncated"],
        "pid": launched["pid"],
    }


def release_acquired(items, close):
    if not isinstance(items, list):
        return {"open": [], "failed": ["acquired"]}
    failed = []
    for item in reversed(items):
        if not isinstance(item, dict) or not item.get("open"):
            continue
        try:
            outcome = close(item)
        except Exception:
            item["release"] = {"invoked": False, "verified_absent": None}
            failed.append(item.get("kind"))
            continue
        if isinstance(outcome, dict) and "verified_absent" in outcome:
            verified = outcome.get("verified_absent")
            invoked = outcome.get("invoked") is True
            item["release"] = {"invoked": invoked, "verified_absent": verified}
            if invoked and verified is not False:
                item["open"] = False
            else:
                failed.append(item.get("kind"))
        elif outcome is not False:
            item["open"] = False
        else:
            failed.append(item.get("kind"))
    return {
        "failed": failed,
        "open": [item.get("kind") for item in items if isinstance(item, dict) and item.get("open")],
    }


def cleanup(state):
    steps = {
        "job_closed": False,
        "aces_revoked": False,
        "profile_delete_attempted": False,
        "audit_restored": False,
        "storage_absent": False,
        "no_container_sid_ace": False,
        "audit_matches_prior": False,
    }
    unknown = False
    releases = []
    acquired = state.get("acquired") if isinstance(state, dict) else None
    if not isinstance(acquired, list):
        unknown = True
        open_kinds = []
    else:
        closer = state.get("closer")
        if closer is None:
            def closer(_item):
                return False
        release_acquired(acquired, closer)
        open_kinds = [item.get("kind") for item in acquired if isinstance(item, dict) and item.get("open")]
        for item in acquired:
            release = item.get("release") if isinstance(item, dict) else None
            if isinstance(release, dict):
                releases.append(
                    {
                        "kind": item.get("kind"),
                        "invoked": release.get("invoked"),
                        "verified_absent": release.get("verified_absent"),
                    }
                )
        steps["job_closed"] = "job" not in open_kinds
        if open_kinds:
            unknown = True
    try:
        if state.get("profile") and state.get("grants") is not None:
            outcomes = revoke_paths(state["profile"]["sid"], state["grants"])
            steps["aces_revoked"] = all(outcomes) if outcomes or state["grants"] == [] else False
            steps["no_container_sid_ace"] = steps["aces_revoked"]
        else:
            steps["aces_revoked"] = True
            steps["no_container_sid_ace"] = True
    except (OSError, SetupError, TimeoutError):
        unknown = True
    try:
        if state.get("profile"):
            deleted = delete_profile(state["profile"]["name"])
            steps["profile_delete_attempted"] = True
            folder = state["profile"].get("folder")
            if not deleted or not folder:
                unknown = True
            else:
                steps["storage_absent"] = not Path(folder).exists()
        else:
            steps["profile_delete_attempted"] = True
            steps["storage_absent"] = True
    except (OSError, SetupError):
        unknown = True
    try:
        if state.get("audit_prior") is not None:
            steps["audit_restored"] = restore_audit(state["audit_prior"])
            current, _text = read_audit()
            steps["audit_matches_prior"] = current == state["audit_prior"]
        else:
            unknown = True
    except (OSError, SetupError, TimeoutError):
        unknown = True
    if unknown or any(value is False for value in steps.values()):
        status = "unknown" if unknown or any(value is None for value in steps.values()) else "dirty"
        if unknown:
            status = "unknown"
        elif any(value is False for value in steps.values()):
            status = "dirty"
    else:
        status = "clean"
    return {"status": status, "steps": steps, "open": open_kinds, "releases": releases}


def _listeners():
    import socket
    import threading

    tcp = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    tcp.bind(("127.0.0.1", 0))
    tcp.listen(8)
    tcp.settimeout(0.5)
    udp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    udp.bind(("127.0.0.1", 0))
    udp.settimeout(0.5)
    stop = {"flag": False}

    def accept_loop():
        while not stop["flag"]:
            try:
                conn, _addr = tcp.accept()
            except OSError:
                continue
            conn.close()

    def echo_loop():
        while not stop["flag"]:
            try:
                payload, addr = udp.recvfrom(512)
            except OSError:
                continue
            try:
                udp.sendto(payload, addr)
            except OSError:
                continue

    threads = [
        threading.Thread(target=accept_loop, daemon=True),
        threading.Thread(target=echo_loop, daemon=True),
    ]
    for thread in threads:
        thread.start()
    return tcp, udp, stop


def _probe_argv(role, address, loop_port, udp_port, internet):
    return [
        sys.executable,
        str(Path(__file__).resolve()),
        "--probe-role",
        role,
        "--external-address",
        address,
        "--external-port",
        "443",
        "--loopback-address",
        "127.0.0.1",
        "--loopback-port",
        str(loop_port),
        "--udp-port",
        str(udp_port),
        "--internet-client-sid",
        internet,
    ]


def _parse_probe(stdout):
    try:
        parsed = json.loads(stdout)
    except json.JSONDecodeError:
        return None
    return parsed if isinstance(parsed, dict) else None


def run_hosted():
    import socket

    results = Path("results")
    results.mkdir(exist_ok=True)
    (results / "plan.json").write_text(
        json.dumps(plan_document(), indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    state = {
        "audit_prior": None,
        "profile": None,
        "grants": None,
        "acquired": [],
        "closer": _close_launch_item,
    }
    receipts = {
        "profile": {"sid": None, "created_once": False},
        "resolved_external": {},
        "name_resolution": {"result": "not_measurable", "external_query_proven": False},
        "wfp_filters": {"text": "", "truncated": False, "failed": True},
        "event_capture": {"truncated": False, "failed": True},
        "verify": {
            "preflight": {
                "verified": False,
                "connected": True,
                "offline_arm": False,
                "archive_digest_ok": False,
                "identity": CERTIFICATE_IDENTITY,
                "issuer": CERTIFICATE_OIDC_ISSUER,
            }
        },
    }
    setup_error = None
    tcp = udp = None
    stop = {"flag": True}
    try:
        (results / "context.json").write_text(
            json.dumps(record_context(), indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
        prior, prior_text = read_audit()
        state["audit_prior"] = prior
        (results / "audit-prior.txt").write_text(prior_text, encoding="utf-8")
        if prior is None:
            raise SetupError("audit setting unreadable")
        set_audit_failure()
        tcp, udp, stop = _listeners()
        infos = socket.getaddrinfo("github.com", 443, type=socket.SOCK_STREAM)
        address = infos[0][4][0]
        receipts["resolved_external"] = {"address": address, "port": 443}
        loop_port = tcp.getsockname()[1]
        udp_port = udp.getsockname()[1]
        receipts["h_before"] = {
            "legs": {
                "tcp_loopback": run_leg("tcp", "127.0.0.1", loop_port),
                "tcp_external": run_leg("tcp", address, 443),
            }
        }
        profile = create_profile_once()
        state["profile"] = profile
        receipts["profile"] = {"sid": profile["sid"], "created_once": True}
        internet = derive_internet_client_sid()
        grants = []
        state["grants"] = grants
        grant_paths(
            profile["sid"],
            [
                (Path(sys.base_prefix), True),
                (Path(__file__).resolve(), False),
                (Path(__file__).resolve().parent, True),
                (BUNDLE_PATH, False),
                (BUNDLE_PATH.parent, True),
            ],
            grants,
        )
        preflight = signature_preflight(results / "preflight")
        receipts["verify"]["preflight"] = {
            key: preflight[key]
            for key in (
                "verified",
                "connected",
                "offline_arm",
                "archive_digest_ok",
                "identity",
                "issuer",
            )
        }
        grant_paths(
            profile["sid"],
            [
                (preflight["assay"].parent, True),
                (preflight["assay"], False),
            ],
            grants,
        )
        env = child_environment(os.environ)
        captures = []

        def adopt(label, role, capability):
            launched = launch_in_profile(
                profile["sid"],
                capability,
                _probe_argv(role, address, loop_port, udp_port, internet),
                env,
                60,
                state["acquired"],
            )
            parsed = _parse_probe(launched["stdout"])
            if parsed is None:
                raise SetupError(label + " receipt missing")
            arm = {
                "pid": launched["pid"],
                "token": parsed.get("token"),
                "legs": parsed.get("legs") or {},
                "job_total_processes": launched["job_total_processes"],
            }
            if role == "c0":
                arm["grandchild"] = parsed.get("grandchild") or {}
                observed = parsed.get("name_resolution")
                if isinstance(observed, dict):
                    observed["external_query_proven"] = False
                    receipts["name_resolution"] = observed
            receipts[label] = arm
            captures.append(_attach_events(arm))

        adopt("c1_before", "c1", internet)
        adopt("c0", "c0", None)
        receipts["verify"]["outside"] = run_verifier(preflight["assay"], BUNDLE_PATH)
        receipts["verify"]["inside"] = run_verifier(
            preflight["assay"],
            BUNDLE_PATH,
            sid=profile["sid"],
            capability_sid=None,
            acquired=state["acquired"],
        )
        adopt("c1_after", "c1", internet)
        receipts["event_capture"] = {
            "truncated": any(item.get("truncated") for item in captures),
            "failed": (not captures) or any(item.get("failed") for item in captures),
        }
        filter_path = results / "wfp-filters.xml"
        try:
            receipts["wfp_filters"] = collect_filter_evidence(
                filter_path,
                _retained_filter_ids(receipts),
            )
        except (OSError, TimeoutError):
            if filter_path.exists():
                filter_path.write_bytes(b"")
            receipts["wfp_filters"] = {"text": "", "truncated": False, "failed": True}
        receipts["h_after"] = {
            "legs": {
                "tcp_loopback": run_leg("tcp", "127.0.0.1", loop_port),
                "tcp_external": run_leg("tcp", address, 443),
            }
        }
    except (SetupError, OSError, TimeoutError, json.JSONDecodeError) as exc:
        setup_error = str(exc)[:300]
    finally:
        stop["flag"] = True
        if tcp is not None:
            tcp.close()
        if udp is not None:
            udp.close()
        try:
            receipts["cleanup"] = cleanup(state)
        except Exception as exc:  # noqa: BLE001 - cleanup must not raise
            receipts["cleanup"] = {"status": "unknown", "error": str(exc)[:300]}
    if setup_error:
        receipts["setup_error"] = setup_error
    (results / "receipts.json").write_text(
        json.dumps(receipts, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    verdict = evaluate(receipts)
    (results / "verdict.json").write_text(
        json.dumps(verdict, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    (results / "cleanup.json").write_text(
        json.dumps(receipts.get("cleanup"), indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    if verdict["completed"]:
        return 0
    if verdict["verdict"] == "MECHANISM_FAILS":
        return 3
    if verdict["verdict"] == "SETUP":
        return 4
    return 2


def main(argv):
    if "--self-test" in argv:
        return self_test()
    if sys.platform != "win32":
        print(
            "Windows host experiment was not executed: this interpreter is not Windows.",
            file=sys.stderr,
        )
        return 4
    if "--probe-role" in argv:
        return probe_role(argv)
    return run_hosted()


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
