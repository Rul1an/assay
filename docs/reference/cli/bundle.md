# assay bundle

Create and verify replay bundles.

---

## Synopsis

```bash
assay bundle create [--from <PATH> | --run-id <ID>] [--output <PATH>] [--config <PATH>] [--trace-file <PATH>]
assay bundle verify --bundle <BUNDLE.tar.gz>
```

---

## Subcommands

### `assay bundle create`

Builds a replay bundle (`.tar.gz`) from run artifacts.

Default selection behavior:
- `--from`: explicit source directory/file
- `--run-id`: select run path under `.assay/`
- no selector: latest `run.json` by mtime

Default output path:
- `.assay/bundles/<run_id>.tar.gz`

Create behavior:
- writes canonical bundle layout (`manifest.json`, `files/`, `outputs/`, `cassettes/`)
- captures `source_run_path` + `selection_method` for audit
- computes file manifest and digests
- scrubs cassette content
- runs `bundle verify` before success

### `assay bundle verify`

Validates bundle integrity and safe-to-share checks.

Verification includes:
- a complete inventory: a bundle with members under `files/`, `outputs/` or `cassettes/` must list every member in `manifest.files`; a missing inventory or an unlisted member fails verification (a bundle with only `manifest.json` has nothing to check)
- manifest-vs-file hash/size checks
- a declared `trace_path` must be a canonical member listed in the inventory
- forbidden pattern scan
  - `files/` and `cassettes/`: hard fail
  - `outputs/`: warn only

---

## Options

### create

| Option | Description |
|--------|-------------|
| `--from <PATH>` | Source run directory or `run.json` path. |
| `--run-id <ID>` | Source run id (alternative to `--from`). |
| `--output <PATH>` | Output bundle path. |
| `--config <PATH>` | Optional config file to include. |
| `--trace-file <PATH>` | Optional trace file to include. |

### verify

| Option | Description |
|--------|-------------|
| `--bundle <PATH>` | Bundle archive path (`.tar.gz`). |

---

## Examples

```bash
# Create from latest run
assay bundle create

# Create from explicit run id
assay bundle create --run-id 12345

# Create from explicit source path
assay bundle create --from .assay/runs/12345 --output /tmp/replay.tar.gz

# Verify bundle
assay bundle verify --bundle /tmp/replay.tar.gz
```
