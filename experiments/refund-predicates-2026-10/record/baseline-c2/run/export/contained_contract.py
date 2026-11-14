"""Shared pure contained execution contract. No process, filesystem or engine imports."""
from __future__ import annotations

OUTPUT_CAP_BYTES = 4 * 1024 * 1024


class ContractError(ValueError):
    """Finite predicate tag plus legacy diagnostic text; never classify the text."""

    def __init__(self, message, *, code):
        self.code = code
        super().__init__(message)


HEX64 = frozenset("0123456789abcdef")


CONTAINED_USER = "65532:65532"


TMPFS_BYTES = 1048576


TMPFS_INODES = 128


MEMORY_4G = 4 * 1024 * 1024 * 1024


DECLARED_CEILINGS = {
    "deadline_seconds": 8,
    "disk_bytes": TMPFS_BYTES,
    "file_count": TMPFS_INODES,
    "output_bytes": OUTPUT_CAP_BYTES,
}


RESOURCE_PROFILE_SCHEMA = "corpus-adequacy.aee-checker-sealed.resource-profile.v1"


RESOURCE_PROFILE_V2_SCHEMA = "corpus-adequacy.aee-checker-sealed.resource-profile.v2"


CPU_PERIOD_USEC = 100000


MILLICPU_PER_CPU = 1000


_INT64_MAX = 2 ** 63 - 1


MAX_CPU_RATE_MILLICPU = _INT64_MAX * MILLICPU_PER_CPU // CPU_PERIOD_USEC


MAX_NOFILE = _INT64_MAX


RESOURCE_PROFILE_KEYS = (
    "schema", "work_bytes", "tmp_bytes", "work_inodes", "tmp_inodes",
    "work_exec", "deadline_seconds", "output_bytes", "memory_bytes",
    "memory_swap_bytes", "pids",
)


RESOURCE_PROFILE_V2_KEYS = RESOURCE_PROFILE_KEYS + (
    "cpu_rate_millicpu", "nofile_soft", "nofile_hard",
)


DEFAULT_MOUNT_SPEC = (
    ("input", "/input"),
    ("vendor", "/vendor"),
    ("tool", "/tool"),
)


def exact_object(doc, keys, where: str) -> None:
    if type(doc) is not dict:
        raise ContractError("%s must be an object" % where, code='object-shape')
    want, got = set(keys), set(doc)
    if got != want:
        raise ContractError(
            "%s exact keys missing=%s unknown=%s" % (
                where, sorted(want - got), sorted(got - want)), code='object-shape')


def _resource_profile(*, work_bytes, tmp_bytes, work_inodes, tmp_inodes,
                      work_exec, deadline_seconds, output_bytes, memory_bytes,
                      memory_swap_bytes, pids) -> dict:
    return {
        "schema": RESOURCE_PROFILE_SCHEMA,
        "work_bytes": work_bytes,
        "tmp_bytes": tmp_bytes,
        "work_inodes": work_inodes,
        "tmp_inodes": tmp_inodes,
        "work_exec": work_exec,
        "deadline_seconds": deadline_seconds,
        "output_bytes": output_bytes,
        "memory_bytes": memory_bytes,
        "memory_swap_bytes": memory_swap_bytes,
        "pids": pids,
    }


INERT_RESOURCE_PROFILE = _resource_profile(
    work_bytes=TMPFS_BYTES,
    tmp_bytes=TMPFS_BYTES,
    work_inodes=TMPFS_INODES,
    tmp_inodes=TMPFS_INODES,
    work_exec=False,
    deadline_seconds=DECLARED_CEILINGS["deadline_seconds"],
    output_bytes=DECLARED_CEILINGS["output_bytes"],
    memory_bytes=MEMORY_4G,
    memory_swap_bytes=MEMORY_4G,
    pids=512,
)


CANDIDATE_RESOURCE_PROFILE = _resource_profile(
    work_bytes=256 * 1024 * 1024,
    tmp_bytes=16 * 1024 * 1024,
    work_inodes=16384,
    tmp_inodes=2048,
    work_exec=True,
    deadline_seconds=120,
    output_bytes=DECLARED_CEILINGS["output_bytes"],
    memory_bytes=MEMORY_4G,
    memory_swap_bytes=MEMORY_4G,
    pids=512,
)


CANDIDATE_RESOURCE_PROFILE_V2 = {
    **CANDIDATE_RESOURCE_PROFILE,
    "schema": RESOURCE_PROFILE_V2_SCHEMA,
    # Operator policy, not measured tuning and not a cumulative CPU-seconds budget:
    # one CPU of aggregate cgroup rate, and a per-process descriptor limit.
    "cpu_rate_millicpu": 1 * MILLICPU_PER_CPU,
    "nofile_soft": 1024,
    "nofile_hard": 1024,
}


def _require_positive_int(profile, key) -> int:
    """`type(...) is not int` rather than isinstance: bool is an int subclass, and a profile
    carrying True where a count belongs must refuse rather than be read as 1."""
    value = profile[key]
    if type(value) is not int or value <= 0:
        raise ContractError("resource profile %s" % key, code='resource-profile')
    return value


