# SafeSkill Scan Notes (2026-03-30)

## Context

SafeSkill flagged a broad set of content findings against the Assay repository on 2026-03-29. The highest-volume findings were not code-execution paths. They were mostly prompt/content heuristics triggered by:

- outward-facing docs and quickstarts using sample paths like `/etc/passwd` or `/etc/shadow`
- config-directory docs spelling `~/.config/assay/packs`
- security/threat-model prose using verbs like `exfiltrate`
- test fixtures, goldens, and security-audit inputs that intentionally model sensitive-path abuse

## What we changed

The outward-facing hygiene follow-up landed in [#1010](https://github.com/Rul1an/assay/pull/1010).

That sweep intentionally changed only active docs/examples:

- README / quickstarts now use neutral out-of-scope demo paths instead of `/etc/passwd`
- active docs prefer `$XDG_CONFIG_HOME` / `$HOME/.config` wording over raw `~/.config`
- security docs now use clearer phrases like `send sensitive data out` instead of `exfiltrate`

## What we did not change

We intentionally left tests and audit fixtures alone.

Remaining scanner-trigger strings in `crates/**` and `tests/**` are currently by design in three buckets:

1. Golden fixtures and e2e tests that verify path-deny behavior.
2. Security-audit fixtures that simulate obviously bad file access or destructive tool arguments.
3. Code/tests that document canonical config-dir fallback logic or path-generalization behavior.

Those examples are part of product coverage, not outward marketing copy.

## Maintainer interpretation

Treat the residual findings as **content-scanner false positives against intentional test coverage**, not as evidence that Assay itself contains a live data-exfiltration path.

The current maintainer posture is:

- keep outward-facing docs/examples reasonably scanner-friendly
- do not rewrite or weaken security tests just to improve a content score
- prefer documenting these false-positive buckets over deleting useful abuse-case fixtures

## Follow-up

- SafeSkill-facing feedback was sent separately to ask about best practices for scanning GitHub forks, docs/examples, and test-fixture-heavy repos: [OyadotAI/safeskill#1](https://github.com/OyadotAI/safeskill/issues/1).
- If the scanner later supports file-class weighting or fixture exclusions, reevaluate whether a repo-level scan profile is worth adding.
