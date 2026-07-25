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
findings follow; the appendix carries the evidence, and reproduction steps for everything except the
authorization paths that are still open:

- The reference verifier applies its own resource ceiling after it has already materialized the
  input. `assay evidence verify` on a 600 MB file peaks at 622 MB resident while
  `VerifyLimits::max_bundle_bytes` is 100 MB. The ceiling is passed correctly and the `LimitReader`
  is a genuine streaming guard; the defect is that `BundleReader::open_internal` reads the input to
  the end first, so that guard streams from a `Cursor` over a `Vec` that is already complete. At
  process level it therefore bounds nothing. The profile requires decompression and size limits
  before any profile parsing without pinning a value, so this is a resource-contract defect in the
  reference implementation against its own stated security consideration — not a divergence in the
  verdict an independent verifier would reach.
- The MCP server emits `"certified": true` and `"partner": "agent_framework"` in every successful
  `initialize` response. Neither carries a basis a reviewer can check, and `meta` is not a protocol
  field at all — the reserved key is `_meta`. The claims-boundary guard that keeps the stop list
  honest scans prose paths only, so it cannot see anything the binaries assert on the wire.
- The bundle fuzz target covers `assay_core::replay::verify_bundle`, a different bundle format from
  the evidence chain, and the replay module carries no resource ceiling at all.
- Token validation runs only in the `initialize` branch. A `tools/call` sent without a handshake
  reaches tool dispatch even with authentication configured in strict mode.
- The default `AuthMode` is `Permissive`, where a correct rejection is downgraded to a pass. The
  algorithm allowlist does reject `alg:none` as designed; the surrounding policy then opens the
  session anyway. The weakness is in the mode, not in the validation.
- Two individually defensible auth decisions compose into a fail-open. In strict mode a non-HTTPS
  JWKS URI is refused and the field is set to `None`; the missing-token check then requires
  `jwks_uri.is_some()` and admits the request. Strict mode with a rejected JWKS URI has no
  authentication left, and a caller who sends no token fares better than one who sends a bad token.
- The sandbox substitutes a built-in policy when the file named by `--policy` fails to load, also
  under `--fail-closed`, and records no identity for the policy that actually applied.

None of these need a new product direction. They need the rules that ADR-042 implies to be stated
so they can be enforced mechanically rather than remembered.

## Decision

1. **Bounded ingest binds every entrypoint that consumes untrusted evidence.** The obligation is not
   limited to functions named "verify": it holds for any entrypoint that parses or semantically
   consumes an untrusted artifact, which today means the bundle reader, the lint engine, the push
   path in both its verifying and `--no-verify` branches, the CLI stdin path, and the replay reader.
   Each applies the whole limit set to the source stream before the input is materialized, and a
   single byte ceiling is not that set. `VerifyLimits` already carries eight dimensions — compressed
   and decoded bytes, per-file sizes, event and line counts, path length and JSON nesting — so a
   decompression bomb is already named by `max_decode_bytes` and nothing needs adding to the
   vocabulary. The defect is the order in which the existing set is applied, plus the replay reader,
   which carries no limits at all. The pattern is already in the tree:
   `assay_evidence::generate_trust_basis` takes `max_bundle_bytes + 1` off the reader before
   reading.

2. **The stop list binds emitted artifacts, not only prose.** ADR-042 §3 already refuses compliance
   and safe-agent claims; a field asserting certification or partner status is such a claim, so this
   is that entry applied to a second surface rather than a new entry. The refusal covers what the
   software puts on the wire and into evidence, on the same terms as what the documentation says. A
   field asserting certification, partnership, or an equivalent status is removed unless it
   carries a basis a reviewer can check. The mechanism is
   a closed set of public wire status claims, asserted structurally against generated responses and
   schema fixtures — not a pattern scan over source literals, which is simultaneously noisy and
   blind to output composed indirectly.

3. **Verification effort follows the golden path.** The evidence-chain verifier is the primary
   target for fuzzing and property testing. Coverage of an adjacent bundle format does not satisfy
   this, and a target that exists but never runs in CI does not either.