def _require_cpu_and_nofile(profile) -> None:
    """v2-only policy. Ints only, so no float, NaN, infinity or bool can reach the argv, and
    finite, so an unrepresentable rate refuses here instead of being encoded."""
    rate = _require_positive_int(profile, "cpu_rate_millicpu")
    if rate * CPU_PERIOD_USEC % MILLICPU_PER_CPU:
        raise ContractError("resource profile cpu_rate_millicpu is not exactly representable", code='cpu-rate')
    if rate > MAX_CPU_RATE_MILLICPU:
        raise ContractError("resource profile cpu_rate_millicpu exceeds the representable ceiling", code='cpu-rate')
    soft = _require_positive_int(profile, "nofile_soft")
    hard = _require_positive_int(profile, "nofile_hard")
    if soft > hard:
        raise ContractError("resource profile nofile_soft exceeds nofile_hard", code='nofile')
    if hard > MAX_NOFILE:
        raise ContractError("resource profile nofile_hard exceeds the representable ceiling", code='nofile')


_PROFILE_POLICIES = {
    RESOURCE_PROFILE_SCHEMA: (RESOURCE_PROFILE_KEYS, None),
    RESOURCE_PROFILE_V2_SCHEMA: (RESOURCE_PROFILE_V2_KEYS, _require_cpu_and_nofile),
}


def _require_profile(profile, *, schema: str) -> dict:
    keys, extra = _PROFILE_POLICIES[schema]
    exact_object(profile, keys, "resource profile")
    if profile.get("schema") != schema:
        raise ContractError("resource profile schema", code='resource-profile')
    for key in keys:
        if key in ("schema", "work_exec"):
            continue
        _require_positive_int(profile, key)
    if type(profile["work_exec"]) is not bool:
        raise ContractError("resource profile work_exec", code='resource-profile')
    if profile["output_bytes"] != OUTPUT_CAP_BYTES:
        raise ContractError("resource profile output_bytes is not enforced", code='resource-profile')
    if extra is not None:
        extra(profile)
    return dict(profile)


def require_resource_profile(profile) -> dict:
    """The v1 loader, unchanged in meaning: it admits v1 and refuses v2 by exact keys."""
    return _require_profile(profile, schema=RESOURCE_PROFILE_SCHEMA)


def require_resource_profile_v2(profile) -> dict:
    return _require_profile(profile, schema=RESOURCE_PROFILE_V2_SCHEMA)


def require_versioned_resource_profile(profile) -> dict:
    """Admit a v1 or a v2 profile, selected by its own schema field, through the one shared rule.

    For the container funnel only, which serves both versions. Which version a run may use is
    decided upstream by the resolved execution profile; this selects the validator, not the policy.
    """
    schema = profile.get("schema") if type(profile) is dict else None
    if type(schema) is not str or schema not in _PROFILE_POLICIES:
        raise ContractError("resource profile schema", code='resource-profile')
    return _require_profile(profile, schema=schema)


def cpu_quota_usec(profile) -> int:
    """The one mapping from a v2 rate to the CFS quota, shared by the argv and the comparator."""
    checked = require_resource_profile_v2(profile)
    return checked["cpu_rate_millicpu"] * CPU_PERIOD_USEC // MILLICPU_PER_CPU


def validate_mount_destinations(
    destinations, *, strictly_sorted: bool = False
) -> tuple[str, ...]:
    """Validate a sequence of mount destinations.

    Destinations must be a non-empty sequence of non-empty absolute path strings,
    with no duplicates. When strictly_sorted is True, items must be strictly increasing.
    """
    if type(destinations) not in (list, tuple) or not destinations:
        raise ContractError("mount specification", code='mount')
    seen = set()
    prev = None
    res = []
    for dest in destinations:
        if not isinstance(dest, str) or not dest or not dest.startswith("/"):
            raise ContractError("mount specification", code='mount')
        if dest in seen:
            raise ContractError("mount specification", code='mount')
        if strictly_sorted:
            if prev is not None and dest <= prev:
                raise ContractError("mount specification", code='mount')
            prev = dest
        seen.add(dest)
        res.append(dest)
    return tuple(res)


def _require_mount_spec(mount_spec) -> tuple[tuple[str, str], ...]:
    if type(mount_spec) not in (list, tuple) or not mount_spec:
        raise ContractError("mount specification", code='mount')
    normalized = []
    keys = set()
    destinations = []
    for item in mount_spec:
        if type(item) not in (list, tuple) or len(item) != 2:
            raise ContractError("mount specification", code='mount')
        key, destination = item
        if not isinstance(key, str) or not key:
            raise ContractError("mount specification", code='mount')
        if key in keys:
            raise ContractError("mount specification", code='mount')
        keys.add(key)
        destinations.append(destination)
        normalized.append((key, destination))
    validate_mount_destinations(destinations, strictly_sorted=False)
    return tuple(normalized)


