# assay generate

Generate policy scaffolding from trace or profile inputs.

---

## Synopsis

```bash
assay generate [OPTIONS]
```

---

## Description

`assay generate` supports two modes:

- single-run mode from `--input` trace events
- profile mode from `--profile` stability data

For parity hardening, the key reviewer-facing surface is `--diff`: it previews how generated policy output differs from an existing output file.

---

## Options

| Option | Description |
|--------|-------------|
| `--input`, `-i <FILE>` | Input trace file (single-run mode). |
| `--profile <FILE>` | Profile file (multi-run mode). |
| `--output`, `-o <FILE>` | Output path. Default: `policy.yaml`. |
| `--name <NAME>` | Policy name metadata. |
| `--format <FMT>` | Output format (`yaml` default). |
| `--dry-run` | Do not write output file. |
| `--diff` | Show policy diff versus existing output file. |
| `--heuristics` | Enable heuristics in single-run generation. |
| `--entropy-threshold <N>` | Entropy threshold for heuristics. |
| `--min-stability <N>` | Minimum stability to auto-allow in profile mode. |
| `--review-threshold <N>` | Threshold below which entries can be marked risky. |
| `--new-is-risky` | Treat low-stability entries as risky instead of skipping. |
| `--alpha <N>` | Smoothing parameter for profile mode. |
| `--min-runs <N>` | Minimum runs before auto-allow. |
| `--wilson-z <N>` | Wilson lower-bound confidence parameter. |

---

## Examples

### Trace input

```bash
assay generate --input traces/session.jsonl --output policy.yaml
```

### Diff preview (no write)

```bash
assay generate --input traces/session.jsonl --output policy.yaml --diff --dry-run
```

### Profile input

```bash
assay generate --profile .assay/profile.json --min-stability 0.8 --new-is-risky
```

---

## Exit Behavior

- exits non-zero on invalid inputs/arguments
- with `--dry-run`, does not write files
