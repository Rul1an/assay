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
   the code has. The work is done when the number outside the shared reader is zero.

Settled here so that the pull requests do not each reopen it:

- The shared reader lives in `assay-common` behind a feature that is not on by default. It is
  `#[doc(hidden)]` and carries no stability promise until the policy document has landed on it.
- JSON counts only where a user authors the file. Wire messages and evidence are outside this ADR;
  evidence ingest stays bound by ADR-043.
- A refusal names the key by its path as the file spells it, as #3571 set out.

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

The items recorded on #2959 earlier get an answer in that release: the audit of the five registry
enums, #2972, and whether the jsonschema bump from #3176 rides along or stays a minor.

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

- A format cannot be closed without an exception outside the shared reader. The work stops for that
  format and this ADR is revisited.
- A comparison of the released binary and the branch over the same corpus shows a verdict nobody
  listed. The pull request does not land.
- The counter rises.

## Non-claims

- This is a rule about reading. It does not say that a consumer which reads a key does something
  sensible with it.
- Two bypasses pass the compiler and the lints: a discard written with a type annotation, and a
  pattern that skips the rest with `..`. A text check on the document modules will cover them.
- Nothing here changes what a policy decision, an observation or an outcome means.
