<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: 2026 b7n0de -->

# Duplicate member names, five probes

These are five supplementary probes derived from PV-01 and PV-03. Each targets one member name in one object. S11 to S14 add one occurrence; S15 adds two occurrences around the existing one. They test the CAP-1 requirement to reject duplicate member names during parsing. This contribution does not change an existing runner; the commands below run the probes explicitly.

## What each probe tests

| Probe | Base | Change | `verify.py` |
|---|---|---|---|
| `S11_same_value.json` | PV-01 | `eligible` twice with the same value | conforms, exit 0 |
| `S12_escaped_spelling.json` | PV-01 | `eligible`, then `\u0065ligible` with a value that breaks R1 | refused under R1, exit 1 |
| `S13_top_level_profile.json` | PV-01 | top-level `profile` twice, `cap/0` then `cap/1` | conforms, exit 0 |
| `S14_deep_algorithm.json` | PV-01 | `subject.digest.algorithm` twice, `SHA-256` then `MD5` | conforms, exit 0 |
| `S15_threefold_complete.json` | PV-03 | `integrity.complete` three times, true, false, true | refused under R7, exit 1 |

S12 needs the decoded comparison. Its two names differ as bytes and are equal after escape decoding (RFC 8259, section 8.3), so a check over the undecoded spellings in the received bytes does not see the duplicate.

The included `verify.py` prints CONFORMS for S11 and S14. S11 leaves the selected value of `eligible` unchanged, and the rule checks in this verifier do not inspect `subject.digest.algorithm` in S14. These outputs do not establish compliance with the duplicate-name clause.

S13 places the duplicate at the top level, where a first-wins reader and a last-wins reader read different profiles.

The included `verify.py` retains the final `true` in S15 and refuses it under R7 because a unit failed. That refusal comes from the rule check, not from detecting the repeated name, so rejection alone does not demonstrate duplicate-name checking.

A reader that compares decoded names within each object and refuses the document on a duplicate refuses all five.

## Running

Run these commands from this directory. The generator and verifier use Python 3 and the standard library; the command block also needs a POSIX shell and `sha256sum`.

    python3 generate_probes.py ../../vectors out
    sha256sum out/*.json
    for f in S1*.json; do python3 ../../verify.py "$f"; echo "$f exit $?"; done

The generated files are byte-identical to the five files here.

| sha256 | file |
|---|---|
| 0fb269aabb003bd4e8a637b9823a55a032b4939589940813b0d61ea5e9eb8e69 | `S11_same_value.json` |
| 6b2ce9d78aa9ac1d2ef0634aea9d9a0394c7b87052edbdfec8d321d57223f4cd | `S12_escaped_spelling.json` |
| cfac760a875b458e0a4d016cc878780243e0ce0a6ccbe20b4fa6dde1abf62945 | `S13_top_level_profile.json` |
| c2263c7776bd234094633250766e8d2b769d1a066c1adc4e775acb360f86112d | `S14_deep_algorithm.json` |
| c412a31d4d2f29557badde80fd0f41b57b6e5be5803c9ec94ac0d71c25e040f5 | `S15_threefold_complete.json` |

## Origin

Written by b7n0de on 10 October 2026 against cap-1 at 0980d32 for the duplicate-name clause of CAP-1 -01, and sent with the measured table on 12 October 2026. With PV-01 and PV-03 from 0d8ebd75, the generator here reproduces the five probe hashes recorded on 10 October. It reads UTF-8 explicitly and writes LF line endings, avoiding the platform defaults for those operations. The line-ending change was checked by simulating Windows text output on Linux; it has not been measured on a native Windows system.

## Licence

All seven files in this directory are licensed under Apache-2.0. The generator and this README carry SPDX identifiers. The five JSON probes are modified copies of `PV-01.json` and `PV-03.json` in `../../vectors`, which `../../../LICENSE-SPEC` assigns to Apache-2.0. The table above is the modification notice for each named JSON file and identifies its base and changes. The original notice is Copyright 2026 Certisyn, Inc.; the probe modifications and generator are by b7n0de, 2026. See `../../../LICENSE` for the licence text and retain `../../../NOTICE` when redistributing the probes, together with this modification notice. The JSON bytes contain no comments so that licence annotations do not change the cases being tested.

Assisted by little helpers.
