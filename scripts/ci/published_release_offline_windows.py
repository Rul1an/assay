#!/usr/bin/env python3
"""AppContainer launcher for the published-release offline phase.

Carried from scripts/experiments/windows_appcontainer_probe.py at 46cd95c6
(the feasibility head whose hosted runs passed, including the production
layout). Line ranges below are that file. Diagnostics that identified the
WFP filters (event 5157, auditpol, fwpuclnt, grandchild, cosign download)
are not in this module.

- child_environment 531-543
- valid_sid 527-528
- _dacl_has_sid, _covered_by_inherited_grant, grant_paths, revoke_paths 6091-6143
- read_process_token 6763-6818, without the internetClient rename and with a
  non-container return so the host arm can be read before ResumeThread
- create_profile_once 6915-6945 (alphanumeric moniker; the profile name is
  not a display sentence)
- delete_profile 7002-7010
- _job_process_records 7013-7021 and _read_job_notifications 7025-7047
- _close_launch_item 7050-7063
- resume_suspended and resume_or_terminate 7066-7081
- launch_in_profile and _open_in_profile 7084-7427, plus a host launch that
  omits PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES
- release_acquired 7530-7558
- cleanup 7561-7627, without the auditpol steps at 7628-7636
- _declare_win32 binds for kernel32, advapi32, userenv, and ole32 at 5486-5543
  (kernelbase, wevtapi, and fwpuclnt at 5544-5562 are not bound)

_icacls at 6087 called the experiment's supervise_owned. This module uses one
bounded subprocess instead. A verdict still compares only values this process
recorded: profile SID, capability list, process and job identifiers, token,
exit, wait, and the listener accept count lives in the phase.
"""

from __future__ import annotations

import collections.abc
import ctypes
from datetime import datetime, timezone
import os
from pathlib import Path
import re
import subprocess
import sys
import uuid


INTERNET_CLIENT_SID = "S-1-15-3-1"
ALL_APPLICATION_PACKAGES = "S-1-15-2-1"
_SID_RE = re.compile(r"S-1-\d+(?:-\d+)+")
_JOB_PROCESS_MESSAGES = {6: "NEW_PROCESS", 7: "EXIT_PROCESS"}
PROFILE_REMOVAL_NOTE = (
    "DeleteAppContainerProfile success is not proof the registration is gone"
)

# Logical names, not ctypes.__name__. DWORD and c_size_t alias each other on
# some hosts; the pin compares these names and the applier resolves them.
PROTOTYPES = {
    "advapi32.ConvertSidToStringSidW": ("BOOL", ("c_void_p", "LP_c_wchar_p")),
    "advapi32.ConvertStringSidToSidW": ("BOOL", ("c_wchar_p", "LP_c_void_p")),
    "advapi32.FreeSid": ("c_void_p", ("c_void_p",)),
    "advapi32.GetTokenInformation": ("BOOL", ("c_void_p", "c_int", "c_void_p", "DWORD", "LP_DWORD")),
    "advapi32.OpenProcessToken": ("BOOL", ("c_void_p", "DWORD", "LP_c_void_p")),
    "kernel32.AssignProcessToJobObject": ("BOOL", ("c_void_p", "c_void_p")),
    "kernel32.CloseHandle": ("BOOL", ("c_void_p",)),
    "kernel32.CreateFileW": (
        "c_void_p",
        ("c_wchar_p", "DWORD", "DWORD", "c_void_p", "DWORD", "DWORD", "c_void_p"),
    ),
    "kernel32.CreateIoCompletionPort": ("c_void_p", ("c_void_p", "c_void_p", "c_size_t", "DWORD")),
    "kernel32.CreateJobObjectW": ("c_void_p", ("c_void_p", "c_wchar_p")),
    "kernel32.CreatePipe": ("BOOL", ("LP_c_void_p", "LP_c_void_p", "c_void_p", "DWORD")),
    "kernel32.CreateProcessW": (
        "BOOL",
        (
            "c_wchar_p",
            "c_wchar_p",
            "c_void_p",
            "c_void_p",
            "BOOL",
            "DWORD",
            "c_void_p",
            "c_wchar_p",
            "c_void_p",
            "c_void_p",
        ),
    ),
    "kernel32.DeleteProcThreadAttributeList": ("None", ("c_void_p",)),
    "kernel32.GetCurrentProcess": ("c_void_p", ()),
    "kernel32.GetExitCodeProcess": ("BOOL", ("c_void_p", "LP_DWORD")),
    "kernel32.GetLastError": ("DWORD", ()),
    "kernel32.GetQueuedCompletionStatus": (
        "BOOL",
        ("c_void_p", "LP_DWORD", "LP_c_void_p", "LP_c_void_p", "DWORD"),
    ),
    "kernel32.InitializeProcThreadAttributeList": ("BOOL", ("c_void_p", "DWORD", "DWORD", "LP_c_size_t")),
    "kernel32.LocalFree": ("c_void_p", ("c_void_p",)),
    "kernel32.QueryInformationJobObject": ("BOOL", ("c_void_p", "c_int", "c_void_p", "DWORD", "LP_DWORD")),
    "kernel32.ReadFile": ("BOOL", ("c_void_p", "c_void_p", "DWORD", "LP_DWORD", "c_void_p")),
    "kernel32.ResumeThread": ("DWORD", ("c_void_p",)),
    "kernel32.SetHandleInformation": ("BOOL", ("c_void_p", "DWORD", "DWORD")),
    "kernel32.SetInformationJobObject": ("BOOL", ("c_void_p", "c_int", "c_void_p", "DWORD")),
    "kernel32.TerminateJobObject": ("BOOL", ("c_void_p", "UINT")),
    "kernel32.TerminateProcess": ("BOOL", ("c_void_p", "UINT")),
    "kernel32.UpdateProcThreadAttribute": (
        "BOOL",
        ("c_void_p", "DWORD", "c_size_t", "c_void_p", "c_size_t", "c_void_p", "LP_c_size_t"),
    ),
    "kernel32.WaitForSingleObject": ("DWORD", ("c_void_p", "DWORD")),
    "ole32.CoTaskMemFree": ("None", ("c_void_p",)),
    "userenv.CreateAppContainerProfile": (
        "c_long",
        ("c_wchar_p", "c_wchar_p", "c_wchar_p", "c_void_p", "DWORD", "LP_c_void_p"),
    ),
    "userenv.DeleteAppContainerProfile": ("c_long", ("c_wchar_p",)),
    "userenv.GetAppContainerFolderPath": ("c_long", ("c_wchar_p", "LP_c_wchar_p")),
}


