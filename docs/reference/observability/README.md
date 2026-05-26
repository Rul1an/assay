# Observability Reference Contracts

This directory contains small reference contracts used by Assay's
observability-layering experiments. They are review and research
contracts, not public product APIs unless a later ADR explicitly
promotes them.

| Contract | Role |
|---|---|
| [`claim-classes-v0.md`](claim-classes-v0.md) | Vocabulary for saying what a trace, archive, or joined artifact can honestly claim. |
| [`join-contract-v0.md`](join-contract-v0.md) | Vocabulary for joining trace, SDK, policy, and measured-run evidence without silently upgrading weak keys. |

These contracts are intentionally separate from Runner archive schemas.
Runner artifacts remain the primary measured-run evidence. The
observability contracts describe comparison and interpretation output
above those artifacts.
