# SPLIT REVIEW PACK - Wave43 Decision Kernel Step3

## Reviewer intent
Read Step3 as closure and scope control for the shipped Step2 split.

## What this step should do
- document the shipped `decision.rs` facade + `decision_next/*` ownership line
- forbid redesign drift after the successful mechanical split
- bound any later cleanup to internal polish only

## What this step must not do
- reopen the split design
- propose new public API surfaces
- loosen payload/reason/replay guarantees
- spill into handler, policy, CLI, or MCP server work

## Review questions
- Does the plan now reflect the actual shipped Step2 layout?
- Is Step3 clearly constrained to micro-cleanup only?
- Do the gates still protect the key decision/replay invariants?