class SetupError(Exception):
    def __init__(self, message: str, last_error: int | None = None) -> None:
        super().__init__(message)
        self.last_error = last_error


class _FakeDll:
    def __init__(self) -> None:
        self._fns: dict[str, object] = {}

    def __getattr__(self, name: str):
        if name not in self._fns:
            self._fns[name] = type("Fn", (), {"restype": None, "argtypes": None})()
        return self._fns[name]


def _prototype_types(ctypes_module, wintypes):
    handle = ctypes_module.c_void_p
    dword = wintypes.DWORD
    size = ctypes_module.c_size_t
    wchar = ctypes_module.c_wchar_p
    return {
        "None": None,
        "BOOL": wintypes.BOOL,
        "DWORD": dword,
        "UINT": wintypes.UINT,
        "c_int": ctypes_module.c_int,
        "c_long": ctypes_module.c_long,
        "c_size_t": size,
        "c_void_p": handle,
        "c_wchar_p": wchar,
        "LP_DWORD": ctypes_module.POINTER(dword),
        "LP_c_size_t": ctypes_module.POINTER(size),
        "LP_c_void_p": ctypes_module.POINTER(handle),
        "LP_c_wchar_p": ctypes_module.POINTER(wchar),
    }


def _declare_win32(ctypes_module, wintypes, kernel32, advapi32, userenv, ole32) -> None:
    """Apply PROTOTYPES. HANDLE stays pointer-sized; the default restype is c_int."""
    types = _prototype_types(ctypes_module, wintypes)
    dlls = {
        "kernel32": kernel32,
        "advapi32": advapi32,
        "userenv": userenv,
        "ole32": ole32,
    }
    for key, (rest, args) in PROTOTYPES.items():
        dll_name, name = key.split(".", 1)
        function = getattr(dlls[dll_name], name)
        function.restype = types[rest]
        function.argtypes = tuple(types[item] for item in args)


def applied_prototype_names() -> dict:
    """Declare on stand-in DLLs and return the table only when every bind matches."""
    from ctypes import wintypes

    dlls = {name: _FakeDll() for name in ("kernel32", "advapi32", "userenv", "ole32")}
    _declare_win32(ctypes, wintypes, dlls["kernel32"], dlls["advapi32"], dlls["userenv"], dlls["ole32"])
    types = _prototype_types(ctypes, wintypes)
    for key, (rest, args) in PROTOTYPES.items():
        dll_name, name = key.split(".", 1)
        function = getattr(dlls[dll_name], name)
        if function.restype is not types[rest]:
            return {key: ("restype", str(rest))}
        if tuple(function.argtypes) != tuple(types[item] for item in args):
            return {key: ("argtypes", args)}
    return dict(PROTOTYPES)


def _load_win32():
    if sys.platform != "win32":
        raise SetupError("Windows DLLs are not loaded on this host")
    from ctypes import wintypes

    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    advapi32 = ctypes.WinDLL("advapi32", use_last_error=True)
    userenv = ctypes.WinDLL("userenv", use_last_error=True)
    ole32 = ctypes.WinDLL("ole32", use_last_error=True)
    _declare_win32(ctypes, wintypes, kernel32, advapi32, userenv, ole32)
    return ctypes, wintypes, kernel32, advapi32, userenv, ole32


def valid_sid(value) -> bool:
    return isinstance(value, str) and bool(_SID_RE.fullmatch(value))


