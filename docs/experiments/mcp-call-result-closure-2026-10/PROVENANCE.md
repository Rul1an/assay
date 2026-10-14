# Provenance and limits

The code and fixtures in this directory are original Assay contributor work,
released under the repository's [MIT license](../../../LICENSE). The pinned
Assay parser modules are also MIT; dependencies retain their own licenses.
Corpus Adequacy is invoked from its separate public MIT-licensed checkout,
not copied into this package. No third-party experiment logs or code are included.

The earlier local experiment was reported in [W3C 0093](https://lists.w3.org/Archives/Public/public-agent-conformance/2026Oct/0093.html).
This package exports its 21 synthetic fixtures, seven declared mutations, two
controls, normalized checker, twelve original contract cases, tests and separate
finite-cohort recount. None of the fixture bytes or expected outcomes changed.
The source wrapper and Cargo.lock are unchanged. Export changes are limited to
local sibling imports, CA staging paths, test formatting, neutral descriptions and reproduction
instructions. The normalized contract omits preparation history and internal
references; the decision rules and literal cases are preserved.

The new `record/` execution is a public-package rerun, not a renamed copy of the
historical private measurement. `result.json` names the Assay and CA revisions,
six native module digests, Rust/dependency features, wrapper, lock, adapter,
checker and executable hashes, fixture identities and all native results.
`ca-report.json` retains the actual adapter-mutation observations and controls.
`source-receipt.json` identifies the built source capsule. These are locally
produced, unsigned records, not execution authentication. Source review and
execution remain separate; a reviewer has not rerun anything unless its record
explicitly says so.

Codex authored the fixture expectations, implementation and recount; Claude
Desktop contributed design/source advice. The recount uses a different algorithm
but has the same author. Independent non-building review is recorded on the PR
for its exact head, with reviewer identity distinct from the GitHub publisher.
No blind implementation, independent operator reproduction, model trial or
physical-effect measurement is claimed.

The 21 native cases and seven Python mapping mutations are a closed small cohort.
The 29 normalized-checker tests are a separate population; do not add those counts
or present the seven-mutation result as general adequacy. A successful terminal
result for every selected attempt does not establish that no other work is open.
