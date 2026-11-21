# ADR-052: One closed reader and one file form per format, and what a 7.0.0 carries

- Status: Accepted; implementation pending
- Date: 2026-11-16
- Supersedes: none
- Amends: [ADR-026](ADR-026-Protocol-Adapters.md) and its
  [distribution policy](ADR-026-Adapter-Distribution-Policy.md): the ACP and UCP adapters leave the
  workspace. [ADR-002](ADR-002-Trace-Replay.md): from 7.0.0 the `assay.trace` entry it specifies is
  read only by its converter. Records the accepted breaks for the next-major ledger (#2959).

Nothing in this ADR is implemented at `827a0bed`.

## Context

Between 6.22.3 and 6.27.0, and again in the unreleased section, the changelog carries the same kind
of entry many times: a reader accepted a key, did nothing with it, and now refuses it. #3508, #3511,
#3517, #3522, #3523, #3535, #3561, #3563, #3565, #3568, #3569 and #3575 are of that kind. #3512,
#3513, #3518, #3521 and #3527 are the neighbouring case: two commands read the same file and answered
differently.

Each of those fixes closes the gap for the readers it names, and no other. Measured on `origin/main`
`827a0bed`:

- `grep -rnE 'serde_yaml::from_(str|reader|slice|value)' crates/*/src`, with paths under `tests/`
  excluded, finds 57 lines in 32 files across six crates. Some of those lines sit in `#[cfg(test)]`
  modules, and the pattern misses the readers that go through `serde_yaml::Deserializer`, the eval
  config loader among them. The number is an indication; the counter in decision 5 is the measure.
- A sequence policy file is parsed by four functions, which do not agree on the order in which to
  try its forms: `assay-core/src/model/validation.rs:1012-1049`,
  `assay-metrics/src/sequence_valid.rs:70-81`,
  `assay-mcp-server/src/tools/check_sequence.rs:54-61` and
  `assay-cli/src/cli/commands/coverage/legacy.rs:74-95`.
- Three readers hold their types in a private module under `#![forbid(dead_code)]`, so that a field
  nothing reads fails the build (`mcp/trust_policy.rs`, `proxy/enforce/policy.rs`,
  `monitor_next/view.rs`). Nothing makes the next reader do the same.

#3561 states the rule for the monitor sections: every key is acted on or refused. What is missing is
a place where that rule holds for every reader without each one remembering it.

## Decision

1. **One reader per format.** Bytes become a document in one function. No command parses a
   user-authored file itself.

2. **Document types are closed.** A type a file is read into is declared through one declaration
   macro. Its grammar has no way to write `flatten`, `untagged` or any other serde attribute, and its
   expansion refuses a field whose type is not itself closed. A payload that is open on purpose, such
   as a JSON Schema body, is marked as such and can be counted.

3. **Every part of a document is acted on or named.** A consumer takes its part of the document
   through a pattern that names every field. For each one it either acts, or says which reader the
   part belongs to. A part added to the document does not compile until every consumer has said
   which.

4. **One form per format.** An older form is read only by its converter, through its own closed type.
   The reader of the current form has no fallback.

5. **A counter that only falls.** The entry points of the YAML crate are banned for the workspace
   under its real name (`yaml_serde`; the workspace renames it to `serde_yaml`, and a ban written with
   the alias matches nothing), together with `serde_ignored::deserialize`. Each file with an
   existing call is named in a ledger, with its number of calls. A job in the required `CI`
   aggregate counts the calls themselves in the library and binary targets, whatever attribute
   sits on them, and fails when a file's number rises and when the ledger names more calls than
   the code has. For 7.0, one counted call outside the shared reader is admitted for generic
   YAML repair: `assay_core::fix::yaml_patch_target` calls the common syntax reader once.
   The public patch API can edit a tree that fails policy validation and preserves the YAML
   parser error and its source location on malformed input. Callers can downcast that error
   to `serde_yaml::Error`; a parser upgrade must preserve or explicitly migrate this contract.
   The writer projects through JSON and may change comments, anchors and formatting. The
   planner uses the repair path only to check source values after the typed policy loads.
   A future editor that preserves presentation would replace this repair path.
   Loading a document for execution or validation must still use its format reader. The
   counter reports this call, and its ledger entry is removed if the call goes away.
   The counter checks counts per file. Review checks that the remaining call serves only
   this repair path. Any other call outside the shared reader remains a cut blocker.

Settled here so that the pull requests do not each reopen it:

- The shared reader lives in `assay-common` behind a feature that is not on by default. It is
  `#[doc(hidden)]` and carries no stability promise until the policy document has landed on it.
- JSON counts only where a user authors the file. Wire messages and evidence are outside this ADR;
  evidence ingest stays bound by ADR-043.
- A refusal names the key by its path as the file spells it, as #3571 set out.

A lint pack's existing decoder reads authored scalars as text and reports the first invalid
field with its Serde diagnostic. The shared reader preserves that behavior through a
`strict root`. Its `strict part` structs and `strict` tagged checks reject unknown fields
during the same decode. Ordinary `strict struct` continues to check only its own fields.
The nested declarations must implement `StrictValue` through the grammar. The pack marks
open JSON fields with `opaque_json` and the catch-all check with `future`. These markers
preserve the payload forms the decoder already accepts and its duplicate-key checks.

The registry canonicalizes a document before the pack engine reads its fields. Its
`canonical_registry` root retains Assay’s existing subset of SPEC-Pack-Registry-v1 §6.1, including
its integer-only numeric values. Every accepted mapping key contributes to the canonical bytes. The shared reader owns
syntax and value validation. The registry owns JCS encoding, digests and signature verification.

## Leaves the workspace on 6.x

`assay-adapter-acp` and `assay-adapter-ucp`. No crate depends on either and neither is published, so
this is not a break. It does change ADR-026's distribution policy, which kept them available from
workspace builds on `main`.

`gateway-evidence-replay` and `assay-adapter-a2a` stay. The first is a deliberate second reader with
a parity test against the CAP-1 corpus, and two built-in packs judge output of the second.

## What a 7.0.0 carries

**One form per format.** The eval config without a version and the untagged `expected` shapes; the
first form of the MCP policy (root `allow` and `deny`, `constraints`, `version: "1.0"`); the args
policy whose root keys are tool names, and `tools.arg_constraints`; the list forms of a sequence
policy file and `aliases` in such a file; a test that names an external policy file; and the trace
line without a `type` together with the `assay.trace` entry that ADR-002 specifies and `assay init`
writes today. Before
the cut each form has a converter; where none exists today, the 6.x work adds one.

The first MCP policy form was announced for removal in 2.0.0 and was not removed
(`mcp/policy/legacy.rs`, `docs/migration/v1-to-v2.md`). This ADR moves that to 7.0.0, and the 6.x
work corrects the warning and the migration page.

With the forms go the public items that exist only for them or that nothing reads:
`RegistryConfig::allow_unsigned` and `with_allow_unsigned`, `AuthConfig` and `AuthMode`, the public
types of the older forms, and the key-use functions of the MCP policy module. #3406 rides along.

Two public byte-level readers that no Assay code calls go as well:
`assay_core::mcp::policy::read_section`, released in 6.27.0, and
`assay_core::unread_keys::from_yaml_str`, released in 6.28.0. Each parsed YAML outside
the shared reader and was in the ledger only for its own calls.

A caller of `read_section(bytes, key)` loads the policy and projects the section from the loaded
source. `PolicySource::section` carries the compiled example:

```rust
let (_, source) = assay_core::mcp::policy::McpPolicy::from_slice_with_source(bytes)?;
let section = source.section::<KillSwitch>("kill_switch")?;
```

For a policy key, `section` decodes the section and names its unread keys the way `read_section`
did, and a file that holds only the section still loads. Three cases differ, because
`read_section` read the raw tree and `section` reads the loaded source:

- A policy in the first form is refused. It converts with `assay policy migrate`.
- A file with unknown or invalid sibling keys, such as a policy section embedded among another
  tool's keys, is refused.
- A section under a root extension key such as `x-example` loads with the policy but reads as
  absent, because the loaded source removes root extension keys. `read_section` returned it.

The last two have no Assay route. The caller reads its own YAML source: it parses the bytes, takes
the key, and decodes it, with `serde_ignored` when it wants the unread keys. The rustdoc of
`PolicySource::section` compiles both the absence and this route on an accepted file.

`from_yaml_str` has no replacement in the public API. A downstream reader wraps its own YAML
deserializer with `serde_ignored`. The paths it reports differ from the removed function's:
`serde_ignored` writes `?` for each `Option` it passes through, so a caller that compares paths
builds them from the path segments.

For the stdio MCP server, remove the public `ServerConfig::auth` field and the entire
`TokenValidator` type, including `new`, `new_with_static_key`, and `validate`.
Downstream stdio configuration can omit `auth` and use `..ServerConfig::default()`.
An application that calls the validator directly must move its token checks to its own
authentication boundary. Before upgrading, test accepted and rejected tokens against the
application’s signing algorithm, header, issuer, audience, clock skew, and resource rules.
`Claims` and `JwksProvider` stay public under their existing module paths and re-exports.

On the final cut, compile an external consumer that sets a stdio configuration override,
uses `Claims`, and receives an `Arc<jsonwebtoken::DecodingKey>` from
`JwksProvider::get_key`. This checks Rust type compatibility. The stdio boundary tests
separately verify rejection of `ASSAY_AUTH_*` before protocol I/O.

The cut also changes the public Rust struct `assay_registry::LockedPack` to retain the
`namespace`, `fetched_at`, and `etag` fields from the [pack registry specification,
section 8.2](SPEC-Pack-Registry-v1.md#82-lockfile-format). Reading and writing an unchanged
lockfile preserves each field's value on the same pack, including an explicit null namespace.
A field absent from a bundled or BYOS entry stays absent. Generating a lockfile may only
record metadata available for that resolution. Updating a digest must not attribute the old
fetch metadata to the new content.

External callers constructing `LockedPack` with a Rust struct literal will need to supply the
new fields. The Migration section and equal-version witness name this break. The acceptance
test reads the specification's YAML example and compares its entries after serialization; it
also exercises an explicit namespace and an omitted namespace. Lockfile metadata on its own
does not establish which registry served a pack or verify its signature.

For the five enums audited under #2959, 7.0 preserves exhaustive matches on
`assay_evidence::{ErrorCode, Cap1Rule, Cap1Gap, Cap1BasisKind, Cap1Disposition}`.
Adding `#[non_exhaustive]` would make consumers add wildcard arms. This cut adds no variant
that requires such a migration.

For #3176, 7.0 retains the jsonschema 0.55 `Validator` in the public `assay-core` API.
The downstream compile witness checks a consumer passing its own validator and receiving
Assay's validators. A later dependency upgrade that changes this type needs its own migration
decision.

For #2972, 7.0 keeps Aya 0.13 on Linux. A Rust caller can match the Aya errors in
`assay_monitor::MonitorError`, inspect their fields through the error source, and use the
four `assay-common` types implementing `aya::Pod` with Aya's typed maps. A caller built
against Aya 0.13 cannot use those same types through Aya 0.14 without migrating its dependency.
An upgrade that changes these public type identities requires a breaking-change decision.
The coordinated dependency update also needs the kernel and smoke evidence specified in #2972.

## Order

Three phases. First, on 6.x, no public API breaks and no file is refused that loads today: the shared
reader and the counter arrive, the repository's own files move to the current forms, and every older
form gets a converter test. Then, still on 6.x, each format moves behind the shared reader, starting
with the policy document. These changes refuse files that loaded before, as the last releases did,
and each carries a Migration section. Last, one pull request raises the version to 7.0.0 and removes
the older forms and the public remnants.

The cut is one pull request for a reason the semver gate gives itself:
`scripts/ci/test-semver-gate.sh` records that after a declared major `check-release` skips every
check. Inside that window the gate proves nothing, so the cut carries a witness run with the versions
set equal. Its output names only items this ADR lists, and the pull request names each one.

## Alternatives considered

- **Keep fixing reader by reader.** That is what 6.22.3 through 6.27.0 did. It works for the reader
  that is fixed and says nothing about the next one.
- **Keep the older forms readable for good.** #2959 notes that an additive route is usually the
  better design, and the 6.x phase is that route: nothing is removed until every form has a
  converter. What the cut removes is the second way to write the same intent, because every form a
  reader accepts is a form two readers can disagree about.

## What stops the work

- A format cannot be closed without an exception outside the shared reader. Work stops for that
  format and this ADR is revisited. The repair exception in decision 5 covers editing an authored
  tree only. It cannot be used to admit a document to a format reader.
- A comparison of the released binary and the branch over the same corpus shows a verdict nobody
  listed. The pull request does not land.
- The counter rises.

## Non-claims

- This is a rule about reading. It does not say that a consumer which reads a key does something
  sensible with it.
- Two bypasses pass the compiler and the lints: a discard written with a type annotation, and a
  pattern that skips the rest with `..`. A text check on the document modules will cover them.
- Nothing here changes what a policy decision, an observation or an outcome means.