def child_environment(source: collections.abc.Mapping) -> dict[str, str]:
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


def launch_environment(env: object) -> dict[str, str]:
    """The process environment is a mapping, not a dict. An empty block is WinError 10106."""
    if not isinstance(env, collections.abc.Mapping):
        return {}
    return child_environment(env)


def _icacls(args: list[str]) -> dict:
    try:
        completed = subprocess.run(
            ["icacls", *args],
            capture_output=True,
            check=False,
            timeout=30,
        )
    except (OSError, subprocess.TimeoutExpired):
        return {"exit": 1, "stdout": "", "truncated": False}
    stdout = completed.stdout.decode("utf-8", "replace")
    truncated = len(stdout) > 262144
    if truncated:
        stdout = stdout[:262144]
    return {"exit": completed.returncode, "stdout": stdout, "truncated": truncated}


def _dacl_has_sid(path, sid):
    result = _icacls([str(path)])
    if result["exit"] != 0 or result["truncated"]:
        return None
    return sid in result["stdout"]


def _covered_by_inherited_grant(path, paths) -> bool:
    candidate = Path(path)
    for other, directory in paths:
        if directory and Path(other) in candidate.parents:
            return True
    return False


def grant_paths(sid, paths, recorded) -> None:
    if not valid_sid(sid) or sid in (ALL_APPLICATION_PACKAGES, "S-1-15-2-2"):
        raise SetupError("refusing grant for this SID")
    covered = []
    remaining = []
    for path, directory in paths:
        if not directory and _covered_by_inherited_grant(path, paths):
            covered.append(path)
            continue
        remaining.append((path, directory))
    for path in covered:
        recorded.append({"covered": True, "path": str(path)})
    for path, directory in remaining:
        if _dacl_has_sid(path, sid) is not False:
            raise SetupError("SID already present or DACL unreadable")
        rights = "(OI)(CI)(RX)" if directory else "(RX)"
        spec = "*" + sid + ":" + rights
        # Record the host mutation before icacls can fail, so cleanup sees it.
        recorded.append({"path": str(path), "spec": spec})
        result = _icacls([str(path), "/grant", spec])
        if result["exit"] != 0:
            raise SetupError("grant failed")


def grant_read_execute(sid: str, paths: list[str], recorded: list[dict] | None = None) -> list[dict]:
    if recorded is None:
        recorded = []
    grant_paths(sid, [(path, True) for path in paths], recorded)
    return recorded


def revoke(sid: str, recorded: list[dict]) -> tuple[list[bool], list]:
    return revoke_paths(sid, recorded)


def revoke_paths(sid, recorded):
    removals = []
    for item in recorded:
        if item.get("covered"):
            continue
        result = _icacls([item["path"], "/remove:g", "*" + sid])
        removals.append(result["exit"] == 0)
    presence = []
    for item in recorded:
        presence.append(_dacl_has_sid(item["path"], sid))
    return removals, presence


def _parse_pid(value):
    if isinstance(value, bool) or not isinstance(value, int):
        return None
    if value < 0:
        return None
    return value


def _parse_time(value):
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


def _job_process_records(observations):
    records = []
    for message, pid, when in observations:
        name = _JOB_PROCESS_MESSAGES.get(message)
        parsed = _parse_pid(pid)
        if name is None or parsed is None or _parse_time(when) is None:
            continue
        records.append({"message": name, "pid": parsed, "time": when})
    return records


def _read_job_notifications(kernel32, port, now):
    from ctypes import wintypes

    observations = []
    for _ in range(256):
        transferred = wintypes.DWORD()
        key = ctypes.c_void_p()
        overlapped = ctypes.c_void_p()
        ok = kernel32.GetQueuedCompletionStatus(
            port,
            ctypes.byref(transferred),
            ctypes.byref(key),
            ctypes.byref(overlapped),
            0,
        )
        if not ok:
            break
        pid = overlapped.value
        if pid is None:
            continue
        observations.append((int(transferred.value), int(pid) & 0xFFFFFFFF, now()))
    return _job_process_records(observations)


