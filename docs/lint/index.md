# Lint Rules Reference

`assay evidence lint` verifies an evidence bundle and then applies a fixed registry of rules to
every event in it. This page is the target of the `helpUri` that each finding carries in SARIF
output, so every rule below has a stable anchor and the emitter is tested against this file.

```bash
assay evidence lint bundle.tar.gz --format sarif
```

**Verification runs first.** Lint never reads an unverified bundle. If verification fails the
command exits 2 and no rules run, so a lint finding always describes a bundle whose integrity
already held.

**Exit codes.** `0` no findings above the threshold, `1` findings, `2` verification failure.

## Reading a clean run

A run with no findings means no rule in the registry matched. It is not a statement that the
bundle is free of the problems these rules look for. Three of the six rules key on structural
fields that an event may simply not carry — `ASSAY-W004` needs a refusal marker, `ASSAY-W005`
needs an `approval_retained_view` field, `ASSAY-W002` needs the PII flag set — and an event that
omits the field is silent rather than clean. The secret-pattern rules match a fixed list of
literal prefixes and will not find a credential shaped differently.

A passing lint is a lower bound on what was checked, not a coverage claim.

## Severity and `security-severity`

Rules carry a default severity (`error`, `warning`, `note`) and security-relevant rules also carry
a CVSS-like `security-severity` used by GitHub Code Scanning to bucket alerts. The two are
independent: `ASSAY-W005` is a warning with no `security-severity`, because an unreadable approval
basis caps what a review can claim rather than describing an exploitable weakness.

---

## ASSAY-W001 — Subject may contain a secret {#assay-w001}

**Severity** warning · **security-severity** 7.0 · **Tags** `security`, `secrets`

Fires when `event.subject` contains any of a fixed list of literal credential prefixes, compared
case-insensitively: `sk-`, `sk_live_`, `sk_test_`, `api_key=`, `apikey=`, `token=`, `password=`,
`secret=`, `Bearer `, `AKIA` (AWS access key), `ghp_`, `gho_`, `github_pat_` (GitHub tokens). The
message names the pattern that matched.

**Why it matters.** A subject travels in the manifest and in every projection of the bundle. A
credential there is disclosed to everyone who can read the evidence, including anyone the bundle
is shared with for audit.

**How to fix.** Redact the value at the producer before the event is written. Rewriting the bundle
afterwards changes its content hashes and invalidates the Merkle root.

**Note.** The match is a literal substring test, not a validator. It has both false positives
(`token=` in prose) and false negatives (a credential with no recognised prefix).

## ASSAY-W002 — PII flag set with a non-empty subject {#assay-w002}

**Severity** warning · **security-severity** 4.0 · **Tags** `privacy`, `pii`

Fires when `contains_pii` is true and `subject` is present and non-empty.

**Why it matters.** The flag is the producer stating that this event carries personal data. A
non-empty subject on such an event is the most likely place for that data to be sitting in the
clear, and the subject is the field most widely reproduced downstream.

**How to fix.** Redact or omit the subject on PII-bearing events. If the subject is genuinely free
of personal data, the flag is describing the payload only — that is a legitimate shape, and the
finding is advisory.

## ASSAY-W003 — Secret pattern present but `contains_secrets` is false {#assay-w003}

**Severity** warning · **security-severity** 6.5 · **Tags** `security`, `secrets`

Fires when `subject` matches any pattern from the `ASSAY-W001` list and `contains_secrets` is
false.

**Why it matters.** This is a disagreement between what the producer declared and what the record
contains. A consumer filtering on `contains_secrets` to decide what may be shared will treat the
event as safe, so the flag being wrong is worse than the secret being present and declared.

**How to fix.** Correct the producer so the flag reflects the content, then redact.

**Note.** `ASSAY-W001` and `ASSAY-W003` share one pattern list, so an undeclared secret raises
both: one for the disclosure, one for the mislabelling. They are not duplicates.

