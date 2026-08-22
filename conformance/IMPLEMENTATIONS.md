# Public implementation runs

This page lists reviewed, digest-addressed `conformance_run.v1` records.
It is not a leaderboard, badge, certification, or trust score.

A digest addresses bytes. It does not authenticate a publisher.
A match is agreement on the pinned corpus only.
Corpus agreement does not prove implementation independence.
reproduction_mode, image, and identity are publisher-declared and bound; they are not attested or verified.

To add a reviewed row, write it to `public-runs.json` and the digest-named file under `public-runs/`, then run `python3 conformance/project_public_runs.py --check`.

| implementation | suite | record | image | commit | reproduction_mode | match | mismatch | execution_error | harness_error | review_warnings |
|---|---|---|---|---|---|---|---|---|---|---|
| pma-v0-repro | privileged-mcp-action-v0 | [sha256:9275ac65b1f2dde89299fcc811c733096b3b5683cb5ed15a8f32560d4580ae27](public-runs/9275ac65b1f2dde89299fcc811c733096b3b5683cb5ed15a8f32560d4580ae27) | ghcr.io/rul1an/pma-v0-repro@sha256:88a5ef285a80dc0caeb2b11093eba79f8f08c870b549cafc395aa77ee5ffc493 | c226a34f3cea50a114607c35f0976048ff3cab2b | other_disclosed | 14 | 0 | 0 | 0 | 0 |
