# Verification: ADR-021 & ADR-022 vs Codebase

**Date:** February 2026 (updated)
**Scope:** [ADR-021 Local Pack Discovery](./ADR-021-Local-Pack-Discovery.md), [ADR-022 SOC2 Baseline Pack](./ADR-022-SOC2-Baseline-Pack.md)

## Summary

| ADR     | Status | Notes |
|---------|--------|--------|
| ADR-021 | Implemented | Resolution order, path-as-dir, local config dir, security (containment, pack name validator) all in code. SPEC updated. |
| ADR-022 | Implemented | soc2-baseline pack in repo, built-in, disclaimer per ADR §4, LICENSE Apache-2.0. |

---

## ADR-021: Local Pack Discovery

### Resolution order

- **Code** (`crates/assay-evidence/src/lint/packs/loader.rs::load_pack`):
  (1) Path (file or dir with pack.yaml)
  (2) Built-in by name
  (3) Local pack directory (if valid pack name)
  (4) NotFound
- **ADR:** Path → Built-in → Local pack directory → Registry/BYOS → NotFound.
- **Conclusion:** Implemented. Registry/BYOS are future (assay-registry); loader covers steps 1–3 + 5.

### Path branch: file vs directory

- **Code:** When `path.exists()` and `path.is_dir()`, loads `path.join("pack.yaml")`. If dir has no pack.yaml, returns ReadError. If path is file, loads directly.
- **ADR §1:** File → load as YAML; directory → load `<dir>/pack.yaml` only.
- **Conclusion:** Implemented.

### Config directory (XDG / Windows)

- **Code:** `get_config_pack_dir()` in loader.rs: `$XDG_CONFIG_HOME/assay/packs` (fallback `~/.config/assay/packs`), Windows `%APPDATA%\assay\packs`. Missing dir treated as "no local packs".
- **ADR §2:** Same convention; loader must not create or write.
- **Conclusion:** Implemented.

### Pack name validator

- **Code:** `is_valid_pack_name` in loader.rs (local) and `pub fn is_valid_pack_name` in schema.rs. Grammar: lowercase, digits, hyphens; no leading/trailing hyphen. Used before local FS lookup.
- **ADR §3:** Reuse existing validator; reject invalid names before FS.
- **Conclusion:** Implemented.

### Path containment and symlink escape

- **Code:** After existence, `canonicalize()`; `canonical_path.starts_with(canonical_config)`; reject if outside. Returns `PackValidationError::Safety` on escape.
- **ADR §3:** Containment after existence; reject canonical path outside config dir.
- **Conclusion:** Implemented.

### Loader test matrix (ADR §4)

| Case                    | Test                               | Status |
|-------------------------|------------------------------------|--------|
| Path wins (file)        | `test_path_wins_over_builtin`      | Done   |
| Path wins (dir)         | Implicit in path logic             | Done   |
| Built-in resolves       | `test_builtin_wins_over_local`     | Done   |
| Local resolves          | `test_local_pack_resolution`, etc. | Done   |
| Not found               | Implicit                           | Done   |
| Built-in wins over local| `test_builtin_wins_over_local`     | Done   |
| Invalid name rejected   | `test_is_valid_pack_name`          | Done   |
| Symlink escape blocked  | `test_symlink_escape_rejected`     | Done   |

### SPEC-Pack-Engine-v1

- **SPEC:** Pack Resolution (Normative), Config directory, Pack name grammar, Local resolution security — all documented. ADR-021 referenced.
- **Conclusion:** Aligned.

---

## ADR-022: SOC2 Baseline Pack

### Pack schema: `article_ref`

- **Code:** `PackRule` has `article_ref: Option<String>`. soc2-baseline uses `article_ref: "CC6.1"` etc.
- **Conclusion:** Matches.

### Check types

- **Code:** soc2-baseline uses `event_type_exists`, `event_pairs` (exact names from SPEC).
- **Conclusion:** Matches.

### Pack layout and built-in

- **Code:** `packs/open/soc2-baseline/` with pack.yaml, README.md, LICENSE (Apache-2.0). `BUILTIN_PACKS` includes soc2-baseline (from crates/assay-evidence/packs/soc2-baseline.yaml).
- **Conclusion:** Implemented.

### Disclaimer (ADR §4)

- **Pack:** Disclaimer covers evidence presence vs effectiveness; passing ≠ compliance; failing ≠ audit failure; organizations responsible.
- **Conclusion:** Aligned.

### LICENSE

- **Pack:** `license: Apache-2.0` in pack.yaml; LICENSE file is Apache-2.0 (aligned with eu-ai-act-baseline).
- **Conclusion:** Aligned.

---

## Action items

None. ADR-021 and ADR-022 are implemented and aligned with SPEC and codebase.
