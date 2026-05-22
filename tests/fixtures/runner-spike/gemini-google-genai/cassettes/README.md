# Gemini Identity Probe Cassettes

This directory holds checked-in cassettes for the Gemini identity preservation
probe and (in a later PR) the Gemini second-runtime fixture.

**Cassette content is added by maintainer curation only.** This README is a
placeholder so the directory exists in repo; it does not contain any cassette
data yet.

## Redaction contract

Per the cassette redaction contract in
[issue #1307](https://github.com/Rul1an/assay/issues/1307), every cassette
committed under this directory MUST satisfy:

**Headers stripped (REDACTED placeholder substituted):**

- `x-goog-api-key`
- `Authorization`
- `X-Goog-User-Project`
- any other authentication-bearing header observed during recording

**Query parameters stripped:**

- `key` (Gemini REST API key parameter)
- `access_token`
- any other authentication-bearing query parameter

**Body fields stripped:** none required for the canonical probe and fixture
path. If a recording inadvertently captures a body field carrying authentication,
it MUST be stripped before commit.

**Fields that MUST NOT be stripped or rewritten:**

- `FunctionCall.id` in the model response — this is the identity seam the
  candidate evaluation rests on
- the `functionCall` part `name`, `args`, and ordering
- response `status` and `content-type` headers (needed for replay correctness)

## Process

1. Run the probe in record mode (`python3 identity_probe.py --record`) on a
   maintainer workstation with a live Gemini API key.
2. The probe's VCR.py configuration applies the redaction filters above
   automatically before writing the cassette.
3. Manually review the resulting `.yaml` cassette and confirm that no auth
   credentials appear in clear text.
4. Run a secret scanner (GitGuardian, `detect-secrets`, or equivalent) on the
   cassette file.
5. Commit the cassette only after both manual review and scanner pass.

If the cassette cannot satisfy redaction without scrubbing `FunctionCall.id`,
kill criterion 3 in #1307 has fired and the implementation line must stop.

## What lives here in later PRs

The implementation PR for the full Gemini fixture (separate from this probe
PR) will add the canonical fixture cassette here. That cassette is the source
of truth for delegated acceptance replay; it is not the probe cassette.

The probe cassette and the fixture cassette serve different purposes:

- `identity-probe.yaml`: one-shot record/replay used to verify the level-3
  identity assumption. Maintainer-only.
- `<fixture-cassette>.yaml` (later PR): the canonical recording the
  delegated `gates=all` acceptance replays against.

Both are checked-in artifacts; neither contains live credentials.
