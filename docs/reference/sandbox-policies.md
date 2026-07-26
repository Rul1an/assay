# Sandbox Policies Reference

This page documents the policy shape consumed by `assay sandbox --policy`.
It does not describe the separate MCP policy format consumed by
`assay policy validate`.

## Current schema

```yaml
api_version: "assay/v1"

fs:
  allow:
    - "/opt/example-data"
  deny:
    - "${HOME}/.ssh"

net:
  allow: []
  deny: []
```

All entries under `fs` and `net` are strings. A named policy that is
missing or cannot be parsed is fatal: `assay sandbox` exits 2 with
`E_POLICY_LOAD_FAILED_UNENFORCEABLE` and does not start the child.

`api_version` defaults to `assay/v1`. The current loader deserializes this
field but does not yet reject other values.

The schema also deserializes an `extends` list, but the sandbox runtime does
not resolve or merge those entries. A non-empty list is therefore a fatal
configuration error. `assay sandbox --profile` emits an empty list until pack
resolution is implemented.

Unknown YAML keys are currently ignored by the loader. Do not use that as an
extension mechanism: an ignored key has no enforcement effect.

## Filesystem contract

On Linux with Landlock available, the runtime installs these baseline rules:

- the working directory is readable and executable;
- the scoped Assay temporary directory has full filesystem access;
- required system files are readable;
- required runtime directories are readable and executable.

Each entry in `fs.allow` names an existing file or directory and grants the
Landlock access set used for explicit policy allows. For a directory, Landlock
applies the rule beneath that directory. The current policy shape cannot
express separate read, write, or execute permissions for a custom allow.

Use exact paths:

```yaml
fs:
  allow:
    - "${CWD}/data"
    - "/opt/example-cache"
```

Do not append shell-style glob syntax such as `/**`. The runtime passes the
expanded string to the filesystem and does not implement policy globs. A
nonexistent path does not become an active Landlock rule.

`fs.deny` is used for compatibility checking. Landlock is allowlist-based:
paths not admitted by the baseline or an explicit allow are unavailable.
A deny nested inside an allowed directory cannot be represented. Assay
therefore degrades to audit mode, or exits 2 with `--fail-closed`, rather
than claiming that the nested deny was enforced.

```yaml
# Not enforceable: the allow grants the whole directory while the deny is nested in it.
fs:
  allow:
    - "${HOME}"
  deny:
    - "${HOME}/.ssh"
```

## Path expansion

The filesystem enforcement path expands:

| Token | Meaning |
|---|---|
| `~` at the start of a path | current home directory |
| `${HOME}` | current home directory |
| `${CWD}` | process current directory |
| `${USER}` | current username |

`${TMPDIR}` is not expanded. The scoped Assay temporary directory is already
part of the baseline Landlock rules and should not be added through a literal
`${TMPDIR}` policy entry.

`--workdir` and `${CWD}` are not yet one contract: filesystem expansion reads
the process current directory. Avoid combining a different `--workdir` with a
`${CWD}` policy entry until that gap is closed.

## Network contract

Network policy has no enforcement effect unless both `--enforce` and
`--enforce-net` are supplied and the host supports Landlock network rules.

The enforced form is an allowlist of explicit TCP destination ports:

```yaml
net:
  allow:
    - "443"
    - "tcp:8443"
  deny: []
```

The following forms are rejected by `--enforce-net`:

- hostnames, IP addresses, CIDRs, and wildcards;
- UDP, QUIC, or other non-TCP protocols;
- port ranges and port 0;
- every `net.deny` entry.

Landlock can constrain TCP destination ports, not endpoint identity. Without
`--enforce-net`, `net.allow` and `net.deny` are retained policy data but are not
kernel network enforcement.

## Examples

### Baseline filesystem containment

No explicit filesystem allow is required to read and execute from the working
directory or to use Assay's scoped temporary directory:

```yaml
api_version: "assay/v1"

fs:
  allow: []
  deny:
    - "${HOME}/.ssh"

net:
  allow: []
  deny: []
```

### Writable data directory

An explicit allow grants the current full-access policy permission set beneath
that directory:

```yaml
api_version: "assay/v1"

fs:
  allow:
    - "${CWD}/output"
  deny: []

net:
  allow: []
  deny: []
```

### Enforced HTTPS destination port

Use this with `--enforce --enforce-net` on a compatible Linux host:

```yaml
api_version: "assay/v1"

fs:
  allow: []
  deny: []

net:
  allow:
    - "443"
  deny: []
```

## Checking a sandbox policy

There is not yet a dedicated validation command for this sandbox schema.
`assay policy validate` validates the separate MCP policy format.

To exercise sandbox loading with a harmless command:

```bash
assay sandbox --policy my-policy.yaml -- true
```

This confirms that the file parses and passes the runtime's compatibility
checks. On a supported Linux host it also applies the resulting containment
before starting `true`.

Exit code 2 covers configuration that cannot be enforced, including:

- a missing or malformed named policy;
- a Landlock compatibility conflict under `--fail-closed`;
- a network policy that is not expressible under `--enforce-net`.

## Current limitations

- non-empty `extends` is rejected because pack resolution is not implemented;
- Unknown keys are ignored.
- Custom filesystem allows do not carry per-operation permissions.
- Policy globs such as `/**` are not implemented.
- `${TMPDIR}` is not expanded.
- `${CWD}` does not follow a different `--workdir`.
- Network enforcement accepts explicit TCP ports only.
- A host without the required Landlock support cannot provide the Linux
  enforcement described here.
