# SPEC-Pack-Engine-v1: Compliance Pack Engine Specification

## Status

Draft (January 2026)

## Overview

The Pack Engine enables external rule definitions ("packs") for evidence bundle linting. Packs are YAML files containing compliance, security, or quality checks that map to regulatory requirements or best practices.

**Design goals:**
- Extend `assay evidence lint` without modifying core rule registry
- Support pack composition (`--pack a,b`)
- Produce GitHub Code Scanning-compatible SARIF
- Enable baseline (OSS) and pro (Enterprise) pack split per [ADR-016](./ADR-016-Pack-Taxonomy.md)

## CLI Interface

### New Arguments

```bash
assay evidence lint bundle.tar.gz [OPTIONS]

--pack <PACK>     Comma-separated list of pack references
                  Built-in:  --pack eu-ai-act-baseline
                  File:      --pack ./custom-pack.yaml
                  Multiple:  --pack eu-ai-act-baseline,soc2-baseline
```

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success, no findings at/above threshold |
| 1 | Findings at/above threshold |
| 2 | Bundle verification failed |
| 3 | Pack loading/validation failed |

### Examples

```bash
# Baseline pack only
assay evidence lint bundle.tar.gz --pack eu-ai-act-baseline

# Composition (both packs run)
assay evidence lint bundle.tar.gz --pack eu-ai-act-baseline,soc2-baseline

# Custom pack from file
assay evidence lint bundle.tar.gz --pack ./my-org-pack.yaml

# Mixed: built-in + custom
assay evidence lint bundle.tar.gz --pack eu-ai-act-baseline,./exceptions.yaml

# With SARIF output
assay evidence lint bundle.tar.gz --pack eu-ai-act-baseline --format sarif
```

## Pack Schema

### Pack Definition (YAML)

```yaml
# Required fields
name: string          # Pack identifier, lowercase alphanumeric + hyphens
version: string       # Semver (e.g., "1.0.0")
kind: enum            # compliance | security | quality
description: string   # Human-readable description
author: string        # Pack author name/org
license: string       # SPDX identifier (e.g., "Apache-2.0")

# Optional fields
source_url: string    # Primary source URL (e.g., EUR-Lex for EU regulations)

# REQUIRED if kind == "compliance"
disclaimer: string    # Multi-line legal disclaimer

# Version constraints
requires:
  assay_min_version: string         # Semver constraint (e.g., ">=2.9.0")
  evidence_schema_version: string   # Optional schema version (e.g., "1.0")

# Rule definitions
rules: []             # Array of PackRule (see below)
```

### Rule Definition

```yaml
rules:
  - id: string              # Short rule ID (e.g., "EU12-001"), unique within pack
    severity: enum          # error | warning | info
    description: string     # One-line description
    article_ref: string     # Regulatory reference (optional, e.g., "12(1)")
    help_markdown: string   # Multi-line help text with markdown
    check: CheckDefinition  # Check to perform (see below)
```

### Check Types

#### `event_count`

Verify bundle contains minimum number of events.

```yaml
check:
  type: event_count
  min: 1                    # Minimum event count required
```

#### `event_pairs`

Verify matching start/finish event pairs exist.

```yaml
check:
  type: event_pairs
  start_pattern: string     # Glob pattern for start events (e.g., "*.started")
  finish_pattern: string    # Glob pattern for finish events (e.g., "*.finished")
```

#### `event_field_present`

Verify at least one event contains one of the specified fields.

```yaml
check:
  type: event_field_present
  any_of: [string]          # Field names to check (e.g., ["run_id", "traceparent"])
  in_data: bool             # If true, check in `data.*` (default: false, check top-level)
```

#### `event_type_exists`

Verify at least one event of specified type exists.

```yaml
check:
  type: event_type_exists
  pattern: string           # Glob pattern for event type (e.g., "assay.policy.*")
```

#### `manifest_field`

Verify manifest contains specified field.

```yaml
check:
  type: manifest_field
  field: string             # Field path (e.g., "x-assay-retention")
  required: bool            # If true, missing = error; if false, missing = warning
```

### Example Pack