## ASSAY-W004 — Refusal observation not backed by a decision record {#assay-w004}

**Severity** warning · **security-severity** 4.0 · **Tags** `security`, `enforcement`,
`attribution`

Fires when an event carries a proxy refusal marker and the bundle contains no
`assay.enforcement_decision.v0` record with a `deny` decision bound to the same
`(tool_name, target_digest)` pair. Two distinct messages:

- **unbacked** — no decision record for that key at all.
- **contradicted** — a decision record exists for that key and says `allow`.

**Why it matters.** "The call was denied" is an attribution claim: it says an enforcement point
acted. Without a digest-bound decision record, the observation asserts an actor that left no
trace. The contradicted case is stronger — an audit record and the enforcement record disagree
about the same digest, and that conflict is itself the finding.

**How to fix.** Ensure the enforcing component emits `assay.enforcement_decision.v0` bound to the
same target digest the refusal marker uses. A refusal recorded by one component and a decision
recorded by another must agree on the digest, or they cannot be joined.

## ASSAY-W005 — Approval basis declares an unreadable retained view {#assay-w005}

**Severity** warning · **Tags** `retention`, `review`, `sufficiency`

Applies only to `assay.enforcement_decision.v0` events that carry an `approval_retained_view`
field. An absent field is not a finding — the record makes no retained-view claim, so the rule is
out of scope.

Silent for the one readable value, `structured_meta_jcs`, which is a digest-recomputable
structured basis. Every other value is treated as not readable, fail-closed, in three cases:

| declared view | reading | recovery |
|---|---|---|
| `encrypted` **with** `approval_plaintext_commitment` | `opaque_bindable` | a later disclosure is checkable against the commitment |
| `encrypted` **without** the commitment | `opaque_unbindable` | key disclosure only |
| unknown value, empty string, or non-string | fail-closed | correct the producer |

**Why it matters.** A content-review claim over an approval nobody can read is not a review. The
rule caps such claims at *incomplete* rather than rejecting the bundle, because integrity is a
floor and never a lift: a bundle can verify perfectly and still not support the claim being made
over it.

**How to fix.** Emit `structured_meta_jcs` where the approval basis is structured. Where the body
genuinely must be encrypted, emit `approval_plaintext_commitment` alongside it so a later
disclosure can be bound to what was approved at the time.

## ASSAY-I001 — Source does not follow the URN convention {#assay-i001}

**Severity** note · **Tags** `convention`, `format`

Fires when `event.source` starts with neither `urn:` nor `https://`.

**Why it matters.** Convention only. A source that is neither a URN nor an absolute URL is not
resolvable or globally unique, which makes correlation across bundles from different producers
unreliable.

**How to fix.** Use `urn:` for logical producers and `https://` where the source is a real
resolvable endpoint.

---

## Truncation disclosure

When a run produces more findings than the configured cap, the report discloses it in two places:

- `run.properties.appliedCap` — the ceiling in force, declared on every run whether or not it was
  reached, so silence is never ambiguous between "no bound" and "a bound that did not fire".
- a `toolExecutionNotifications` entry at `warning` carrying `droppedCount` — emitted only when a
  drop actually occurred.

`executionSuccessful` stays `true` on a truncated run. Per SARIF §3.20.21 an `error`-level
notification asserts the run failed; a bounded report over a complete analysis is not a failed
run, and reporting it as one would make a deliberate policy indistinguishable from an
environmental fault.

This shape follows the cross-emitter envelope agreed in
[`aliksir/claude-code-skill-security-check#24`](https://github.com/aliksir/claude-code-skill-security-check/issues/24),
where the split between run-level configuration and run-specific event was settled.

## Known gap

The `helpUri` values point at this page as published from the default branch, not at the release
that emitted the report. A consumer reading an old SARIF file gets the current text, which may
describe a rule that has since changed. Release-tagged pointers would fix it and are not
implemented.
