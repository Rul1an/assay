# Review pack: wave-g2-delegation-context-signal-step1

## Primary review questions

1. Does `G2` stay on the existing `assay.tool.decision` carrier without adding
   a new event type?
2. Are `delegated_from` and `delegation_depth` emitted only from explicit
   `_meta.delegation` context?
3. Do direct or hint-only flows remain free of delegation fields?
4. Do docs avoid claiming delegation verification, chain integrity, or temporal
   correctness?
5. Do existing decision consumers remain behaviorally unchanged when the new
   fields are absent?

## Expected outcome

- supported flows can surface explicit delegation context on decision evidence
- no chain completeness or integrity claim
- no taxonomy creep
- no pack broadening in the same PR
