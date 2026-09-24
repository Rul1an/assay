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
- internetClient's capability SID is S-1-15-3-1; a name is recorded only when
  the token SID equals the SID DeriveCapabilitySidsFromName returns.
- Event 5157 Direction is the documented word Outbound or Inbound. The
  official sample XML contains %%14592 and does not define that token, so the
  token is retained and does not establish outbound. FilterRTID is the
  documented Filter Run-Time ID. netsh wfp show filters is the documented
  filter-file command. The retained file keeps only item elements whose
  filterId is cited by a retained event. A larger dump, a failed capture, or
  a cited id that is absent is not completed proof.
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
    fields = {}
    for node in root.iter():
        name = _local(node.tag)
        if name == "EventID" and node.text:
            event_id = node.text.strip()
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
        "time": when.isoformat(),
    }


def records_for_leg(events, leg, pid):
    if not isinstance(events, list):
        return []
    kept = []
    for event in events:
        if event_matches(event, leg, pid):
            kept.append(event)
        if len(kept) >= 40:
            break
    return kept


def event_matches(event, leg, pid):
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
    if event.get("direction") != "outbound":
        return False
    start = parse_time(leg.get("start"))
    end = parse_time(leg.get("end"))
    moment = parse_time(event.get("time"))
    if start is None or end is None or moment is None:
        return False
    opened, closed = window_bounds(start, end)
    return opened <= moment <= closed


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
    return str(event.get("filter_runtime_id") or "").isdigit()


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


def select_filter_evidence(xml_text, runtime_ids):
    wanted = []
    for item in runtime_ids or []:
        text = str(item)
        if text.isdigit() and text not in wanted:
            wanted.append(text)
    if not isinstance(xml_text, str):
        return {"text": "", "truncated": False, "failed": True}
    if len(xml_text.encode("utf-8")) > 65536:
        return {"text": "", "truncated": True, "failed": False}
    if not wanted:
        return {"text": "", "truncated": False, "failed": True}
    try:
        root = ET.fromstring(xml_text)
    except ET.ParseError:
        return {"text": "", "truncated": False, "failed": True}
    kept = []
    seen = set()
    for node in root.iter():
        if _local(node.tag) != "item":
            continue
        filter_id = None
        for child in node.iter():
            if _local(child.tag) != "filterId" or not isinstance(child.text, str):
                continue
            candidate = child.text.strip()
            if candidate.isdigit():
                filter_id = candidate
                break
        if filter_id in wanted and filter_id not in seen:
            kept.append(ET.tostring(node, encoding="unicode"))
            seen.add(filter_id)
    if seen != set(wanted):
        return {"text": "", "truncated": False, "failed": True}
    body = "<filters>" + "".join(kept) + "</filters>"
    if len(body.encode("utf-8")) > 65536:
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
        or len(text) > 65536
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


def _run_command(argv, timeout, env=None):
    import subprocess

    completed = subprocess.run(
        argv,
        capture_output=True,
        timeout=timeout,
        env=env,
        check=False,
    )
    stdout, truncated = _bounded(completed.stdout, 262144)
    stderr, stderr_truncated = _bounded(completed.stderr, 65536)
    return {
        "exit": completed.returncode,
        "stdout": stdout,
        "stderr": stderr,
        "truncated": truncated or stderr_truncated,
    }


def _powershell(script, env, timeout):
    import subprocess

    completed = subprocess.run(
        ["powershell.exe", "-NoProfile", "-NonInteractive", "-Command", script],
        capture_output=True,
        timeout=timeout,
        env=env,
        check=False,
    )
    stdout, truncated = _bounded(completed.stdout, 32768)
    return completed.returncode, stdout, truncated


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


def collect_events(leg, pid):
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
    try:
        code, stdout, truncated = _powershell(script, env, 30)
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
            parsed.append(event)
    return records_for_leg(parsed, leg, pid), truncated, False


def collect_filter_evidence(destination, runtime_ids):
    destination.parent.mkdir(parents=True, exist_ok=True)
    shown = _run_command(
        ["netsh", "wfp", "show", "filters", "file=" + str(destination)],
        60,
    )
    if shown["exit"] != 0 or not destination.is_file():
        return {"text": "", "truncated": False, "failed": True}
    if destination.stat().st_size > 65536:
        destination.write_bytes(b"")
        return {"text": "", "truncated": True, "failed": False}
    selected = select_filter_evidence(
        destination.read_text(encoding="utf-8", errors="replace"),
        runtime_ids,
    )
    if selected["failed"] or selected["truncated"] or not selected["text"]:
        destination.write_bytes(b"")
    else:
        destination.write_text(selected["text"], encoding="utf-8")
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
        import subprocess

        grandchild = subprocess.run(
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
            capture_output=True,
            timeout=20,
            env=child_environment(os.environ),
            check=False,
        )
        text, _truncated = _bounded(grandchild.stdout, 65536)
        try:
            body["grandchild"] = json.loads(text)
        except json.JSONDecodeError:
            body["grandchild"] = {"spawn": "inherited", "parse_error": True}
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


def derive_internet_client_sid():
    ctypes, wintypes, kernel32, advapi32, userenv, _ole32 = _load_win32()
    group_sids = ctypes.c_void_p()
    group_count = wintypes.DWORD()
    cap_sids = ctypes.c_void_p()
    cap_count = wintypes.DWORD()
    hr = userenv.DeriveCapabilitySidsFromName(
        "internetClient",
        ctypes.byref(group_sids),
        ctypes.byref(group_count),
        ctypes.byref(cap_sids),
        ctypes.byref(cap_count),
    )
    if hr & 0xFFFFFFFF >= 0x80000000 or cap_count.value < 1:
        raise SetupError("DeriveCapabilitySidsFromName failed")
    first = ctypes.cast(cap_sids, ctypes.POINTER(ctypes.c_void_p))[0]
    text = ctypes.c_wchar_p()
    if not advapi32.ConvertSidToStringSidW(first, ctypes.byref(text)):
        raise SetupError("capability SID conversion failed")
    value = text.value
    kernel32.LocalFree(text)
    if value != INTERNET_CLIENT_SID:
        raise SetupError("internetClient SID was not the documented value")
    return value


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
    _ctypes, _wintypes, kernel32, advapi32, _userenv, _ole32 = _load_win32()
    kind = item.get("kind")
    value = item.get("value")
    if kind in ("app_sid", "cap_sid"):
        return not advapi32.FreeSid(value)
    if kind == "attribute_list":
        return bool(kernel32.DeleteProcThreadAttributeList(value))
    return bool(kernel32.CloseHandle(value))


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
                destination.write_bytes(response.read())
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
    unpacked = work / "unpacked"
    unpacked.mkdir()
    with zipfile.ZipFile(paths[ARCHIVE_NAME]) as archive:
        for info in archive.infolist():
            if not safe_zip_member(info.filename):
                raise SetupError("archive member rejected")
            archive.extract(info, unpacked)
    assay = next(unpacked.rglob("assay.exe"), None)
    if assay is None:
        raise SetupError("assay.exe missing after digest check")
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
            ok = close(item) is not False
        except Exception:
            ok = False
        if ok:
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
    return {"status": status, "steps": steps, "open": open_kinds}


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
