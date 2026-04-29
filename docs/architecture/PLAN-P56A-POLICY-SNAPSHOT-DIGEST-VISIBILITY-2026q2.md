# PLAN — P56a Policy Snapshot Digest Visibility (Q2 2026)

- **Date:** 2026-04-29
- **Owner:** Evidence / MCP Security
- **Status:** Execution slice
- **Scope:** Surface the existing canonical policy digest as an explicit,
  self-describing policy snapshot review boundary on supported
  `assay.tool.decision` evidence.

## 1. Goal

Make supported MCP decision evidence explicitly show which canonical policy
snapshot governed the decision, using a digest-bound review surface.

P56a makes the existing policy digest boundary self-describing on supported
`assay.tool.decision` events. It does not add policy truth, policy approval,
policy retrieval, or tool-definition binding.

The slice is deliberately small:

- keep the existing `policy_digest` compatibility field;
- add explicit `policy_snapshot_*` projection fields on `assay.tool.decision`;
- document the digest algorithm, canonicalization, and bounded snapshot schema;
- avoid any claim that the policy itself is correct, sufficient, safe, approved,
  or complete.

## 2. Current Seam

Supported MCP policy paths already compute a deterministic policy digest from
the canonical serialized `McpPolicy` object. P56a makes that digest
self-describing for reviewers and downstream tools rather than leaving it as a
single generic policy field.

The supported projection is:

```json
{
  "policy_digest": "sha256:...",
  "policy_snapshot_digest": "sha256:...",
  "policy_snapshot_digest_alg": "sha256",
  "policy_snapshot_canonicalization": "jcs:mcp_policy",
  "policy_snapshot_schema": "assay.mcp.policy.snapshot.v1"
}
```

`policy_snapshot_digest` is the self-describing projection of the existing
`policy_digest`. In supported decision paths, both fields MUST represent the
same digest value while the compatibility field remains present.

The `jcs:mcp_policy` canonicalization identifier means the digest is computed
over the existing canonical serialization of the `McpPolicy` object using JCS
before SHA-256 hashing, matching the `McpPolicy::policy_digest()` code path.

If `policy_snapshot_digest` is present, the whole cluster is atomic:
`policy_snapshot_digest_alg`, `policy_snapshot_canonicalization`, and
`policy_snapshot_schema` MUST also be present.

## 3. Boundary

This is digest visibility, not policy truth.

P56a does not mean:

- the policy is correct;
- the policy is complete;
- the policy is safe;
- the policy was approved by a reviewer;
- the policy snapshot is retrievable, exportable, or embedded;
- the tool definition was bound to the same snapshot.

P56b covers tool definition digest binding separately.

## 4. Implementation Shape

- Project the snapshot fields from supported `assay.tool.decision` paths that
  already carry `policy_digest`.
- Project only; never infer or reconstruct a policy snapshot after the fact.
- Preserve `policy_digest` for compatibility.
- Keep all fields optional and additive under Evidence Contract v1.
- Keep missing policy digest explicit by omitting `policy_snapshot_digest`; do
  not treat absence as safe.

## 5. Acceptance

- Supported MCP decision events with a policy digest also include the
  `policy_snapshot_*` fields.
- Tests prove `policy_digest == policy_snapshot_digest` when both are present.
- Tests prove the `policy_snapshot_*` cluster is produced atomically, and is
  absent when no policy digest is visible.
- Stable payload parsing accepts the new fields as additive optional data.
- ADR-006 and Evidence Contract v1 document the fields and the non-goal.
- Canonicalization and schema identifiers are defined as code constants and
  documented as spec strings.
- No new Trust Basis claim, Trust Card schema bump, or policy-quality assertion
  is introduced.
