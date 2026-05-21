# Phase 1 Delegated Proof Pack

This proof pack preserves the durable metadata that remains available for the
Phase 1 delegated Linux/eBPF acceptance run.

It is intentionally narrower than a full raw bundle archive, but it stores the
workflow sources needed to verify the run metadata and PASS-line excerpt after
GitHub Actions log retention expires. The historical workflow run did not
upload GitHub artifacts, and the runner wrote the per-gate tarballs to
temporary `/tmp` paths that were cleaned after the job. This pack therefore
records the workflow identity, commit, successful job metadata, full workflow
metadata JSON, gzipped workflow log, pass-line excerpt, and the v0 golden-shape
artifact digests that were available after the Phase 2A consolidation work.

The manifest field `created_at` is the time this retention manifest was
assembled. The source workflow run time is recorded separately under
`source.created_at`.

## Source Run

| Field | Value |
|---|---|
| Workflow | `Runner Spike Delegated` |
| Run | `26211485614` |
| URL | <https://github.com/Rul1an/assay/actions/runs/26211485614> |
| Event | `workflow_dispatch` |
| Branch | `codex/assay-runner-drop-kernel-stream-before-stats` |
| Commit | `56571045825de586c459469bfb07ac403611b225` |
| Commit short | `56571045` |
| Conclusion | `success` |
| Created | `2026-05-21T07:17:44Z` |
| Updated | `2026-05-21T07:32:39Z` |

## Retained Files

- [`phase1-delegated-2026-05-21.manifest.json`](phase1-delegated-2026-05-21.manifest.json)
- [`phase1-delegated-2026-05-21.workflow-metadata.json`](phase1-delegated-2026-05-21.workflow-metadata.json)
- [`phase1-delegated-2026-05-21.workflow-log.txt.gz`](phase1-delegated-2026-05-21.workflow-log.txt.gz)
- [`phase1-delegated-2026-05-21.log-excerpt.txt`](phase1-delegated-2026-05-21.log-excerpt.txt)
- [`../golden/observation-health-openai-agents-kernel-policy-v0.json`](../golden/observation-health-openai-agents-kernel-policy-v0.json)
- [`../golden/capability-surface-openai-agents-kernel-policy-v0.json`](../golden/capability-surface-openai-agents-kernel-policy-v0.json)
- [`../golden/correlation-report-openai-agents-kernel-policy-v0.json`](../golden/correlation-report-openai-agents-kernel-policy-v0.json)

## Preserved Assertions

The log excerpt preserves the PASS lines for all Phase 1 proof modes:

| Gate | Preserved result |
|---|---|
| `kernel-only` | 3 acceptance runs plus three-run determinism passed |
| `kernel-policy` | 3 acceptance runs plus three-run determinism passed |
| `openai-agents-kernel-policy` | 3 acceptance runs plus three-run determinism passed |

The manifest records SHA-256 digests for the fetched workflow metadata,
gzipped workflow log, raw workflow log content, log excerpt, and v0
golden-shape JSON files.

The retained capability-surface golden shape follows the Phase 2A precision
rename from `filesystem_prefixes` to `filesystem_paths`. Because the historical
run did not retain raw runner archives, this proof pack preserves the delegated
run proof plus the current v0 contract anchors; it does not claim a byte replay
of the pre-rename raw capability-surface archive.

## Limits

This pack does not claim to contain the original `runner-*.tar.gz` archives
from the delegated host. It also does not replace a future re-dispatch when a
review requires fresh delegated proof.

Future delegated acceptance workflows should upload a first-class proof-pack
artifact during the run so archive digests and selected extracted JSON
artifacts are retained at the time of execution. That workflow follow-up is
tracked in <https://github.com/Rul1an/assay/issues/1287>.
