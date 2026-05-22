# Gemini Identity Preservation Probe — Maintainer Curation

This directory contains the **identity preservation probe** required by
[issue #1307](https://github.com/Rul1an/assay/issues/1307) as the first
implementation step for the Gemini Python `google-genai` second-runtime
fixture.

The probe is **maintainer-only curation**, not part of CI or delegated
acceptance. It exists so the level-3 stable-identity assumption that
#1305 selected on — *"Gemini 3 model APIs now generate a unique `id` for
every function call"* — is verified empirically before the rest of the
fixture is built on top of it.

## What the probe does

1. Records one non-streaming `client.models.generate_content()` call
   against `gemini-3.5-flash` with a single `read_file` function tool,
   using a maintainer-supplied Gemini API key. The recording is saved
   to `cassettes/identity-probe.yaml`.
2. Replays the recorded cassette with no key and no network.
3. Asserts that `FunctionCall.id` is present, non-empty, and
   byte-identical between record and replay.

If any of those assertions fail, one or more of the hard kill criteria
in [issue #1307](https://github.com/Rul1an/assay/issues/1307) has fired
and the implementation line must stop.

## What the probe does NOT do

- It is not a fixture. It does not emit normalized runner artifacts.
- It does not run in delegated CI.
- It does not authorize fixture implementation by passing — it only
  removes one specific blocker. The full fixture PR has its own
  acceptance criteria.
- It does not commit live API keys or any auth credential under any
  circumstance.

## Prerequisites

- Python 3.10 or later (any version that supports `google-genai 2.6.0`)
- A live Gemini API key — obtained out-of-band by the maintainer
- No requirement that the workstation be the delegated runner; the
  probe is portable.

## One-time setup

From this directory:

```sh
python3 -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
```

## Running the probe

```sh
# Record one cassette using a live API key (key never written to disk
# by the probe; the recording strips auth headers and query params
# before writing the cassette):
GEMINI_API_KEY=<live-key> python3 identity_probe.py --record

# Replay the recorded cassette with no key, no network:
python3 identity_probe.py --replay

# One-shot record + replay + compare:
GEMINI_API_KEY=<live-key> python3 identity_probe.py --record-and-replay
```

The probe writes its outcome to `probe-results/identity-probe-<UTC-date>.json`.
That outcome file is the artifact the maintainer commits alongside the
cassette to anchor the probe result in repo history.

## Recording the canonical fixture cassette

After the identity probe passes, record the canonical delegated-replay
cassette with the same fixture code the acceptance script will use:

```sh
GEMINI_API_KEY=<live-key> python3 fixture.py --record /tmp/assay-runner-gemini-fixture-record
unset GEMINI_API_KEY
```

This writes `cassettes/fixture.yaml`. It is separate from
`cassettes/identity-probe.yaml`: the probe cassette proves the level-3
identity assumption, while the fixture cassette is the source of truth for
delegated acceptance replay.

Before committing `cassettes/fixture.yaml`, run the same redaction checks as
for the probe cassette and verify that the cassette contains exactly one
human-reviewable `functionCall.id`.

## Required curation steps before commit

After a successful probe run:

1. **Verify cassette redaction**: open `cassettes/identity-probe.yaml`
   and confirm that no `x-goog-api-key`, `Authorization`,
   `X-Goog-User-Project`, query `key`, or query `access_token` values
   appear in clear text. The probe's VCR.py configuration strips these
   automatically, but maintainer review is required before commit.
2. **Run a secret scanner**: the cassette MUST be scanned by a secret
   scanner (GitGuardian, `detect-secrets`, or equivalent) before commit.
   This is a hard requirement, not a recommendation.
3. **Confirm `FunctionCall.id` preserved**: in the outcome JSON, verify
   that `function_call.id` is present and non-empty, and that the
   probe `passed` field is `true`.
4. **Commit cassette + outcome together**: the cassette and the
   matching outcome file should land in the same commit so future
   readers can correlate them.

## What to do if the probe fails

If the outcome has `passed = false`, do **not** proceed with the
fixture implementation. Instead:

- If `FunctionCall.id` was missing in the response: kill criterion 1
  fired. The level-3 selection in #1305 must be re-evaluated, because
  the documentation guarantee did not hold for the recorded call.
- If `FunctionCall.id` was synthesized by the SDK or differed between
  record and replay: kill criterion 2 or 3 fired. The selection rests
  on direct propagation; if that breaks, the candidate evaluation
  must be updated.
- In all failure cases: open a follow-up evaluation PR in
  `second-runtime-candidate-selection.md` documenting what the probe
  observed, or open a separate decision PR for the relevant follow-up
  issue.

Do not work around a failing probe by editing the cassette manually,
by synthesizing a `tool_call_id`, or by suppressing assertions.

## Why this lives in repo

The probe tooling is checked in so:

- the procedure is reproducible by any maintainer who obtains a key
- the assumption #1305 selected on is documented as testable
- the cassette redaction contract is enforced by code, not by
  out-of-band convention
- future maintainers re-recording the cassette (per the bump flow in
  [`fixtures-v0.md` § Dependency Upgrade Contract](../../docs/reference/runner/fixtures-v0.md#dependency-upgrade-contract))
  follow the same discipline

## References

- Issue requiring the probe:
  <https://github.com/Rul1an/assay/issues/1307>
- Candidate selection that the probe verifies:
  [`second-runtime-candidate-selection.md` § Candidate: Gemini Python `google-genai` direct](../../docs/reference/runner/second-runtime-candidate-selection.md#candidate-gemini-python-google-genai-direct)
- Fixture design that scopes the probe:
  [`gemini-fixture-design.md`](../../docs/reference/runner/gemini-fixture-design.md)
