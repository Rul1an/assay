# ADR-025 I2 Closure Release Integration (Step4A Freeze)

## Intent
Integrate ADR-025 I2 closure evidence into release/promote flows with an explicit mode contract.

This Step4A slice freezes policy and integration semantics only.
- No workflow changes in this slice.
- No PR required-check behavior changes in this slice.

## Scope
Release integration consumes the nightly closure artifact:
- workflow: `adr025-nightly-closure.yml`
- artifact: `adr025-closure-report`
- files:
  - `closure_report_v1.json`
  - `closure_report_v1.md`

## Gate Modes (v1)
Supported modes:
- `off`: skip closure integration entirely.
- `attach`: fetch closure artifact and attach/persist as release evidence (default).
- `warn`: evaluate closure policy, report non-pass in logs, do not fail release.
- `enforce`: evaluate closure policy and fail release on non-pass.

Default mode:
- `attach`

## Exit Contract (integration script)
- `0`: pass / completed in current mode.
- `1`: policy fail (closure below threshold or hard violations).
- `2`: measurement/contract fail (missing artifact, parse errors, invalid contract).

Mode handling:
- `off`: always returns `0`.
- `attach`: returns `0` on successful fetch/attach, `2` on measurement failure.
- `warn`: converts policy/measurement failures to warnings in release flow.
- `enforce`: propagates `1/2` as hard release failure.

## Policy Source
`schemas/closure_release_policy_v1.json` defines:
- default mode
- score threshold
- minimum readiness window
- classifier version lock
- release evidence requirements

## Safety
- No silent bypass: mode is explicit and logged.
- No PR trigger impact.
- No workflow permission expansion in this Step4A freeze.

## Step4B Preview
Step4B wires release workflow implementation:
1. download latest `adr025-closure-report`
2. apply mode contract (`off|attach|warn|enforce`)
3. attach evidence and optionally enforce

## Step4C Preview
Step4C closes rollout with runbook + promotion criteria for moving from `attach/warn` to `enforce`.