def read_process_token(process, expected_capability_sids: list[str]) -> dict:
    """Harness read of the suspended process. Capability values are SIDs."""
    _ctypes, wintypes, kernel32, advapi32, _userenv, _ole32 = _load_win32()
    token = wintypes.HANDLE()
    if not advapi32.OpenProcessToken(process, 0x0008, ctypes.byref(token)):
        raise SetupError("OpenProcessToken failed", int(kernel32.GetLastError()))
    try:
        is_container = wintypes.DWORD()
        length = wintypes.DWORD()
        if not advapi32.GetTokenInformation(
            token, 29, ctypes.byref(is_container), ctypes.sizeof(is_container), ctypes.byref(length)
        ):
            raise SetupError("TokenIsAppContainer failed", int(kernel32.GetLastError()))
        if not is_container.value:
            return {"capabilities": [], "is_app_container": False, "sid": None}
        needed = wintypes.DWORD()
        advapi32.GetTokenInformation(token, 31, None, 0, ctypes.byref(needed))
        buffer = ctypes.create_string_buffer(max(needed.value, 8))
        if not advapi32.GetTokenInformation(
            token, 31, buffer, ctypes.sizeof(buffer), ctypes.byref(needed)
        ):
            raise SetupError("TokenAppContainerSid failed", int(kernel32.GetLastError()))
        sid_ptr = ctypes.cast(buffer, ctypes.POINTER(ctypes.c_void_p)).contents
        sid_text = ctypes.c_wchar_p()
        if not advapi32.ConvertSidToStringSidW(sid_ptr, ctypes.byref(sid_text)):
            raise SetupError("ConvertSidToStringSidW failed", int(kernel32.GetLastError()))
        app_sid = sid_text.value
        kernel32.LocalFree(sid_text)
        needed = wintypes.DWORD()
        advapi32.GetTokenInformation(token, 30, None, 0, ctypes.byref(needed))
        buffer = ctypes.create_string_buffer(max(needed.value, 4))
        if not advapi32.GetTokenInformation(
            token, 30, buffer, ctypes.sizeof(buffer), ctypes.byref(needed)
        ):
            raise SetupError("TokenCapabilities failed", int(kernel32.GetLastError()))
        count = ctypes.cast(buffer, ctypes.POINTER(wintypes.DWORD)).contents.value

        class SidAttr(ctypes.Structure):
            _fields_ = [("Sid", ctypes.c_void_p), ("Attributes", wintypes.DWORD)]

        base = ctypes.sizeof(wintypes.DWORD)
        if base % ctypes.sizeof(ctypes.c_void_p):
            base += ctypes.sizeof(ctypes.c_void_p) - (base % ctypes.sizeof(ctypes.c_void_p))
        sids = []
        for index in range(count):
            entry = SidAttr.from_buffer(buffer, base + index * ctypes.sizeof(SidAttr))
            text = ctypes.c_wchar_p()
            if advapi32.ConvertSidToStringSidW(entry.Sid, ctypes.byref(text)):
                sids.append(text.value)
                kernel32.LocalFree(text)
        return {"capabilities": sids, "is_app_container": True, "sid": app_sid}
    finally:
        kernel32.CloseHandle(token)


def create_profile_once(state: dict | None = None) -> dict:
    _ctypes, _wintypes, kernel32, advapi32, userenv, ole32 = _load_win32()
    # CreateAppContainerProfile accepts an alphanumeric moniker. Hyphens are rejected.
    name = "a" + format(os.getpid(), "x") + uuid.uuid4().hex[:8]
    sid = ctypes.c_void_p()
    hr = userenv.CreateAppContainerProfile(name, name, "Assay offline phase", None, 0, ctypes.byref(sid))
    code = hr & 0xFFFFFFFF
    if code == 0x800700B7:
        raise SetupError("profile already exists; not reused", code)
    if code >= 0x80000000:
        raise SetupError("CreateAppContainerProfile failed", code)
    # The profile exists from here on. Record it before SID conversion can raise.
    profile = {"folder": None, "name": name, "sid": None}
    if isinstance(state, dict):
        state["profile"] = profile
    text = ctypes.c_wchar_p()
    converted = False
    try:
        if not advapi32.ConvertSidToStringSidW(sid, ctypes.byref(text)):
            raise SetupError("profile SID conversion failed", int(kernel32.GetLastError()))
        converted = True
        sid_text = text.value
        profile["sid"] = sid_text
        path_ptr = ctypes.c_wchar_p()
        folder_hr = userenv.GetAppContainerFolderPath(sid_text, ctypes.byref(path_ptr))
        folder = path_ptr.value if (folder_hr & 0xFFFFFFFF) < 0x80000000 else None
        if path_ptr:
            ole32.CoTaskMemFree(path_ptr)
        profile["folder"] = folder
        return profile
    finally:
        if converted and text:
            kernel32.LocalFree(text)
        advapi32.FreeSid(sid)


def delete_profile(name: str) -> bool:
    _ctypes, _wintypes, _kernel32, _advapi32, userenv, _ole32 = _load_win32()
    last = None
    for _attempt in range(2):
        hr = userenv.DeleteAppContainerProfile(name)
        last = hr & 0xFFFFFFFF
        if last < 0x80000000:
            return True
    return False


def _close_launch_item(item: dict) -> dict:
    _ctypes, _wintypes, kernel32, advapi32, _userenv, _ole32 = _load_win32()
    kind = item.get("kind")
    value = item.get("value")
    if kind in ("app_sid", "cap_sid"):
        # ConvertStringSidToSidW allocates with LocalAlloc. LocalFree releases it.
        return {"invoked": True, "verified_absent": not kernel32.LocalFree(value)}
    if kind == "attribute_list":
        kernel32.DeleteProcThreadAttributeList(value)
        return {"invoked": True, "verified_absent": None}
    return {"invoked": True, "verified_absent": bool(kernel32.CloseHandle(value))}


