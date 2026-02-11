# ADR-021: Local Pack Discovery and Pack Resolution Order

## Status

Accepted (February 2026)

## Context

The roadmap prioritises open-core work before Enterprise. One option is **local pack discovery**: allow packs to be resolved by name from a well-defined config directory (e.g. `~/.assay/packs/` or XDG-based) so teams and the community can use custom or forked packs without modifying the binary or relying on the remote registry.

Current behaviour: `--pack <ref>` resolves (1) existing file path → load file, (2) built-in name → load embedded pack, (3) else NotFound. There is no resolution from a "local pack directory."

Requirements and constraints:
- **No new pack schema or engine changes** — only loader resolution order and config directory.
- **Security** — local discovery must not introduce path traversal or symlink escape; reference must be constrained to a safe grammar.
- **Override semantics** — built-in names should not be overridable by name (no spoofing); users who want to override must use an explicit path (already supported).
- **Single source of truth** — the normative pack resolution order lives in [SPEC-Pack-Engine-v1](./SPEC-Pack-Engine-v1.md); this ADR records the decision and guardrails.

## Decision

### 1. Pack resolution order (normative)

The **canonical resolution order** is defined in SPEC-Pack-Engine-v1 and implemented in the assay-evidence pack loader. Order:

1. **Path** — If `reference` is an existing filesystem path: if it is a **file**, load it as YAML; if it is a **directory**, load `<dir>/pack.yaml` only (no `*.yaml` glob). This is the **override mechanism**: to use a custom pack with the same logical name as a built-in, use `--pack ./path/to/pack.yaml` or `--pack ./path/to/pack-dir/` (directory must contain `pack.yaml`).
2. **Built-in** — If `reference` matches a built-in pack name, load the embedded pack. **Built-in wins over local name**: a pack in the config directory with the same name as a built-in is *not* used when resolving by name.
3. **Local pack directory** — If `reference` is a pack name (valid per §3), look in the **config pack directory** for `{name}.yaml` or `{name}/pack.yaml`. If found, load from file (with containment check).
4. **Registry / BYOS** — (Existing or future) If `reference` is a registry reference (e.g. `name@version`) or BYOS URI, resolve accordingly. **This ADR does not change registry/BYOS behaviour** — it only inserts the local step before NotFound.
5. **NotFound** — Otherwise return the **existing** NotFound error (suggestions optional/future; do not introduce a new error contract).

**Override rule (document explicitly):** Names are not overridable by placing a pack in the local directory with the same name. To override a built-in, use a path: `--pack ./my-eu-ai-act-baseline/pack.yaml`.

### 2. Config directory (canonical + fallback)

Use a **single canonical convention** with OS-specific fallbacks:

| Platform | Canonical | Fallback |
|----------|-----------|----------|
| Unix-like (Linux/macOS) | `$XDG_CONFIG_HOME/assay/packs` | If `XDG_CONFIG_HOME` unset or empty: `~/.config/assay/packs` (XDG-compatible convention) |
| Windows | Roaming app data | `%APPDATA%\assay\packs`; if unset, use FOLDERID_RoamingAppData equivalent so resolution does not fail |

No new crate is required; use existing environment/directory logic in the repo where present (e.g. for config or cache). The pack directory is **not** created automatically by the loader; missing directory is treated as "no local packs" (no error). **The loader MUST NOT write to disk** (read-only resolution; security posture).

### 3. Security guardrails

- **Reference sanitization** — When resolving from the local pack directory, `reference` MUST be validated using the **existing pack-name validator** used by the pack schema (`is_valid_pack_name` in assay-evidence; pack name grammar is defined in [SPEC-Pack-Engine-v1, Pack Schema](./SPEC-Pack-Engine-v1.md#pack-schema) (Pack Definition: pack name grammar) and enforced in pack YAML validation). Do not define a new or stricter grammar in this ADR; cite the existing validator to avoid drift. Reject invalid names before any filesystem lookup. *(Non-normative example: `eu-ai-act-baseline`, `soc2-baseline` are valid; `../evil`, `Pack.Name` are invalid.)*
- **Path containment** — Build candidate path, then **check existence**; only then **canonicalize** and enforce that the resolved file path is **under** the config pack directory (no symlink escape, no `..`). If the canonical path is outside the pack directory, reject. **Canonicalization failures** (e.g. non-existent path, permission error, Windows oddities) MUST result in a safe error: either the existing NotFound or an explicit InvalidPackPath/InvalidRef; choose one and document it in the SPEC. Containment is enforced only after existence check.
- **No recursion** — Only one level: `packs/<name>.yaml` or `packs/<name>/pack.yaml`. No scanning of subdirectories beyond `packs/<name>/`.

### 4. Loader test matrix (mechanically testable)

The following cases MUST be covered by unit tests in the pack loader so that resolution behaviour is regression-safe:

| Case | Input | Expected |
|------|--------|----------|
| Path wins (file) | `--pack ./path/to/pack.yaml` (file exists) | Load from file |
| Path wins (dir) | `--pack ./path/to/dir/` (dir exists, contains `pack.yaml`) | Load from `dir/pack.yaml` |
| Built-in resolves | `--pack eu-ai-act-baseline` | Load built-in |
| Local resolves | `--pack my-pack` and `{config_dir}/packs/my-pack.yaml` exists | Load from local file |
| Not found | `--pack nonexistent` (no file, no built-in, no local) | Existing NotFound error (suggestions optional/future) |
| Built-in wins over local | `--pack eu-ai-act-baseline` and `{config_dir}/packs/eu-ai-act-baseline.yaml` exists | Load built-in (not local) |
| Invalid name rejected before FS | `--pack ../evil` or other invalid name | Error (InvalidPackName or NotFound); **no filesystem probing** for local dir |
| Symlink escape blocked | `{config_dir}/packs/foo.yaml` is symlink to `/tmp/foo.yaml` (outside config dir) | Reject (containment check fails) |

### 5. Source of truth

The **normative pack resolution order** and the **config directory convention** are specified in [SPEC-Pack-Engine-v1](./SPEC-Pack-Engine-v1.md). The concept doc [pack-registry.md](../concepts/pack-registry.md) may summarise resolution for users but MUST point to the SPEC as the single source of truth. No duplicate normative resolution order in a second doc.

## Consequences

- Users can install packs (e.g. `soc2-baseline`) by copying into `~/.config/assay/packs/` (or Windows equivalent) and run `assay evidence lint --pack soc2-baseline` without embedding in the binary.
- Built-in packs cannot be overridden by name; override requires explicit path (clear security and UX contract).
- Implementation is limited to loader + config dir resolution + tests + SPEC/concept doc updates; no pack schema or evidence contract changes.
- PR slicing: one PR for loader + tests + docs (no new packs); SOC2 pack content and optional built-in wiring can follow in separate PRs.

## References

- [ADR-016: Pack Taxonomy](./ADR-016-Pack-Taxonomy.md)
- [SPEC-Pack-Engine-v1](./SPEC-Pack-Engine-v1.md) — resolution order (to be updated)
- [Pack registry (concepts)](../concepts/pack-registry.md) — user-facing summary, links to SPEC
- [XDG Base Directory Specification](https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html)
