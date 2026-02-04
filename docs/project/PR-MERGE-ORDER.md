# PR-merge volgorde en afhankelijkheden

**Laatst bijgewerkt:** 2026-02-04

## Gemerged

- **#127** — fix(ci): chown _work/_actions op self-hosted ✅
- **#128** — chore(deny): ignore RUSTSEC-2023-0071 ✅
- **#129** — fix(ci): self-hosted jobs tijdelijk uitgeschakeld (zie issue #130) ✅

## Open issues

- **#130** — Self-hosted runner `assay-bpf-runner` heeft corrupte _actions cache. Vereist handmatige fix op de runner.

## Open PRs (wachten op CI)

Alle PRs zijn bijgewerkt met main en hebben auto-merge aan:

| PR   | Titel                                  | Status |
|------|----------------------------------------|--------|
| #123 | fix(deps): jsonschema 0.40             | CI draait |
| #122 | fix(evidence): object_store 0.13       | CI draait |
| #116 | chore(deps): jsonwebtoken 10.3.0       | CI draait |
| #118 | chore(deps): base64 0.22.1             | CI draait |
| #117 | chore(ci): bencher bump                | CI draait |
| #115 | chore(ci): codeql-action 3.32.1        | CI draait |
| #114 | feat(observability): E5/E8 Step 3      | CI draait |

## Self-hosted runner status

De self-hosted runner `assay-bpf-runner` heeft een corrupte `_work/_actions` cache waardoor alle jobs die `actions/checkout` gebruiken falen met "tar: Cannot open: Operation not permitted".

**Tijdelijke workaround:** Self-hosted jobs (Kernel Matrix, eBPF smoke self-hosted) zijn uitgeschakeld met `if: ... && false` in de workflow files.

**Permanente fix:** Zie issue #130 voor instructies om de runner handmatig te fixen.