def release_acquired(items, close) -> dict:
    if not isinstance(items, list):
        return {"failed": ["acquired"], "open": []}
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


def _resume_failed(code) -> bool:
    if isinstance(code, bool) or not isinstance(code, int):
        return True
    return code & 0xFFFFFFFF == 0xFFFFFFFF


def resume_suspended(kernel32, thread, process, before_resume):
    token = None
    if before_resume is not None:
        token = before_resume(process)
    resumed = kernel32.ResumeThread(thread)
    if _resume_failed(resumed):
        raise SetupError("ResumeThread failed", int(kernel32.GetLastError()))
    return token


def read_process_exit_code(kernel32, handle) -> int | None:
    """None when GetExitCodeProcess fails. A failed read is not exit 0."""
    from ctypes import wintypes

    exit_code = wintypes.DWORD()
    if not kernel32.GetExitCodeProcess(handle, ctypes.byref(exit_code)):
        return None
    return int(exit_code.value)


def resume_or_terminate(kernel32, thread, process, before_resume):
    try:
        return resume_suspended(kernel32, thread, process, before_resume)
    except Exception:
        kernel32.TerminateProcess(process, 1)
        raise


def _bad_handle(value) -> bool:
    raw = getattr(value, "value", value)
    if raw in (None, 0, -1):
        return True
    return isinstance(raw, int) and raw & 0xFFFFFFFFFFFFFFFF == 0xFFFFFFFFFFFFFFFF


