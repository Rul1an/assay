# Resolving a live third-party evidenceRef

> Experimental. The recomputation and resolution layer only, not a trust, issuer, or effect verdict.
> Nothing here is a judgement about the producer.

The June experiment in `../evidenceref-recompute-consumer-2026-06` states as its fourth non-claim that
it operates on committed bytes only and queries no producer. This is the one step past that: a
reference published by a producer we do not control, resolved with that consumer unmodified.

The subject is one verdict record from the MCP Verification Gate (0.4.1, commit `a2c5fdf7d26c`),
served content-addressed at `/record/<record_sha256>` and published for recomputation by its operator
in [modelcontextprotocol#1913](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/1913#issuecomment-5593534179).

## Run

```bash
python3 capture.py            # verify the pinned capture offline (self-verifying, no trust in the capturer)
python3 capture.py --fetch    # re-fetch the same address and compare byte for byte
python3 resolve.py            # 11 cases, writes runs/resolve-run.json, exit 0 iff every expectation holds
python3 independent_resolve.py  # re-derive every verdict with code that imports neither runner nor consumer
python3 mutate.py             # 7 rules, writes runs/mutation.json, exit 0 iff each is killed
pytest                        # 12 tests
```

Python 3 standard library only. `pytest` and `rfc8785` are used by the test suite; the RFC 8785
cross-check skips if the library is absent.

## What the record does

SHA-256 of the served body equals the last path segment of the URL that served it. Fetched twice,
hours apart, byte for byte identical. So the address resolves, and it resolves for a party holding
those exact bytes.

One practical note for a record published so an independent party can recompute it: the endpoint
answers `403` to the default `Python-urllib` agent while serving the same address to `curl`. A bare
stdlib fetch is the likeliest first attempt by exactly the verifier the design wants, so `capture.py`
sends an explicit agent string.

## The finding

The finding is about the **reference**, not the body. Three forms over one pinned body:

| case | reference as | verdict |
|---|---|---|
| A | digest and locator, the shape served today | `malformed_ref` |
| B | read charitably as `jcs-json-v1` | `digest_mismatch` |
| C | naming `octets-as-served-v1` plus a schema identity | `recomputed` |

One object, three content addresses:

```
octets as served       sha256:4dffe218167d40bb5940ba6cd404225b95b3d20ca5cdac880fbd4c62f0e343fc  <- published
RFC 8785 JCS           sha256:3c24a9a486bebe9888b6526e5c82727fba7226e3f72af80fc7a956ff0141b99f
deterministic CBOR     sha256:862392e49afa99803d73dc003fff03a7c9036ebea0dee9aea40c0a89fd50c38d
```

The served bytes and the JCS bytes are **both 10190 bytes**. Same object, same byte count, different
address: the difference is key order alone. A consumer that guesses the profile gets a mismatch rather
than an error, which is the whole reason a scheme name has to travel beside a digest.

That the B result is a real RFC 8785 result and not an artifact of naive key sorting is checked twice:
the record's value space is asserted ASCII and float-free, where naive sorting and true JCS coincide,
and the bytes are compared against an independent `rfc8785` implementation.

`octets-as-served-v1` is carried here as a **proposed** profile and is deliberately not added to the
published consumer's registry. Naming a profile the consumer cannot resolve is what the reference
would have to do today, and a consumer must refuse a name it cannot resolve rather than assume one.

A second, structural point falls out of case C: an octets profile hashes the received bytes, so a
consumer that parses into an object before hashing cannot support it at all. A byte-pinned reference
and an object-pinned reference need different consumer plumbing, which is why this is not simply a
matter of adding a string.

## Layout

- `record/gate-record-4dffe218.json` the captured octets, byte for byte; `record/capture.json` the
  provenance manifest. The capture is self-verifying: its SHA-256 is the address it was fetched from.
- `resolve.py` the reference runner. Cases A and B go through the published June consumer unmodified,
  pinned by `test_published_consumer_is_used_unmodified`. Cases C onward use the octets consumer here.
- `independent_resolve.py` re-derives every verdict from the pinned octets with code that imports
  neither `resolve.py` nor the published consumer, asserted by AST. It shares the standard library and,
  for the object profile, the same one-line `json.dumps` JCS expression; the genuinely independent JCS
  check is the `rfc8785` package in the test suite.
- `mutate.py` silences each octets rule in turn. Three of the seven kills are crash-kills (the guard's
  removal makes a later line raise), recorded as such in `runs/mutation.json`; a crash shows the guard is
  reachable, a verdict change shows it discriminates, and the report keeps the two apart. Its case roster is derived from `build_cases()` and
  never hand-listed.
- `runs/resolve-run.json`, `runs/mutation.json` machine-readable records; `SHA256SUMS.txt`.

## Two disciplines worth naming, because both bit

**Integrity before interpretation.** The content address is checked before the bytes are parsed, so a
body that is both tampered and incomplete is refused at the address and never reaches the completeness
rules. Interpreting first would let a body choose which rule judged it. Pinned by
`test_refusal_order_integrity_before_interpretation`, with the untouched body as the control on the
same line.

**A rule no case reaches is decoration.** The first mutation run killed three of seven rules; four
survived because no case exercised them. Cases H to K exist for that reason. The roster is derived
rather than listed, because a hand-kept roster restating machine state is how a rule stays unexercised
while the report reads green.

## What this establishes, and what it does not

Establishes: the published address recomputes from the received octets; the reference as served names
no canonicalization and no schema, so the published consumer refuses it fail closed rather than
guessing; naming a profile and a schema identity on the reference reaches a clean verdict over the
same unmodified bytes; and a byte-pinned reference does not survive a reserialization of the same
object.

Does not establish: anything about the honesty, competence or correctness of the producer; anything
about whether the measured server behaved as the record says; anything about records other than this
one capture; and no claim that `octets-as-served-v1` is the right profile to standardize. It is one
profile that makes this reference resolvable, and the shape of the field matters more than the name.

## Row, in the format of the SCITT joint interoperability report (section 1.2)

Offered so that a reader who keeps such a register can copy it rather than reconstruct it.

- **Contributor:** Rul1an.
- **Artefact and the exact version or commit run:** one MCP Verification Gate verdict record, gate
  0.4.1 commit `a2c5fdf7d26c`, at content address
  `sha256:4dffe218167d40bb5940ba6cd404225b95b3d20ca5cdac880fbd4c62f0e343fc`, 10190 bytes; resolved by
  the consumer published at `Rul1an/assay@a052bf7f` in
  `docs/experiments/evidenceref-recompute-consumer-2026-06`, used unmodified.
- **Kind:** `independent-implementation`. The consumer was written from the reference shape discussed
  in SEP-1913 and published three months before this record existed; the record and its address are
  the producer's. Neither party authored the other's side. It is not
  `independent-implementation-independent-vectors`, because there is one record here and no vector set.
- **What the run showed:** the published address recomputes from the received octets; 11 of 11 cases
  land where expected under two independent runners; 7 of 7 octets rules killed under mutation with
  the clean control preserved; the reference as served reaches `malformed_ref`, and reaches
  `recomputed` once a profile and schema identity are named on it.
- **What it does not establish:** nothing about the producer's honesty or about the measured server;
  nothing about any record other than this capture; no cross-implementation result over a vector set;
  no claim that the proposed profile is correct to standardize.
- **Prior art:** Henri Sirkkavaara (vaaraio), who ran the first reproduction attempt against this
  producer and reported the negative that led to the fix this record depends on
  ([issuecomment-5588082099](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/1913#issuecomment-5588082099));
  the producer, who conceded the defect and shipped the content-addressed record within hours. RFC 8785
  (JCS) and RFC 8949 section 4.2 for the two object profiles.
- **Non-comparable pairs:** none stated.
- **Platform and runtime:** macOS arm64, Python 3.14.3, 2026-09-09.
