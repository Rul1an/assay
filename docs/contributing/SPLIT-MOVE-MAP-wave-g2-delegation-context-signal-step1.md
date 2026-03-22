# Move map: wave-g2-delegation-context-signal-step1

## Delegation context flow

`_meta.delegation.delegated_from`
- request arguments
- `parse_delegation_context(...)`
- `PolicyMatchMetadata.delegated_from`
- `ToolMatchMetadata.delegated_from`
- `PolicyDecisionEventContext.delegated_from`
- `DecisionData.delegated_from`

`_meta.delegation.delegation_depth`
- request arguments
- `parse_delegation_context(...)`
- `PolicyMatchMetadata.delegation_depth`
- `ToolMatchMetadata.delegation_depth`
- `PolicyDecisionEventContext.delegation_depth`
- `DecisionData.delegation_depth`

## Explicit non-moves

- no `actor_chain`
- no `inherited_scopes`
- no separate `assay.delegation.*` event family
- no pack/probe YAML changes