def _launch_process(profile_sid, capability_sids, argv, env, timeout, acquired, before_resume):
    """One CreateProcessW inside a kill-on-close job.

    capability_sids is None for the host arm (no AppContainer attribute) and a
    list, possibly empty, for a profile launch.
    """
    ctypes_module, wintypes, kernel32, advapi32, _userenv, _ole32 = _load_win32()
    contain = capability_sids is not None
    attribute_count = 2 if contain else 1
    if contain:
        if not valid_sid(profile_sid):
            raise SetupError("launch SID rejected")
        app_sid = ctypes_module.c_void_p()
        if not advapi32.ConvertStringSidToSidW(profile_sid, ctypes_module.byref(app_sid)):
            raise SetupError("ConvertStringSidToSidW failed", int(kernel32.GetLastError()))
        acquired.append({"kind": "app_sid", "open": True, "value": app_sid})

        class SidAttr(ctypes_module.Structure):
            _fields_ = [("Sid", ctypes_module.c_void_p), ("Attributes", wintypes.DWORD)]

        cap_buffer = None
        cap_count = 0
        if capability_sids:
            cap_buffer = (SidAttr * len(capability_sids))()
            for index, capability in enumerate(capability_sids):
                if not valid_sid(capability):
                    raise SetupError("capability SID rejected")
                cap_sid = ctypes_module.c_void_p()
                if not advapi32.ConvertStringSidToSidW(capability, ctypes_module.byref(cap_sid)):
                    raise SetupError("capability conversion failed", int(kernel32.GetLastError()))
                acquired.append({"kind": "cap_sid", "open": True, "value": cap_sid})
                cap_buffer[index] = SidAttr(cap_sid, 0x4)
            cap_count = len(capability_sids)

        class SecurityCapabilities(ctypes_module.Structure):
            _fields_ = [
                ("AppContainerSid", ctypes_module.c_void_p),
                ("Capabilities", ctypes_module.c_void_p),
                ("CapabilityCount", wintypes.DWORD),
                ("Reserved", wintypes.DWORD),
            ]

        security = SecurityCapabilities(
            app_sid,
            ctypes_module.addressof(cap_buffer) if cap_buffer else None,
            cap_count,
            0,
        )
    size = ctypes_module.c_size_t(0)
    kernel32.InitializeProcThreadAttributeList(None, attribute_count, 0, ctypes_module.byref(size))
    attribute_list = ctypes_module.create_string_buffer(size.value)
    if not kernel32.InitializeProcThreadAttributeList(
        attribute_list, attribute_count, 0, ctypes_module.byref(size)
    ):
        raise SetupError("InitializeProcThreadAttributeList failed", int(kernel32.GetLastError()))
    acquired.append({"kind": "attribute_list", "open": True, "value": attribute_list})
    if contain and not kernel32.UpdateProcThreadAttribute(
        attribute_list,
        0,
        0x20009,
        ctypes_module.byref(security),
        ctypes_module.sizeof(security),
        None,
        None,
    ):
        raise SetupError("UpdateProcThreadAttribute failed", int(kernel32.GetLastError()))

    class StartupInfoW(ctypes_module.Structure):
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
            ("lpReserved2", ctypes_module.c_void_p),
            ("hStdInput", wintypes.HANDLE),
            ("hStdOutput", wintypes.HANDLE),
            ("hStdError", wintypes.HANDLE),
        ]

    class StartupInfoExW(ctypes_module.Structure):
        _fields_ = [("StartupInfo", StartupInfoW), ("lpAttributeList", ctypes_module.c_void_p)]

    class ProcessInformation(ctypes_module.Structure):
        _fields_ = [
            ("hProcess", wintypes.HANDLE),
            ("hThread", wintypes.HANDLE),
            ("dwProcessId", wintypes.DWORD),
            ("dwThreadId", wintypes.DWORD),
        ]

    command = subprocess.list2cmdline(argv)
    command_buf = ctypes_module.create_unicode_buffer(command)
    chars: list[str] = []
    for key, value in env.items():
        chars.extend(key + "=" + value)
        chars.append("\0")
    chars.append("\0")
    environment = (ctypes_module.c_wchar * len(chars))(*chars)

    class SecurityAttributes(ctypes_module.Structure):
        _fields_ = [
            ("nLength", wintypes.DWORD),
            ("lpSecurityDescriptor", ctypes_module.c_void_p),
            ("bInheritHandle", wintypes.BOOL),
        ]

    def pipe():
        read = wintypes.HANDLE()
        write = wintypes.HANDLE()
        attributes = SecurityAttributes(ctypes_module.sizeof(SecurityAttributes), None, True)
        if not kernel32.CreatePipe(
            ctypes_module.byref(read), ctypes_module.byref(write), ctypes_module.byref(attributes), 0
        ):
            raise SetupError("CreatePipe failed", int(kernel32.GetLastError()))
        kernel32.SetHandleInformation(read, 1, 0)
        return read, write

    stdout_read, stdout_write = pipe()
    acquired.append({"kind": "stdout_read", "open": True, "value": stdout_read})
    acquired.append({"kind": "stdout_write", "open": True, "value": stdout_write})
    stderr_read, stderr_write = pipe()
    acquired.append({"kind": "stderr_read", "open": True, "value": stderr_read})
    acquired.append({"kind": "stderr_write", "open": True, "value": stderr_write})
    inherit = SecurityAttributes(ctypes_module.sizeof(SecurityAttributes), None, True)
    nul = kernel32.CreateFileW("NUL", 0x80000000, 7, ctypes_module.byref(inherit), 3, 0, None)
    if _bad_handle(nul):
        raise SetupError("NUL open failed", int(kernel32.GetLastError()))
    acquired.append({"kind": "nul", "open": True, "value": nul})
    inherited = (wintypes.HANDLE * 3)(nul, stdout_write, stderr_write)
    if not kernel32.UpdateProcThreadAttribute(
        attribute_list,
        0,
        0x20002,
        ctypes_module.cast(inherited, ctypes_module.c_void_p),
        ctypes_module.sizeof(inherited),
        None,
        None,
    ):
        raise SetupError("handle list rejected", int(kernel32.GetLastError()))
    startup = StartupInfoExW()
    startup.StartupInfo.cb = ctypes_module.sizeof(StartupInfoExW)
    startup.StartupInfo.dwFlags = 0x00000100
    startup.StartupInfo.hStdInput = nul
    startup.StartupInfo.hStdOutput = stdout_write
    startup.StartupInfo.hStdError = stderr_write
    startup.lpAttributeList = ctypes_module.cast(attribute_list, ctypes_module.c_void_p)
    process = ProcessInformation()
    # EXTENDED_STARTUPINFO_PRESENT | CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | CREATE_NO_WINDOW
    flags = 0x00080000 | 0x00000004 | 0x00000400 | 0x08000000
    job = kernel32.CreateJobObjectW(None, None)
    if not job:
        raise SetupError("CreateJobObjectW failed", int(kernel32.GetLastError()))
    acquired.append({"kind": "job", "open": True, "value": job})
    port = kernel32.CreateIoCompletionPort(ctypes_module.c_void_p(-1), None, 0, 1)
    if _bad_handle(port):
        raise SetupError("CreateIoCompletionPort failed", int(kernel32.GetLastError()))
    acquired.append({"kind": "completion_port", "open": True, "value": port})

    class AssociateCompletionPort(ctypes_module.Structure):
        _fields_ = [("CompletionKey", ctypes_module.c_void_p), ("CompletionPort", ctypes_module.c_void_p)]

    association = AssociateCompletionPort(None, port)
    if not kernel32.SetInformationJobObject(
        job, 7, ctypes_module.byref(association), ctypes_module.sizeof(association)
    ):
        raise SetupError("job completion port rejected", int(kernel32.GetLastError()))

    class BasicLimit(ctypes_module.Structure):
        _fields_ = [
            ("PerProcessUserTimeLimit", ctypes_module.c_longlong),
            ("PerJobUserTimeLimit", ctypes_module.c_longlong),
            ("LimitFlags", wintypes.DWORD),
            ("MinimumWorkingSetSize", ctypes_module.c_size_t),
            ("MaximumWorkingSetSize", ctypes_module.c_size_t),
            ("ActiveProcessLimit", wintypes.DWORD),
            ("Affinity", ctypes_module.c_size_t),
            ("PriorityClass", wintypes.DWORD),
            ("SchedulingClass", wintypes.DWORD),
        ]

    class IoCounters(ctypes_module.Structure):
        _fields_ = [
            ("ReadOperationCount", ctypes_module.c_ulonglong),
            ("WriteOperationCount", ctypes_module.c_ulonglong),
            ("OtherOperationCount", ctypes_module.c_ulonglong),
            ("ReadTransferCount", ctypes_module.c_ulonglong),
            ("WriteTransferCount", ctypes_module.c_ulonglong),
            ("OtherTransferCount", ctypes_module.c_ulonglong),
        ]

    class ExtendedLimit(ctypes_module.Structure):
        _fields_ = [
            ("BasicLimitInformation", BasicLimit),
            ("IoInfo", IoCounters),
            ("ProcessMemoryLimit", ctypes_module.c_size_t),
            ("JobMemoryLimit", ctypes_module.c_size_t),
            ("PeakProcessMemoryUsed", ctypes_module.c_size_t),
            ("PeakJobMemoryUsed", ctypes_module.c_size_t),
        ]

    limit = ExtendedLimit()
    limit.BasicLimitInformation.LimitFlags = 0x2000
    limited = kernel32.SetInformationJobObject(
        job, 9, ctypes_module.byref(limit), ctypes_module.sizeof(limit)
    )
    created = kernel32.CreateProcessW(
        None,
        command_buf,
        None,
        None,
        True,
        flags,
        ctypes_module.cast(environment, ctypes_module.c_void_p),
        None,
        ctypes_module.byref(startup),
        ctypes_module.byref(process),
    )
    create_error = int(kernel32.GetLastError()) if not created else None
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
        raise SetupError("CreateProcessW or job limit failed", create_error)
    if not kernel32.AssignProcessToJobObject(job, process.hProcess):
        kernel32.TerminateProcess(process.hProcess, 1)
        raise SetupError("AssignProcessToJobObject failed", int(kernel32.GetLastError()))
    chunks: dict[str, list[bytes]] = {"err": [], "out": []}

    def drain(handle, key):
        buf = ctypes_module.create_string_buffer(4096)
        total = 0
        while total < 262144:
            got = wintypes.DWORD()
            ok = kernel32.ReadFile(handle, buf, 4096, ctypes_module.byref(got), None)
            if not ok or got.value == 0:
                break
            chunks[key].append(buf.raw[: got.value])
            total += got.value

    import threading

    readers = [
        threading.Thread(target=drain, args=(stdout_read, "out")),
        threading.Thread(target=drain, args=(stderr_read, "err")),
    ]
    for reader in readers:
        reader.start()
    try:
        token = resume_or_terminate(kernel32, process.hThread, process.hProcess, before_resume)
    except Exception:
        for reader in readers:
            reader.join(timeout=5)
        raise
    waited = kernel32.WaitForSingleObject(process.hProcess, int(timeout * 1000))
    wait_result = "exited"
    if waited != 0:
        kernel32.TerminateJobObject(job, 1)
        again = kernel32.WaitForSingleObject(process.hProcess, 5000)
        wait_result = "still-running" if again != 0 else "timeout"
    for reader in readers:
        reader.join(timeout=5)
    exit_value = read_process_exit_code(kernel32, process.hProcess)

    class Accounting(ctypes_module.Structure):
        _fields_ = [
            ("TotalUserTime", ctypes_module.c_longlong),
            ("TotalKernelTime", ctypes_module.c_longlong),
            ("ThisPeriodTotalUserTime", ctypes_module.c_longlong),
            ("ThisPeriodTotalKernelTime", ctypes_module.c_longlong),
            ("TotalPageFaultCount", wintypes.DWORD),
            ("TotalProcesses", wintypes.DWORD),
            ("ActiveProcesses", wintypes.DWORD),
            ("TotalTerminatedProcesses", wintypes.DWORD),
        ]

    accounting = Accounting()
    returned = wintypes.DWORD()
    total = None
    if kernel32.QueryInformationJobObject(
        job, 1, ctypes_module.byref(accounting), ctypes_module.sizeof(accounting), ctypes_module.byref(returned)
    ):
        total = int(accounting.TotalProcesses)
    job_processes = _read_job_notifications(
        kernel32, port, lambda: datetime.now(timezone.utc).isoformat()
    )
    stdout = b"".join(chunks["out"])
    stderr = b"".join(chunks["err"])
    if wait_result == "exited" and exit_value == 259:
        wait_result = "still-running"
    return {
        "create_process": True,
        "exit": exit_value,
        "job_processes": job_processes,
        "job_total_processes": total,
        "last_error": None,
        "pid": int(process.dwProcessId),
        "stderr": stderr,
        "stdout": stdout,
        "token": token,
        "truncated": len(stdout) >= 262144 or len(stderr) >= 65536,
        "wait_result": wait_result,
    }


