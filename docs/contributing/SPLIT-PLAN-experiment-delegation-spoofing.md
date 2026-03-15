# SPLIT PLAN - Experiment: Delegation Capability Spoofing with Provenance Ambiguity

## Intent

Test whether schema-valid, protocol-plausible capability claims, provenance signals,
or identity metadata from a delegated actor/adapter can cause downstream consumers to
silently upgrade trust or weaken classification.

## Overarching Invariant

> Delegated capability claims, provenance, or identity must never cause silent trust
> upgrades without detectable invariant failure.

## Vectors

1. **Capability Overclaim** — adapter claims more event types than it produces
2. **Provenance Ambiguity** — events without `raw_payload_ref` or with `BestEffort` lossiness
3. **Delegation Identity Spoofing** — spoofed `source`/`ProtocolDescriptor` from untrusted adapter
4. **Preference/Selection Manipulation** — overclaimed capabilities influence adapter selection

## Conditions

- A: unprotected (claims accepted at face value)
- B: integrity-checked (overclaim detection, provenance tracking, lossiness propagation)
- C: full trust stack (B + identity verification + trust-domain separation + provenance downgrade)

## Metrics

- COR (Capability Overclaim Rate)
- PBR (Provenance Bypass Rate)
- ISSR (Identity Spoofing Success Rate)
- SMR (Selection Manipulation Rate)
- FPBR (False Positive on Benign)

## Benign Controls

- D1: legitimate adapter capability upgrade
- D2: legitimate BestEffort conversion
- D3: legitimate adapter migration (source change)

## Hypotheses

- H1: COR < 10% under Condition B
- H2: PBR < 5% under Condition C
- H3: FPBR < 2%
- H4: V3 (identity spoofing) has highest ISSR under B

## Wave structure

### Step 1 (this PR): Freeze — docs + gate only
### Step 2: Implementation — attack vectors, invariant tests
### Step 3: Closure — results analysis, hypothesis validation