```yaml
name: eu-ai-act-baseline
version: "1.0.0"
kind: compliance
description: EU AI Act Article 12 record-keeping baseline for high-risk AI systems
author: Assay Team
license: Apache-2.0
source_url: https://eur-lex.europa.eu/eli/reg/2024/1689/oj

disclaimer: |
  This pack provides technical checks that map to EU AI Act Article 12 requirements.
  Passing these checks does NOT constitute legal compliance. Organizations remain
  responsible for meeting all applicable legal requirements. Consult qualified
  legal counsel for compliance determination.

requires:
  assay_min_version: ">=2.9.0"
  evidence_schema_version: "1.0"

rules:
  - id: EU12-001
    severity: error
    description: Evidence bundle contains automatically recorded operational events
    article_ref: "12(1)"
    help_markdown: |
      ## EU AI Act Article 12(1) - Automatic Event Recording

      High-risk AI systems must technically allow for automatic recording of events.
      This check verifies that the evidence bundle contains at least one operational event.

      **Reference**: [Article 12(1)](https://eur-lex.europa.eu/eli/reg/2024/1689/oj#d1e3029-1-1)
    check:
      type: event_count
      min: 1

  - id: EU12-002
    severity: error
    description: Events include run lifecycle fields for operation monitoring
    article_ref: "12(2)(c)"
    help_markdown: |
      ## EU AI Act Article 12(2)(c) - Operation Monitoring

      Logs must enable monitoring of AI system operation. This check verifies
      events contain lifecycle fields (started/finished events).
    check:
      type: event_pairs
      start_pattern: "*.started"
      finish_pattern: "*.finished"

  - id: EU12-003
    severity: warning
    description: Events include correlation IDs for post-market monitoring
    article_ref: "12(2)(b)"
    help_markdown: |
      ## EU AI Act Article 12(2)(b) - Post-Market Monitoring

      Logs must facilitate post-market monitoring. This check verifies events
      contain correlation identifiers.
    check:
      type: event_field_present
      any_of: ["run_id", "traceparent", "build_id", "version"]

  - id: EU12-004
    severity: warning
    description: Events include fields enabling risk situation identification
    article_ref: "12(2)(a)"
    help_markdown: |
      ## EU AI Act Article 12(2)(a) - Risk Identification

      Logs must enable identification of risk situations or substantial modifications.
    check:
      type: event_field_present
      any_of: ["policy_decision", "denied", "policy_hash", "config_hash", "violation"]
      in_data: true
```

## Pack Digest

### Algorithm (Normative)

```
pack_digest = sha256( JCS( JSON( parse_yaml(pack_file) ) ) )
```