def launch_in_profile(profile_sid, capability_sids, argv, env, timeout, acquired, before_resume):
    if not isinstance(acquired, list):
        acquired = []
    try:
        return _launch_process(
            profile_sid, capability_sids, argv, env, timeout, acquired, before_resume
        )
    finally:
        release_acquired(acquired, _close_launch_item)


def _failed_launch(exc: SetupError) -> dict:
    return {
        "create_process": False,
        "exit": 1,
        "job_processes": [],
        "job_total_processes": None,
        "last_error": exc.last_error,
        "pid": None,
        "stderr": str(exc).encode(),
        "stdout": b"",
        "token": None,
        "truncated": False,
        "wait_result": "exited",
    }


class ProductionLauncher:
    """Profile, grants, and one job per launch. cleanup revokes what prepare recorded."""

    def __init__(self) -> None:
        self.state = {"acquired": [], "grants": [], "launches": [], "profile": None}

    def prepare(self, grant_paths_list: list[str]) -> dict:
        self.state = {
            "acquired": [],
            "closer": _close_launch_item,
            "grants": [],
            "launches": [],
            "profile": None,
        }
        profile = create_profile_once(self.state)
        grant_read_execute(profile["sid"], grant_paths_list, self.state["grants"])
        return self.state

    def launch(self, argv, env, timeout, capabilities):
        acquired: list[dict] = []
        self.state.setdefault("launches", []).append(acquired)
        filtered = launch_environment(env)

        def before_resume(process):
            return read_process_token(process, list(capabilities or []))

        try:
            if capabilities is None:
                return launch_in_profile(None, None, argv, filtered, timeout, acquired, before_resume)
            profile = self.state.get("profile") or {}
            return launch_in_profile(
                profile.get("sid"),
                list(capabilities),
                argv,
                filtered,
                timeout,
                acquired,
                before_resume,
            )
        except SetupError as exc:
            return _failed_launch(exc)

    def terminate_job(self, result) -> None:
        """The launch already called TerminateJobObject before it returned the wait.

        Repeating it here is the phase's explicit deadline call. The job handle
        is closed when launch returns, so a second call has nothing to signal.
        """
        del result

    def cleanup(self, state) -> dict:
        target = state if isinstance(state, dict) and state.get("profile") else self.state
        return cleanup(target)


