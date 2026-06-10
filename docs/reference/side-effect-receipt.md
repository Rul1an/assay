# Side-effect receipt

Status: Ea (spec + fixtures). Defines the honesty ladder and the binding contract; the recompute
verifier and the consumer rendering are later slices.

The [tool-decision surface](tool-decision-surface.md) already records that a privileged tool returned
success as the provider's *assertion*: `response.side_effect_asserted` may be true while
`response.side_effect_verified` stays false. This spec defines what it takes to honestly move past
asserted, and the one rule that keeps it safe.

## The one rule

> Verified never means "Assay queried the provider." It means an independently produced audit record,
> whose binding Assay recomputes from committed bytes, matches the observed call.

Assay does not hold read credentials for the providers it observes and does not call back to them.
Making the proxy a privileged prober would turn it into the confused-deputy the MCP threat model warns
about, and would break the only identity worth having here: the verifier that reproduces a verdict
from committed bytes, not the actor that re-fetches state. So verification is an *import + recompute*,
never a *query*.

## The ladder

`response.side_effect` carries one `level`, never a bare boolean:

| level | meaning | cost / source |
|-------|---------|---------------|
| `asserted` | the tool returned success | free; the provider's claim, not proof |
| `observed_confirmed` | a later observed read tool-call in the same run returned state consistent with the write | free; pure sequence reasoning over observed calls, no new credentials |
| `verified` | an imported, independently produced provider audit record binds to this call, and Assay recomputed the binding | needs the audit record; never a provider query |

`asserted` never auto-promotes. A higher level is only reached by the evidence its row names, and the
record always says which: `verification_source` is `null`, `observed_read_followup`, or
`provider_audit_import`, and `verification_subject_digest` carries the recomputed binding for the top
two rungs.

## Shape

```json
{
  "response": {
    "status": "success",
    "side_effect": {
      "asserted": true,
      "level": "verified",
      "verification_source": "provider_audit_import",
      "verification_subject_digest": "sha256:..."
    }
  }
}
```

`side_effect.asserted` mirrors the existing `side_effect_asserted` (kept for compatibility);
`level` is the new honest axis.

## The imported audit record and the binding

A `verified` level requires an `assay.provider_audit_record.v0`, produced independently of the run
(exported from the provider's own audit log, then imported), never fetched by Assay:

```json
{
  "schema": "assay.provider_audit_record.v0",
  "provider": "github",
  "record_id": "audit-log-entry-9931",
  "subject": {
    "action_class": "github_deploy_key",
    "verb": "create",
    "target": { "owner": "org", "repo": "prod-repo", "key_title_hash": "sha256:..." }
  },
  "binding_digest": "sha256:...",
  "non_claims": [
    "imported from a provider audit export; Assay did not query the provider",
    "verifies that an audit entry binds to the observed call, not that the entry itself is authentic beyond its own signature"
  ]
}
```

The binding is `sha256(jcs({action_class, verb, target}))` over the canonical subject. Verification
(Eb) recomputes that digest from the imported record's `subject` and requires:

1. the recomputed digest equals the record's `binding_digest` (the record is internally consistent);
2. the same digest equals the digest of the observed tool-decision's `action` projection (the audit
   record binds to *this* call, not merely to some call of the same shape).

Only when both hold does the consumer move the level to `verified`. A record that fails either check
leaves the level at `asserted` and is reported, never silently promoted.

## Non-claims

- does not query providers or hold provider read credentials;
- `verified` proves an imported audit record binds to the observed call, not that the audit record is
  authentic beyond whatever signature it carries (signature trust is a separate, later concern);
- `observed_confirmed` is sequence evidence within the run, not external verification;
- side effects are never promoted past what their evidence supports.

## Reference fixtures

`crates/assay-mcp-server/tests/fixtures/side_effect/`:

- `asserted.json` — allowed success, `level: asserted`, no verification source
- `observed_confirmed.json` — a later read tool-call confirmed the state, `level: observed_confirmed`
- `verified.json` — a matching imported audit record, `level: verified`
- `audit_record_github_deploy_key.json` — the imported `assay.provider_audit_record.v0` whose binding
  matches `verified.json`
- `audit_record_mismatch.json` — an audit record for a different target; binding does not match, so
  the level must stay `asserted`
