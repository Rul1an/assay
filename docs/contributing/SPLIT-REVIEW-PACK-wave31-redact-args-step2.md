# SPLIT REVIEW PACK — Wave31 Redact Args Step2

## Intent
Implement bounded `redact_args` contract/evidence shape while keeping runtime behavior non-mutating and non-blocking.

## Allowed implementation surface
- core MCP policy/runtime metadata paths
- decision event context and emission wiring
- bounded tests for redaction contract/evidence
- Step2 docs/gate files

## What reviewers should verify
1. Diff is bounded to runtime metadata/evidence + tests + Step2 docs/gate.
2. Typed `redact_args` shape exists and is represented in obligations.
3. Additive redaction evidence fields are emitted on decision events.
4. `redact_args` runtime outcome remains contract-only (`Skipped`) in this wave.
5. No args rewrite/mutation/deny path is introduced for `redact_args`.
6. Existing `log`/`alert`/`approval_required`/`restrict_scope` behavior remains intact.

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-wave31-redact-args-step2.sh
```