def cleanup(state) -> dict:
    steps = {
        "aces_revoked": False,
        "job_closed": False,
        "no_container_sid_ace": False,
        "profile_delete_attempted": False,
        "storage_absent": False,
    }
    unknown = False
    releases = []
    open_kinds: list = []
    acquired = state.get("acquired") if isinstance(state, dict) else None
    if not isinstance(acquired, list):
        unknown = True
    groups: list[list] = []
    if isinstance(acquired, list):
        groups.append(acquired)
    launches = state.get("launches") if isinstance(state, dict) else None
    if isinstance(launches, list):
        for group in launches:
            if isinstance(group, list) and all(group is not existing for existing in groups):
                groups.append(group)
    if isinstance(state, dict) and groups:
        closer = state.get("closer") or _close_launch_item
        for group in groups:
            release_acquired(group, closer)
            for item in group:
                if not isinstance(item, dict):
                    continue
                if item.get("open"):
                    open_kinds.append(item.get("kind"))
                release = item.get("release")
                if isinstance(release, dict):
                    releases.append(
                        {
                            "invoked": release.get("invoked"),
                            "kind": item.get("kind"),
                            "verified_absent": release.get("verified_absent"),
                        }
                    )
        steps["job_closed"] = "job" not in open_kinds
        if open_kinds:
            unknown = True
    try:
        profile = state.get("profile") if isinstance(state, dict) else None
        grants = state.get("grants") if isinstance(state, dict) else None
        if profile and grants is not None:
            removals, presence = revoke_paths(profile["sid"], grants)
            granted = [item for item in grants if not item.get("covered")]
            steps["aces_revoked"] = all(removals) if removals or granted == [] else False
            if any(flag is None for flag in presence):
                unknown = True
                steps["no_container_sid_ace"] = None
            elif presence:
                steps["no_container_sid_ace"] = all(flag is False for flag in presence)
            else:
                steps["no_container_sid_ace"] = True
        else:
            steps["aces_revoked"] = True
            steps["no_container_sid_ace"] = True
    except (OSError, SetupError, TimeoutError):
        unknown = True
    try:
        profile = state.get("profile") if isinstance(state, dict) else None
        if profile:
            deleted = delete_profile(profile["name"])
            steps["profile_delete_attempted"] = True
            folder = profile.get("folder")
            if not deleted or not folder:
                unknown = True
            else:
                steps["storage_absent"] = not Path(folder).exists()
        else:
            steps["profile_delete_attempted"] = True
            steps["storage_absent"] = True
    except (OSError, SetupError):
        unknown = True
    if unknown or any(value is None for value in steps.values()):
        status = "unknown"
    elif any(value is False for value in steps.values()):
        status = "dirty"
    else:
        status = "clean"
    return {
        "open": open_kinds,
        "profile_registration_removal": PROFILE_REMOVAL_NOTE,
        "releases": releases,
        "status": status,
        "steps": steps,
    }
