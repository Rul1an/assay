# Credential-scope evidence

Status: P59. Producer side (the `required_scope` field on each tool decision) lands with this spec;
the consumer side (scope matching and findings) lives in the review/gate layer.

This builds directly on the [tool-decision surface](tool-decision-surface.md): an observed privileged
tool action already carries a `credential_alias` (the alias the call used, never the token) and a
`category`. Credential-scope adds the question an auditor actually asks: *was this credential
appropriate for what the action required?*

## The load-bearing rule

> Credentials are declared metadata and observed aliases, not verified provider grants.

`required_scope` is Assay's deterministic static claim about what an action class needs. The declared
scopes of an alias are operator-provided metadata. Neither is a provider-verified grant: Assay does
not introspect tokens or query the provider. The evidence is "this alias, declared to hold these
scopes, was used for an action that statically requires this scope", and the match between them.

## Producer: `required_scope` (this PR)

Each classified tool decision now carries `action.required_scope`, derived from the action
`category` (never from arguments):

| category | required_scope |
|----------|----------------|
| `github_deploy_key` | `repo:deploy_key:write` |
| `slack_add_member` | `conversations:members:write` |
| `workspace_admin` | `workspace:admin` |
| (unclassified) | `null` |

`null` means the tool was not classified, read downstream as `required_scope_unknown`, never as "no
scope needed". The `credential_alias` stays where it was, in the redaction block, and is `null`
unless the proxy was configured to map the call to an alias. No raw token ever appears.

## Consumer: declared credentials as policy, scope coverage, findings

### Declared credentials are policy, not a runtime artifact

The scopes an alias is declared to hold are operator configuration, the same shape as the network
declared endpoints. They are a policy map, not a new evidence carrier:

```yaml
declared_credentials:
  github-prod-admin:
    scopes: [repo:admin, repo:deploy_key:write]
  github-readonly:
    scopes: [repo:read]
```

(Modeled internally as `CredentialDeclaration { alias, scopes, metadata? }` so it can migrate to a
carrier later if the scope taxonomy stabilizes, but it stays a policy map in v0.)

### Scope coverage is a small deterministic lattice, not set membership

A declared scope can cover a required scope it is not string-equal to. Matching is an explicit,
deterministic coverage relation, never a plain `required in declared` test:

```text
covers(declared_scopes, required_scope) -> sufficient | insufficient | unknown
```

The initial GitHub lattice, kept deliberately small:

```text
required repo:deploy_key:write is covered by:  repo:deploy_key:write, repo:admin
required repo:deploy_key:write is NOT covered by: repo:read, repo:metadata, repo:contents:read
unrecognized scope string -> unknown (never silently "covered")
```

Broadening the lattice is a deliberate, fixture-backed change, not a guess.

### Findings

| condition | result |
|-----------|--------|
| alias declared and `covers` is sufficient | no finding |
| alias declared and `covers` is insufficient | finding: `credential_scope_insufficient` |
| alias not in `declared_credentials` | finding: `credential_alias_unknown` (never clean) |
| `required_scope` is null (unclassified action) | inconclusive: `required_scope_unknown` |
| an unrecognized scope string in the lattice | inconclusive, never "covered" |
| declared scopes far broader than required | warning: `credential_scope_overbroad` (recommendation, not blocking) |
| a high-privilege admin alias used for the action | informational: `privileged_action_used_admin_alias` |

`overbroad` is least-privilege hygiene, not a regression: without provider introspection it cannot be
proven, so it stays a recommendation. A future policy may opt in to
`treat_overbroad_as_blocking: true`; not in v0.

## Non-claims

- does not introspect tokens or verify provider-side grants;
- `required_scope` is a static claim about the action class, not a provider-verified requirement;
- declared scopes are operator metadata, not proof the alias actually holds them;
- `overbroad` is a recommendation, not a proven least-privilege violation;
- does not expose raw secrets, tokens, or credentials.

## Kill criteria

Stop before token introspection (provider-specific and risky). Static `action_class → required_scope`
mapping plus declared alias metadata only. Start with GitHub; add a provider only with its lattice
entries and fixtures.