def contained_user_ids(value=None) -> tuple[int, int]:
    """Parse the one canonical Docker uid:gid declaration without normalization."""
    if value is None:
        value = CONTAINED_USER
    if not isinstance(value, str) or value.count(":") != 1:
        raise ContractError("user", code='tmpfs')
    values = value.split(":")
    parsed = []
    for item in values:
        if (not item or not item.isascii() or not item.isdecimal() or
                (len(item) > 1 and item.startswith("0"))):
            raise ContractError("user", code='tmpfs')
        number = int(item)
        if not 0 <= number <= 0xffffffff:
            raise ContractError("user", code='tmpfs')
        parsed.append(number)
    return parsed[0], parsed[1]


def _canonical_uint(value: str) -> int:
    if (not value or not value.isascii() or not value.isdecimal() or
            (len(value) > 1 and value.startswith("0"))):
        raise ContractError("tmpfs", code='tmpfs')
    number = int(value)
    if number <= 0:
        raise ContractError("tmpfs", code='tmpfs')
    return number


def parse_tmpfs_options(value, *, owner_bound: bool, user=None) -> dict:
    """Parse a closed Docker tmpfs declaration; v2 additionally binds its owner."""
    if not isinstance(value, str) or type(owner_bound) is not bool:
        raise ContractError("tmpfs", code='tmpfs')
    flags, values = set(), {}
    for part in value.split(","):
        if not part:
            raise ContractError("tmpfs", code='tmpfs')
        if "=" not in part:
            if part not in {"rw", "exec", "noexec", "nosuid", "nodev"} or part in flags:
                raise ContractError("tmpfs", code='tmpfs')
            flags.add(part)
            continue
        key, raw = part.split("=", 1)
        if key not in {"mode", "uid", "gid", "size", "nr_inodes"} or key in values:
            raise ContractError("tmpfs", code='tmpfs')
        values[key] = raw
    if owner_bound and "rw" not in flags:
        raise ContractError("tmpfs", code='tmpfs')
    if "exec" in flags and "noexec" in flags:
        raise ContractError("tmpfs", code='tmpfs')
    required = {"size", "nr_inodes"}
    if owner_bound:
        required |= {"mode", "uid", "gid"}
        if flags & {"nosuid", "nodev"} or ("exec" in flags) == ("noexec" in flags):
            raise ContractError("tmpfs", code='tmpfs')
    elif "uid" in values or "gid" in values:
        raise ContractError("tmpfs", code='tmpfs')
    if ((owner_bound and set(values) != required) or
            (not owner_bound and (
                not {"size", "nr_inodes"} <= set(values) or
                set(values) - {"size", "nr_inodes", "mode"})) or
            owner_bound and values["mode"] != "1777"):
        raise ContractError("tmpfs", code='tmpfs')
    parsed = {
        "exec": "exec" in flags,
        "mode": values.get("mode", "1777"),
        "nr_inodes": _canonical_uint(values["nr_inodes"]),
        "rw": True,
        "size": _canonical_uint(values["size"]),
    }
    if owner_bound:
        uid, gid = contained_user_ids(user)
        if values["uid"] != str(uid) or values["gid"] != str(gid):
            raise ContractError("tmpfs", code='tmpfs')
        parsed.update(uid=uid, gid=gid)
    return parsed


def tmpfs_request(profile: dict, *, destination: str, owner_bound: bool, user=None) -> dict:
    profile = require_versioned_resource_profile(profile)
    if destination not in ("/tmp", "/work"):
        raise ContractError("tmpfs", code='tmpfs')
    prefix = "tmp" if destination == "/tmp" else "work"
    result = {
        "exec": False if destination == "/tmp" else profile["work_exec"],
        "mode": "1777", "nr_inodes": profile[prefix + "_inodes"],
        "rw": True, "size": profile[prefix + "_bytes"],
    }
    if owner_bound:
        uid, gid = contained_user_ids(user)
        result.update(uid=uid, gid=gid)
    return result


def require_tmpfs_spec(spec, *, owner_bound: bool, user=None) -> dict:
    """Validate the exact structured tmpfs value shared by writers and readers."""
    keys = {"exec", "mode", "nr_inodes", "rw", "size"}
    if owner_bound:
        keys |= {"uid", "gid"}
    if (type(spec) is not dict or set(spec) != keys or
            type(spec["rw"]) is not bool or spec["rw"] is not True or
            type(spec["exec"]) is not bool or
            type(spec["mode"]) is not str or spec["mode"] != "1777"):
        raise ContractError("tmpfs", code='tmpfs')
    for key in ("size", "nr_inodes"):
        if type(spec[key]) is not int or spec[key] <= 0:
            raise ContractError("tmpfs", code='tmpfs')
    if owner_bound:
        uid, gid = contained_user_ids(user)
        if (type(spec["uid"]) is not int or type(spec["gid"]) is not int or
                spec["uid"] != uid or spec["gid"] != gid):
            raise ContractError("tmpfs", code='tmpfs')
    return spec


def require_image_id(value: str) -> str:
    if not isinstance(value, str) or not value.startswith("sha256:"):
        raise ContractError("image id must be sha256:<64hex>", code='image')
    digest = value[7:]
    if len(digest) != 64 or any(ch not in HEX64 for ch in digest):
        raise ContractError("image id must be sha256:<64hex>", code='image')
    return value
