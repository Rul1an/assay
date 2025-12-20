# ADR-004: Scope keuzes — RAG-first MVP

## Status
Accepted (v3 plan alignment)

## Decision
- Target users: **RAG teams**
- Agent/trace features: **v0.5.0+**
- MVP storage: **SQLite 4 tables**: `runs`, `results`, `quarantine`, `cache`
- Compare/baseline: **v0.4.0 stub** (not blocking)

## Rationale
RAG has ground truth and clear failure modes (retrieval errors, hallucination/faithfulness). Agent evaluation is research-grade for PR gating and tends to cause scope creep.
