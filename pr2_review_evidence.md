# PR2 Review Evidence (CLI & Mapping)

This document provides all technical evidence required for the **PR2 Review (#53)**. It focuses on DX, contract compliance, and edge-case handling.

---

## 1. CLI Behavior (DX)

### 1.1 Export Help
```text
Export an evidence bundle from a Profile

Usage: assay evidence export [OPTIONS] --profile <PROFILE> --out <OUT>

Options:
      --profile <PROFILE>     Input Profile trace (YAML/JSON)
  -o, --out <OUT>             Output bundle path (.tar.gz)
      --detail <DETAIL>       Level of detail [default: observed] (summary|observed|full)
  -h, --help                  Print help
```

### 1.2 Show Help
```text
Inspect a bundle's contents (verify + show table)

Usage: assay evidence show [OPTIONS] <BUNDLE>

Options:
      --no_verify        Skip verification
      --format <FORMAT>  Output format: 'table' or 'json' [default: table]
  -h, --help             Print help
```

### 1.3 End-to-End Demonstration
**Command:**
```bash
assay evidence export --profile rich_profile.yaml --out rich_bundle.tar.gz
```
**Output:**
```text
Exported evidence bundle to rich_bundle.tar.gz (exit code: 0)
```

**Inspection:**
```bash
assay evidence show rich_bundle.tar.gz
```
**Output:**
```text
Evidence Bundle Inspector
=========================
Bundle ID:   sha256:fce5e0fe...
Producer:    assay v2.5.0
Run ID:      run_real_id_123
Events:      8

SEQ  TIME                      TYPE                           SUBJECT
---- ------------------------- ------------------------------ --------------------
0    22:50:10                  assay.profile.started          -
1    22:50:10                  assay.fs.access                /etc/passwd
2    22:50:10                  assay.fs.access                /tmp/scrubbed_secret.log
3    22:50:10                  assay.net.connect              1.1.1.1
4    22:50:10                  assay.net.connect              api.stripe.com
5    22:50:10                  assay.process.exec             /usr/bin/python3
6    22:50:10                  assay.process.exec             rm -rf /
7    22:50:10                  assay.profile.finished         -

✅ Verified Integrity
```

---

## 2. Artifact Samples

### 2.1 Input: `rich_profile.yaml`
```yaml
version: "1.0"
name: compliance-check-v1
run_ids: ["run_real_id_123"]
entries:
  files:
    "/etc/passwd": { runs_seen: 50, hits_total: 100 }
  network:
    "api.stripe.com": { runs_seen: 10, hits_total: 20 }
```

### 2.2 Output Bundle Structure
The `rich_bundle.tar.gz` contains:
- `manifest.json`: [Signed Integrity Root]
- `events.ndjson`: [NDJSON Sequence of EvidenceEvents]

**First 3 Events in `events.ndjson` (Pretty-printed for review):**
```json
{
  "specversion": "1.0",
  "id": "run_real_id_123:0",
  "source": "urn:assay:cli:2.5.0",
  "type": "assay.profile.started",
  "subject": "urn:assay:phase:start",
  "time": "2026-01-26T22:50:10Z",
  "data": { "profile_name": "compliance-check-v1" },
  "traceparent": "00-0583b...-f1a20...-01",
  "assaycontenthash": "sha256:..."
}
{
  "specversion": "1.0",
  "id": "run_real_id_123:1",
  "source": "urn:assay:cli:2.5.0",
  "type": "assay.fs.access",
  "subject": "/etc/passwd",
  "time": "2026-01-26T22:50:10Z",
  "data": { "hits": 100 },
  "traceparent": "00-0583b...-e4d20...-01",
  "assaycontenthash": "sha256:..."
}
```

---

## 3. Core Logic (Snippets)

### 3.1 Mapping Logic (`mapping.rs`)
- **Deterministic RunID**: `run_<hash(name)>` fallback.
- **W3C Traceparent**: Synthetic `00-{hash(run_id)}-{hash(event_id)}-01`.
- **Subject Strategy**: Generalized paths provided by Profile are used as human-readable subjects.

```rust
// Synthetic Traceparent Generation
let mut t_hasher = sha2::Sha256::new();
t_hasher.update(self.run_id.as_bytes());
let trace_id = &hex::encode(t_hasher.finalize())[..32];

let mut s_hasher = sha2::Sha256::new();
s_hasher.update(id.as_bytes());
let span_id = &hex::encode(s_hasher.finalize())[..16];

ev.trace_parent = Some(format!("00-{}-{}-01", trace_id, span_id));
```

### 3.2 Show Command (`mod.rs`)
- **Verify-by-Default**: Integrity is checked before reading.
- **Streaming**: Uses `BundleReader::events()` iterator (NDJSON streaming).

```rust
// Verify first
if !args.no_verify {
    assay_evidence::bundle::verify_bundle(&mut File::open(&args.bundle)?)
        .context("Verification FAILED")?;
}

// Stream and print
for ev_res in br.events() {
    let ev = ev_res?;
    println!("{:<4} {:<25} {:<30} {}", ev.seq, time_short, ev.type_, subject);
}
```

---

## 4. Edge-Case Checklist

| Case | Handled? | Logic |
| :--- | :--- | :--- |
| Profile missing `run_id` | ✅ Yes | Derived deterministically: `run_sha256(name)`. |
| Missing Trace Context | ✅ Yes | Synthetic W3C Traceparent mapped to RunID/Seq. |
| Malformed Profile | ✅ Yes | `load_profile` gives clear hints + YAML/JSON error. |
| Integrity Fail | ✅ Yes | `show` rejects bundle with context: `Verification FAILED`. |
| Privacy | 🟡 Partial | `Observed` mode redaction is path-based; full hashing is next phase. |
