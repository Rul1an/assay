# PLAN — P56a Policy Snapshot Digest Visibility (Q2 2026)

## 1. Goal

Make supported MCP decision evidence explicitly show which canonical policy
snapshot governed the decision, using a digest-bound review surface.

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
  "policy_snapshot_digest": "sha256:...",
  "policy_snapshot_digest_alg": "sha256",
  "policy_snapshot_canonicalization": "jcs:mcp_policy",
  "policy_snapshot_schema": "assay.mcp.policy.snapshot.v1"
}
```

## 3. Boundary

This is digest visibility, not policy truth.

P56a does not mean:

- the policy is correct;
- the policy is complete;
- the policy is safe;
- the policy was approved by a reviewer;
- the tool definition was bound to the same snapshot.

P56b covers tool definition digest binding separately.

## 4. Implementation Shape

- Project the snapshot fields from supported decision-context paths that already
  carry `policy_digest`.
- Preserve `policy_digest` for compatibility.
- Keep all fields optional and additive under Evidence Contract v1.
- Keep missing policy digest explicit by omitting `policy_snapshot_digest`; do
  not treat absence as safe.

## 5. Acceptance

- Supported MCP decision events with a policy digest also include the
  `policy_snapshot_*` fields.
- Stable payload parsing accepts the new fields as additive optional data.
- ADR-006 and Evidence Contract v1 document the fields and the non-goal.
- No new Trust Basis claim, Trust Card schema bump, or policy-quality assertion
  is introduced.
