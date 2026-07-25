# ADR-043: Evidence-chain integrity invariants

- Status: Proposed
- Date: 2026-07-25
- Supersedes: none
- Amends: ADR-042 (evidence-first positioning and scope freeze)

## Context

ADR-042 made the evidence artifact the product and the enforcing proxy its reference producer. Two
of its consequences carry obligations that were never written down as testable rules: claims must be
checkable by someone who does not trust us, and the stop list must not erode by accretion.

A verification pass over the current `main` found that both obligations have gaps at the exact
places ADR-042 makes load-bearing, while the decision layer it describes holds up well. The measured
findings, with reproduction steps in the appendix:

- The reference verifier applies its own resource ceiling after it has already materialized the
  input. `assay evidence verify` on a 600 MB file peaks at 622 MB resident while
  `VerifyLimits::max_bundle_bytes` is 100 MB. An independent verifier written from the profile text
  would apply the ceiling first and behave differently from the reference on the same input.
- The MCP server emits `"certified": true` in every successful `initialize` response, including a
  session opened with a forged `alg:none` token. The claims-boundary guard that keeps the stop list
  honest scans prose paths only, so it cannot see anything the binaries assert on the wire.
- The bundle fuzz target covers `assay_core::replay::verify_bundle`, a different bundle format from
  the evidence chain, and the replay module carries no resource ceiling at all.
- Token validation runs only in the `initialize` branch. A `tools/call` sent without a handshake
  reaches tool dispatch even with authentication configured in strict mode.
- The sandbox substitutes a built-in policy when the file named by `--policy` fails to load, also
  under `--fail-closed`, and records no identity for the policy that actually applied.

None of these need a new product direction. They need the rules that ADR-042 implies to be stated
so they can be enforced mechanically rather than remembered.

## Decision

1. **Bounded ingest is a verifier contract, not an optimization.** Every verifier entry point
   applies its byte ceiling to the stream before the input is materialized. Reading an untrusted
   artifact into memory ahead of the limit check is a contract defect regardless of what the
   subsequent verification concludes. This binds the evidence bundle reader, the lint engine, the
   evidence push path, the CLI stdin path, and the replay reader, which has no ceiling today. The
   pattern is already in the tree: `assay_evidence::trust_basis::generation` takes
   `max_bundle_bytes + 1` before reading.

2. **The stop list binds emitted artifacts, not only prose.** ADR-042 §3 refuses compliance and
   safe-agent claims. That refusal covers what the software puts on the wire and into evidence, on
   the same terms as what the documentation says. A field asserting certification, partnership, or
   an equivalent status is removed unless it carries a basis a reviewer can check. The
   claims-boundary guard's scope extends to emitted literals, so the guard covers the product and
   not only the description of it.

3. **Verification effort follows the golden path.** The evidence-chain verifier is the primary
   target for fuzzing and property testing. Coverage of an adjacent bundle format does not satisfy
   this, and a target that exists but never runs in CI does not either.

4. **Session authorization is stated, not grown.** ADR-042 refuses generic agent identity and
   delegation, so this repository does not build an identity provider. It does owe an honest
   boundary: a handshake-only check is not request authorization, and must not be presented as one.
   Either the privileged method surface refuses an unauthenticated session, or the auth boundary is
   documented as out of the trust boundary and the default mode of a released binary does not accept
   a token it failed to validate.

5. **A supporting capability records what it enforced, or states nothing about enforcement.** When
   an enforcement path substitutes a different policy than the operator named, that substitution is
   either fatal or recorded with the identity of the policy that actually applied. The golden path
   already binds `declared_policy_digest`; a capability that cannot do the same makes no
   enforcement statement in evidence.

## Consequences

- The invariants in §1 and §3 are testable, so they can move from review attention into CI: a
  resource-ceiling test per verifier entry point, and a fuzz target aimed at the evidence chain.
- §2 widens the claims-boundary guard beyond prose. This is the mechanism ADR-042 relies on for
  "cannot erode by accretion", and it currently has a blind spot the size of the product.
- §4 accepts a smaller auth surface rather than a larger one. Making the boundary explicit is
  cheaper than building session identity, and it is what the stop list already asks for.
- §5 keeps the supporting capabilities inside the same epistemology as the golden path without
  promoting them to co-equal product directions.
- This ADR adds nothing to the ADR-042 stop list and removes nothing from it, so it amends rather
  than supersedes. The kernel-observation posture is unchanged: eBPF validation running post-merge
  rather than on pull requests remains consistent with its supporting status.

## Appendix: reproduction

Measured on `main` at `9fabad8b`, debug binaries, Linux x86_64.

| Observation | Command | Result |
|---|---|---|
| Ingest exceeds the ceiling | counting reader through `BundleReader::open` vs `verify_bundle_with_limits`, 150 MiB input | 157,286,400 bytes read vs 32,768; ceiling is 104,857,600 |
| Memory tracks input | `assay evidence verify` on 50 / 200 / 600 MB files | 72 / 222 / 622 MB peak resident |
| Forged token accepted | `initialize` with an `alg:none` token, default mode | session opened, `"certified": true` returned |
| No token accepted | `initialize` with no token, `ASSAY_AUTH_MODE=strict`, no JWKS | session opened |
| Call without handshake | `tools/call` with no prior `initialize`, strict mode with JWKS | reached tool dispatch |
| Policy substitution | `assay sandbox --fail-closed --policy <broken>` | warning, built-in policy applied, child ran, exit 0 |

Two controls were run so the findings are not artifacts of the setup. Repacking the untouched
fixture bundle verifies clean, while changing one field of equal byte length fails with
`IntegrityManifestHash`; the integrity chain does what the profile says. The privileged-action
conformance corpus reproduces all thirteen vectors, and every policy-decision gate has a passing
test, including `unknown_required_scope_fails_closed`.
