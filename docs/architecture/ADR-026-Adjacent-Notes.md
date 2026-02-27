# ADR-026 Adjacent Notes

## Purpose

These notes capture adjacent decisions and sequencing around protocol adapters without widening ADR-026's normative contract.

## Why ACP first

ACP is the preferred MVP because it creates an immediate governance use case around:
- checkout and intent transitions
- authorization evidence
- protocol payload preservation for dispute and audit analysis

ACP therefore stresses the parts of the adapter API that matter most:
- deterministic translation
- lossiness accounting
- raw payload preservation
- versioned conformance fixtures

## Why A2A second

A2A is strategically important, but it should follow ACP for two reasons:
- the protocol and ecosystem surface are still moving quickly
- A2A will benefit from a proven adapter API and conformance harness instead of co-defining them

## Open-core boundary

Protocol adapters are open core when they provide protocol-to-evidence translation infrastructure.

Out of scope for open core in this area:
- hosted adapter registries
- organization-specific approval middleware
- managed protocol policy workflows

## Attachment-writer direction

Raw payload persistence should live behind a host-provided attachment writer in the evidence/core layer.

This keeps adapters free of filesystem policy and reduces IO surface area.

## Deferred topics

These topics are intentionally deferred to follow-up ADRs or later slices:
- Wasm adapter transport/runtime
- remote adapter registry and signing distribution
- protocol-specific middleware injection into MCP or hosted gateways
- enterprise control-plane features on top of adapters

## Candidate follow-ups

Likely follow-up slices after ADR-026 Step1:
- adapter API freeze implementation skeleton (`assay-adapter-api`)
- ACP MVP adapter crate
- ACP conformance fixture harness
- A2A follow-up ADR or Step2 freeze