**Steps:**
1. Parse YAML file into native data structure
2. Validate against pack schema (unknown fields MUST cause error)
3. Serialize to JSON (only known schema fields)
4. Apply JCS canonicalization ([RFC 8785](https://datatracker.ietf.org/doc/html/rfc8785))
5. Compute SHA-256 hash
6. Format: `sha256:{hex_digest}`

### Unknown Fields Policy

YAML files with fields not defined in the pack schema MUST fail validation with error:

```
Error: Pack 'my-pack' contains unknown field 'x-custom' at root level.
Unknown fields are not allowed (prevents digest bypass attacks).
```

## Rule ID Namespacing

### Canonical Format

```
{pack_name}@{pack_version}:{rule_id}
```

**Examples:**
- `eu-ai-act-baseline@1.0.0:EU12-001`
- `soc2-baseline@1.0.0:SOC2-CC6.1`
- `my-org-pack@2.1.0:CUSTOM-001`

### Collision Policy

| Scenario | `kind: compliance` | `kind: security/quality` |
|----------|-------------------|--------------------------|
| Same canonical ID from same pack | Dedupe (run once) | Dedupe (run once) |
| Same short_id from different packs | Both run | Both run |
| Same canonical ID from different packs | **Hard fail** | Last wins + warning |

**Rationale**: Compliance tooling must not silently change behavior based on pack order.

### Hard Fail Example

```bash
$ assay evidence lint bundle.tar.gz --pack pack-a,pack-b
Error: Rule collision detected (compliance packs):
  - pack-a@1.0.0:RULE-001
  - pack-b@1.0.0:RULE-001

Compliance packs cannot have overlapping canonical rule IDs.
Use explicit 'overrides:' (future) or rename rules.
```

## Version Compatibility

### `assay_min_version` Check

On pack load, verify current Assay version satisfies constraint:

```rust
if !semver_satisfies(current_version, pack.requires.assay_min_version) {
    return Err(PackError::IncompatibleVersion {
        pack: pack.name,
        required: pack.requires.assay_min_version,
        current: current_version,
    });
}
```

**Error message:**
```
Error: Pack 'eu-ai-act-baseline@1.0.0' requires Assay >=2.9.0, but current version is 2.8.0.
Please upgrade Assay: cargo install assay-cli
```

### `evidence_schema_version` Check

Optional field for future schema evolution. Currently informational.

## SARIF Output

### Pack Metadata Location

Pack metadata uses `properties` bags (SARIF-standard extensibility):

```json
{
  "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/...",
  "version": "2.1.0",
  "runs": [{
    "tool": {
      "driver": {
        "name": "assay-evidence-lint",
        "version": "2.9.0",
        "informationUri": "https://docs.assay.dev/lint",
        "properties": {
          "assayPacks": [
            {
              "name": "eu-ai-act-baseline",
              "version": "1.0.0",
              "digest": "sha256:abc123...",
              "source_url": "https://eur-lex.europa.eu/eli/reg/2024/1689/oj"
            }
          ]
        },
        "rules": [
          {
            "id": "eu-ai-act-baseline@1.0.0:EU12-001",
            "shortDescription": {
              "text": "Evidence bundle contains automatically recorded events"
            },
            "help": {
              "markdown": "## EU AI Act Article 12(1)\\n\\n..."
            },
            "defaultConfiguration": {
              "level": "error"
            },
            "properties": {
              "pack": "eu-ai-act-baseline",
              "pack_version": "1.0.0",
              "short_id": "EU12-001",
              "article_ref": "12(1)"
            }
          }
        ]
      }
    },
    "automationDetails": {
      "id": "assay-evidence/lint/{run_id}/{version}"
    },
    "properties": {
      "disclaimer": "This pack provides technical checks..."
    },
    "results": [
      {
        "ruleId": "eu-ai-act-baseline@1.0.0:EU12-001",
        "level": "error",
        "message": {
          "text": "Bundle contains 0 events (minimum: 1)"
        },
        "partialFingerprints": {
          "assayLintFingerprint/v1": "sha256:..."
        },
        "properties": {
          "article_ref": "12(1)"
        }
      }
    ]
  }]
}
```

### Fingerprint Computation

For pack rules, fingerprint includes canonical rule ID:

```rust
let fingerprint = sha256(format!(
    "{}:{}:{}",
    canonical_rule_id,      // eu-ai-act-baseline@1.0.0:EU12-001
    location_key,           // "global" or "seq:line"
    pack_digest             // sha256:abc123...
));
```

## Disclaimer Output

For `kind: compliance` packs, disclaimer appears in:

| Output Format | Location |
|---------------|----------|
| `--format text` | Header before findings |
| `--format json` | Top-level `disclaimer` field |
| `--format sarif` | `run.properties.disclaimer` |

### Console Output Example

```
Assay Evidence Lint
===================
Bundle: sha256:abc... (events: 42, verified: true)

⚠️  COMPLIANCE DISCLAIMER (eu-ai-act-baseline@1.0.0)
This pack provides technical checks that map to EU AI Act Article 12 requirements.
Passing these checks does NOT constitute legal compliance. Organizations remain
responsible for meeting all applicable legal requirements.

[error] eu-ai-act-baseline@1.0.0:EU12-002 (global) Missing lifecycle events
        Article 12(2)(c) requires operation monitoring via start/finish events.

Summary: 1 total (1 errors, 0 warnings, 0 info)
```

## Implementation

### Module Structure

```
crates/assay-evidence/src/lint/
├── mod.rs              # Existing: LintFinding, LintReport, Severity
├── engine.rs           # Existing: lint_bundle() - extend to accept packs
├── rules.rs            # Existing: built-in rules (unchanged)
├── sarif.rs            # Existing: to_sarif() - extend for pack metadata
└── packs/
    ├── mod.rs          # Pack module exports
    ├── schema.rs       # PackDefinition, PackKind, PackRule, CheckDefinition
    ├── loader.rs       # YAML loader, validator, digest computation
    ├── executor.rs     # Run pack checks, collision handling
    └── checks.rs       # Check implementations (event_count, event_pairs, etc.)
```

### Key Types

```rust
// schema.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackKind {
    Compliance,
    Security,
    Quality,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackDefinition {
    pub name: String,
    pub version: String,
    pub kind: PackKind,
    pub description: String,
    pub author: String,
    pub license: String,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub disclaimer: Option<String>,
    pub requires: PackRequirements,
    pub rules: Vec<PackRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackRequirements {
    pub assay_min_version: String,
    #[serde(default)]
    pub evidence_schema_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackRule {
    pub id: String,
    pub severity: PackSeverity,
    pub description: String,
    #[serde(default)]
    pub article_ref: Option<String>,
    #[serde(default)]
    pub help_markdown: Option<String>,
    pub check: CheckDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CheckDefinition {
    EventCount { min: usize },
    EventPairs { start_pattern: String, finish_pattern: String },
    EventFieldPresent { any_of: Vec<String>, #[serde(default)] in_data: bool },
    EventTypeExists { pattern: String },
    ManifestField { field: String, #[serde(default)] required: bool },
}

// loader.rs
pub struct LoadedPack {
    pub definition: PackDefinition,
    pub digest: String,           // sha256:...
    pub source: PackSource,       // BuiltIn | File(PathBuf)
}

pub enum PackSource {
    BuiltIn(&'static str),        // Pack name for built-in packs
    File(PathBuf),
}

pub fn load_pack(reference: &str) -> Result<LoadedPack, PackError>;
pub fn load_packs(references: &[String]) -> Result<Vec<LoadedPack>, PackError>;

// executor.rs
pub struct PackExecutor {
    packs: Vec<LoadedPack>,
}

impl PackExecutor {
    pub fn new(packs: Vec<LoadedPack>) -> Result<Self, PackError>;
    pub fn execute(&self, bundle: &VerifiedBundle) -> Vec<LintFinding>;
}
```

### Engine Integration

```rust
// engine.rs - updated signature
pub fn lint_bundle<R: Read>(
    reader: R,
    limits: VerifyLimits,
    packs: Option<&[LoadedPack]>,  // NEW: optional pack rules
) -> Result<LintReport>;
```

### CLI Integration

```rust
// lint.rs - updated
#[derive(Debug, Args, Clone)]
pub struct LintArgs {
    #[arg(value_name = "BUNDLE")]
    pub bundle: std::path::PathBuf,

    #[arg(long, default_value = "text")]
    pub format: String,

    #[arg(long, default_value = "error")]
    pub fail_on: String,

    /// Comma-separated pack references (built-in name or file path)
    #[arg(long, value_delimiter = ',')]
    pub pack: Option<Vec<String>>,
}
```

## Built-in Packs

### Registration

Built-in packs are embedded at compile time:

```rust
// packs/mod.rs
pub static BUILTIN_PACKS: &[(&str, &str)] = &[
    ("eu-ai-act-baseline", include_str!("../../../../packs/eu-ai-act-baseline.yaml")),
    // Future: ("soc2-baseline", include_str!("../../../../packs/soc2-baseline.yaml")),
];

pub fn get_builtin_pack(name: &str) -> Option<&'static str> {
    BUILTIN_PACKS.iter()
        .find(|(n, _)| *n == name)
        .map(|(_, content)| *content)
}
```

### Pack Resolution

```rust
pub fn resolve_pack_reference(reference: &str) -> Result<LoadedPack, PackError> {
    // 1. Check if it's a file path
    if reference.starts_with("./") || reference.starts_with("/") || reference.ends_with(".yaml") {
        return load_pack_from_file(Path::new(reference));
    }

    // 2. Check built-in packs
    if let Some(content) = get_builtin_pack(reference) {
        return load_pack_from_string(content, PackSource::BuiltIn(reference));
    }

    // 3. Not found
    Err(PackError::NotFound {
        reference: reference.to_string(),
        suggestion: suggest_similar_pack(reference),
    })
}
```

## Error Messages

### Pack Not Found

```
Error: Pack 'eu-ai-act' not found.

Did you mean 'eu-ai-act-baseline'?

Available built-in packs:
  - eu-ai-act-baseline (EU AI Act Article 12 baseline)

Or specify a file path: --pack ./my-pack.yaml
```

### Validation Failed

```
Error: Pack './my-pack.yaml' validation failed:

  - Line 5: 'kind' must be one of: compliance, security, quality
  - Line 12: Rule 'MY-001' missing required field 'check'
  - Line 18: Unknown check type 'custom_check'

See: https://docs.assay.dev/packs/schema
```

### Disclaimer Missing

```
Error: Pack 'my-compliance-pack' is kind 'compliance' but missing 'disclaimer'.

Compliance packs MUST include a disclaimer explaining that passing checks
does not constitute legal compliance. Add a 'disclaimer' field to your pack.

Example:
  disclaimer: |
    This pack provides technical checks only. Passing these checks
    does NOT constitute legal compliance. Consult legal counsel.
```

## Testing

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    // Schema validation
    #[test]
    fn test_valid_pack_parses() { ... }

    #[test]
    fn test_compliance_pack_requires_disclaimer() { ... }

    #[test]
    fn test_unknown_fields_rejected() { ... }

    // Digest computation
    #[test]
    fn test_digest_deterministic() { ... }

    #[test]
    fn test_digest_changes_on_content_change() { ... }

    // Collision handling
    #[test]
    fn test_compliance_collision_hard_fail() { ... }

    #[test]
    fn test_security_collision_last_wins() { ... }

    // Check execution
    #[test]
    fn test_event_count_check() { ... }

    #[test]
    fn test_event_pairs_check() { ... }
}
```

### Integration Tests

```rust
#[test]
fn test_lint_with_baseline_pack() {
    let bundle = create_test_bundle_with_events(vec![
        event("assay.run.started"),
        event("assay.run.finished"),
    ]);

    let report = lint_bundle_with_pack(bundle, "eu-ai-act-baseline").unwrap();

    // EU12-001 should pass (has events)
    // EU12-002 should pass (has started/finished)
    assert!(!report.has_findings_at_or_above(&Severity::Error));
}

#[test]
fn test_lint_empty_bundle_fails_eu12_001() {
    let bundle = create_test_bundle_with_events(vec![]);
    let report = lint_bundle_with_pack(bundle, "eu-ai-act-baseline").unwrap();

    assert!(report.findings.iter().any(|f|
        f.rule_id.contains("EU12-001") && f.severity == Severity::Error
    ));
}
```

## Acceptance Criteria

### Pack Engine (Must Have)

- [ ] `--pack` CLI argument parses comma-separated references
- [ ] Built-in pack resolution (`eu-ai-act-baseline`)
- [ ] File pack loading (`./custom.yaml`)
- [ ] YAML schema validation with clear error messages
- [ ] Unknown fields rejected (security)
- [ ] `kind: compliance` requires disclaimer (hard fail)
- [ ] `assay_min_version` checked on load
- [ ] Pack digest computed (sha256 of JCS-canonical JSON)
- [ ] Collision detection with hard-fail for compliance packs
- [ ] Rule ID namespacing (`{pack}@{version}:{rule_id}`)

### Check Types (Must Have)

- [ ] `event_count` - minimum event count
- [ ] `event_pairs` - start/finish matching
- [ ] `event_field_present` - field existence check

### SARIF Output (Must Have)

- [ ] `tool.driver.properties.assayPacks[]` with name, version, digest
- [ ] `rules[].id` uses canonical format
- [ ] `rules[].properties` includes pack, pack_version, short_id, article_ref
- [ ] `results[].properties` includes article_ref
- [ ] `run.properties.disclaimer` for compliance packs

### Console Output (Must Have)

- [ ] Disclaimer header for compliance packs
- [ ] Rule ID shows canonical format
- [ ] Article reference in finding output

### EU AI Act Baseline Pack (Must Have)

- [ ] EU12-001: Event count check (Article 12(1))
- [ ] EU12-002: Lifecycle events check (Article 12(2)(c))
- [ ] EU12-003: Correlation ID check (Article 12(2)(b))
- [ ] EU12-004: Risk fields check (Article 12(2)(a))

## References

- [ADR-013: EU AI Act Compliance Pack](./ADR-013-EU-AI-Act-Pack.md)
- [ADR-016: Pack Taxonomy](./ADR-016-Pack-Taxonomy.md)
- [RFC 8785: JSON Canonicalization Scheme](https://datatracker.ietf.org/doc/html/rfc8785)
- [SARIF 2.1.0 Specification](https://docs.oasis-open.org/sarif/sarif/v2.1.0/sarif-v2.1.0.html)
- [EU AI Act Article 12](https://eur-lex.europa.eu/eli/reg/2024/1689/oj#d1e3029-1-1)
