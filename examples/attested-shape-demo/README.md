# Attested-claim composition shape (SYNTHETIC DEMONSTRATOR)

> **This is a synthetic demonstrator.** It does not implement, verify, or assert
> any real attestation mechanism. Every envelope and digest in the fixtures is
> fabricated demo data, and "verification" is decided by a flag in the synthetic
> envelope — no cryptography is performed. Nothing here is a commitment or a
> stable contract.

## What it explores

The current claim-class schema (`assay.observability.claim_class_cell.v0`)
recognises four claim bases: `reported`, `measured`, `derived`, `inferred`. This
demonstrator explores the *shape* a fifth, `attested`, basis could take — purely
so the idea can be reasoned about on engineering merit. It is **not** in the
schema and this demonstrator does not add it.

The teaching point is the composition rule, which follows the same evidence-first
discipline the measured/derived cells already use: **absence of verifiable
evidence degrades a claim; it never silently upgrades it.**

A claim cell that cites an attestation is composed with an attestation envelope:

| Envelope state | Effective result |
|----------------|------------------|
| none supplied | degrade to `inferred`, strength capped at `weak` |
| present, not verified | degrade to `inferred`, capped at `weak` (an unverifiable envelope is not evidence) |
| verified, subject digest mismatches the cell | degrade to `inferred`, capped at `weak` (the attestation does not cover this subject) |
| verified, subject digest matches | declared strength stands (basis `attested`) |

Composition can cap a claim but never inflate it: a cell that only declared a
`weak` claim stays `weak` even with a verified, matching envelope.

## Usage

```bash
# No envelope -> the no-evidence degrade
python3 compose_attested.py fixtures/cell.json

# A synthetic verified envelope that binds to the cited subject
python3 compose_attested.py fixtures/cell.json --envelope fixtures/envelope_verified.json

# Synthetic failure shapes
python3 compose_attested.py fixtures/cell.json --envelope fixtures/envelope_wrong_subject.json
python3 compose_attested.py fixtures/cell.json --envelope fixtures/envelope_unverified.json

# JSON output
python3 compose_attested.py fixtures/cell.json \
    --envelope fixtures/envelope_verified.json --format json
```

## Fixtures (all fabricated demo data)

- `fixtures/cell.json` — a synthetic claim cell declaring an `attested` basis.
- `fixtures/envelope_verified.json` — verifies and binds to the cell's subject.
- `fixtures/envelope_wrong_subject.json` — verifies but binds to a different subject.
- `fixtures/envelope_unverified.json` — present but not verified.

## Tests

```bash
python3 -m unittest discover -s examples/attested-shape-demo -p 'test_*.py'
```

Stdlib only — no third-party dependencies.