4. **Authorization is stated, not grown — and never optional at runtime.** ADR-042 refuses generic
   agent identity and delegation, so this repository does not build an identity provider. Within
   that limit four rules hold, and documenting the current behaviour is not one of the options:
   - With no authentication configured, the server makes no authenticated or certified claim.
   - With authentication configured, missing or unusable key material fails at startup rather than
     passing at runtime. A control that refuses an unsafe configuration never thereby disables the
     enforcement it was protecting.
   - A failed validation never yields an authorized request, in any mode.
   - Every privileged method checks the authorization state of the request it is serving, not of a
     handshake that preceded it. This is also the direction MCP is taking: the
     [`2026-07-28` release candidate](https://blog.modelcontextprotocol.io/posts/2026-07-28-release-candidate/),
     locked on 2026-05-21 and due to publish on 2026-07-28, removes the `initialize`/`initialized`
     handshake (SEP-2575) and the protocol-level session (SEP-2567). Read as of this ADR's date that
     is a candidate rather than a published spec, which affects the timing of the argument and not
     its direction: per-request state is the durable target either way, and a handshake-anchored
     gate would be built on a mechanism the protocol is dropping.

5. **A named policy that cannot be loaded is fatal.** When the operator names a `--policy`, failure
   to load it ends the run, unconditionally. `--fail-closed` does not create that obligation; it
   only makes ignoring it more obviously wrong. Substituting a built-in pack for a named policy does
   not honour the CLI contract, and recording the substitution afterwards does not repair it.
   Substitution is legitimate only where no policy was named and a documented default applies, and
   there the identity of the policy that actually applied is recorded. The golden path already binds
   `declared_policy_digest`; a capability that cannot do the same makes no enforcement statement in
   evidence.

## Consequences

- The invariants in §1 and §3 are testable, so they can move from review attention into CI: a
  per-axis ceiling test for each entrypoint, and a fuzz target aimed at the evidence chain.
- §2 widens the claims boundary beyond prose. This is the mechanism ADR-042 relies on for "cannot
  erode by accretion", and it currently has a blind spot the size of the product. A closed set of
  wire claims costs a fixture to maintain and buys a guard that cannot be evaded by string building.
- §4 accepts a smaller auth surface rather than a larger one, and pays for it with four rules that
  hold at runtime rather than a documented exception. Per-request state, not session state, is the
  target; work anchored to the handshake is work with a known expiry.
- §5 changes default behaviour: a named policy that fails to load stops the run instead of falling
  back. That trades a convenience for an honest contract, and it is the reason this is an ADR rather
  than a bug fix.
- This ADR adds no entry to the ADR-042 stop list and removes none, so it amends rather than
  supersedes. Every example it names maps onto an entry that is already there — the wire claims onto
  the compliance and safe-agent refusal, the claim ceilings onto the whole-action-verdict refusal.
  The kernel-observation posture is unchanged: eBPF validation running post-merge rather than on
  pull requests remains consistent with its supporting status.

Four implementation slices follow from the decisions, in this order:

1. Bounded ingest across the five entrypoints in §1, with per-axis tests.
2. Remove the unfounded wire claims and stand up the closed claim set from §2.
3. Policy-load semantics from §5, including the default-behaviour change.
4. Runtime authorization from §4, and an evidence-chain fuzz target for §3.

## Appendix: reproduction

Measured on `main` at `9fabad8b`, debug binaries, Linux x86_64.

Resource behaviour, reproducible as written:

| Observation | Command | Result |
|---|---|---|
| Ingest exceeds the ceiling | counting reader through `BundleReader::open` vs `verify_bundle_with_limits`, 150 MiB input | 157,286,400 bytes read vs 32,768; ceiling is 104,857,600 |
| Memory tracks input | `assay evidence verify` on 50 / 200 / 600 MB files | 72 / 222 / 622 MB peak resident |
| Policy substitution | `assay sandbox --fail-closed --policy <any file that does not parse as a policy> -- /bin/echo hi` | warning naming the load error, built-in pack applied, child ran, exit 0 |

Authorization behaviour, stated as outcomes. These four were measured the same way, but the
step-by-step recipes are withheld from this record until the §4 slice lands, then restored here. The
findings are what the decision needs; the recipes are not:

| Observation | Result |
|---|---|
| Permissive mode admits a token the validator rejected | session opened, and the response asserted `certified` |
| Strict mode without usable key material admits a request carrying no token | session opened |
| A refused JWKS URI removes the enforcement it was meant to protect | the identical call with an accepted URI is correctly refused; only the scheme differs |
| A privileged method is dispatched without any prior authorization | reached tool dispatch |

The policy-substitution row stays reproducible because it is an operator self-misconfiguration
rather than a path a third party can trigger: it needs write access to the policy file the operator
themselves named, and its effect is a weaker sandbox for that same operator.

Each finding was run against a control so it is not an artifact of the setup. Repacking
`tests/fixtures/evidence/test-bundle.tar.gz` untouched verifies clean, while changing one field of
that bundle to a value of equal byte length fails with `IntegrityManifestHash`; the integrity chain
does what the profile says. For the JWKS composition the two runs differ only in the URI scheme,
which isolates the cause to the refusal path itself.
Finding 1 was independently challenged on the grounds that the verifier streams correctly, and
re-confirmed: it does stream, but only over an already-materialized buffer. The privileged-action
conformance corpus reproduces all thirteen vectors, and every policy-decision gate has a passing
test, including `unknown_required_scope_fails_closed`.
