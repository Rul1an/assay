# Verification: ADR-021 & ADR-022 vs Codebase

**Date:** February 2026
**Scope:** [ADR-021 Local Pack Discovery](./ADR-021-Local-Pack-Discovery.md), [ADR-022 SOC2 Baseline Pack](./ADR-022-SOC2-Baseline-Pack.md)

## Summary

| ADR   | Status | Notes |
|-------|--------|--------|
| ADR-021 | In line | Resolution order in code is path → built-in → NotFound; local dir and path-as-dir not yet implemented (per ADR design). One implementation note: pack-name validator is private. |
| ADR-022 | In line | Schema and check types match ADR; soc2-baseline pack not in repo (implementation pending). |

---

## ADR-021: Local Pack Discovery

### Resolution order

- **Code** (`crates/assay-evidence/src/lint/packs/loader.rs::load_pack`):
  (1) `path.exists()` → `load_pack_from_file(path)`
  (2) built-in by name
  (3) `Err(PackError::NotFound { reference, suggestion })`
- **ADR:** Path → Built-in → **Local pack directory** → Registry/BYOS → NotFound.
- **Conclusion:** Code implements path and built-in only. Local pack directory and registry steps are **not yet implemented**; ADR is the design for adding them. No conflict.

### Path branch: file vs directory

- **Code:** `load_pack_from_file(path)` uses `std::fs::read_to_string(path)`. If `reference` is a directory, this fails with an I/O error (reading a dir as file).
- **ADR §1:** If path is a **file**, load as YAML; if path is a **directory**, load `<dir>/pack.yaml` only.
- **Conclusion:** Path-as-file is supported. **Path-as-directory is not implemented**; implementation must add: when `path.exists() && path.is_dir()`, load `path.join("pack.yaml")` (and still enforce read-only, no new contract).

### Config directory (XDG / Windows)

- **Code:** No use of `XDG_CONFIG_HOME/assay/packs` or `%APPDATA%\assay\packs` in `assay-evidence`. Other crates use `dirs::config_dir()`, `XDG_DATA_HOME`, `APPDATA` for config/cache elsewhere.
- **ADR §2:** Defines canonical config dir and fallbacks; loader must not create it and must not write.
- **Conclusion:** Config directory for packs is **not in codebase yet**; ADR defines it for implementation. No conflict.

### Pack name validator

- **Code:** `is_valid_pack_name` in `crates/assay-evidence/src/lint/packs/schema.rs` (lines 431–437): lowercase ASCII, digits, hyphens; no leading/trailing hyphen. Used in `PackDefinition::validate()`. Function is **private** (`fn`, not `pub`).
- **ADR §3:** “Validate using the **existing pack-name validator** … Do not define a new or stricter grammar … cite the existing validator.”
- **Conclusion:** Grammar and usage match. For **local resolution**, the loader must validate the reference before any FS lookup. **Implementation note:** Expose the validator (e.g. `pub(crate) fn is_valid_pack_name` in `schema.rs`) so the loader can reuse it without duplicating rules.

### NotFound and suggestions

- **Code:** `PackError::NotFound { reference, suggestion }` with `suggest_similar_pack(reference)` (built-in names + Levenshtein).
- **ADR §1 step 5:** “Return the **existing** NotFound error (suggestions optional/future).”
- **Conclusion:** Behaviour and contract align; no change required.

### SPEC-Pack-Engine-v1

- **SPEC** (around line 716): “Pack Resolution (Normative)” documents current 3-step order (path exists → file; built-in; NotFound). No config directory, no path-as-dir.
- **ADR §5:** Normative resolution order and config directory convention live in SPEC; this ADR records the decision; SPEC “to be updated.”
- **Conclusion:** SPEC will need to be updated when implementing ADR-021 (resolution order + config dir). No conflict.

### Built-in packs

- **Code:** `BUILTIN_PACKS` in `crates/assay-evidence/src/lint/packs/mod.rs`: `eu-ai-act-baseline`, `mandate-baseline` (from `crates/assay-evidence/packs/*.yaml`).
- **ADR:** Built-in wins over local by name; override only via path. No new built-ins required by ADR-021.
- **Conclusion:** Consistent.

---

## ADR-022: SOC2 Baseline Pack

### Pack schema: `article_ref`

- **Code:** `PackRule` in `crates/assay-evidence/src/lint/packs/schema.rs` has `article_ref: Option<String>` (line 159). SPEC Rule Definition: `article_ref: string` (optional).
- **ADR §2:** Use **existing** field `article_ref` for TSC identifiers (e.g. `CC6.1`); no new schema fields.
- **Conclusion:** Matches; EU AI Act pack already uses `article_ref` (e.g. `"12(1)"`); same field for SOC2 is correct.

### Check types

- **Code:** `CheckDefinition` in schema and `execute_check` in `checks.rs`: `EventCount`, `EventPairs`, `EventFieldPresent`, `EventTypeExists`, `ManifestField` (and `JsonPathExists`, `Conditional`). Serialized names: `event_count`, `event_pairs`, `event_field_present`, `event_type_exists`, `manifest_field` (schema around 360–366).
- **ADR §3:** Use only existing check types with **exact names** from SPEC: `event_count`, `event_pairs`, `event_field_present`, `event_type_exists`, `manifest_field`.
- **Conclusion:** Names and behaviour align; no code change needed for ADR-022.

### Pack layout and built-in

- **Code:** `packs/open/` contains `eu-ai-act-baseline/` (pack.yaml, README, LICENSE). No `packs/open/soc2-baseline/`. Built-ins are `eu-ai-act-baseline`, `mandate-baseline` (no `soc2-baseline`).
- **ADR §5:** Location `packs/open/soc2-baseline/` with pack.yaml, README, LICENSE; built-in optional (PR-C); “implementation pending” until pack content is merged.
- **Conclusion:** Repo state matches “implementation pending”; no conflict.

---

## Action items (for implementation)

1. **ADR-021**
   - Implement local pack directory step (config dir lookup, containment, no recursion).
   - Implement path-as-directory: when reference is an existing directory, load `path.join("pack.yaml")`.
   - Expose pack-name validator for loader (e.g. `pub(crate) is_valid_pack_name` in schema) and use it for local name validation before FS access.
   - Update SPEC-Pack-Engine-v1 with normative resolution order and config directory convention.
2. **ADR-022**
   - None for codebase alignment; pack content and optional built-in wiring follow in PR-B/PR-C.
