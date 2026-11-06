# Incident-package fixtures

## Retained combined session (#3345)

`combined_session_3345.tar` is a real capture of a deliberately small local test
tool, not a production incident. The package, external `.context.json` and
original exporter `.report.json` are byte-pinned in
`combined_session_3345.sha256.json`. Do not reformat or regenerate these files.

On 2026-10-17, MCP Inspector 2.5.0 (producer commit
`793d103db2538e1e8cf7e2eebee6330eeb51a02f`) invoked one `observe_three` call through
`assay mcp wrap`. The unprivileged tool read its fixed task-owned file, connected
to its own loopback socket and executed `/usr/bin/printf`. The existing
`runner-spike run --kernel-capture` observed the proxy/tool cgroup. Capture source
was `30e7871f6764c338fb6162a79c8be42f7deba368`, tree-identical to merged
`588684605d4622c0b9a562d198fafdeed3ad3190`. The aarch64 capture CLI SHA256 was
`9281a7baeb0a9f63195d01d22f2dd1aaf7df419c9072f445d5b9e10166eac9fa`.

The five objects preserve the original Inspector Protocol export, proxy decision
log, kernel NDJSON, producer health and policy bytes. There are 38 examined
retained occurrences: 30 filesystem, one network, five process, one proxy call
and one Inspector call. No aggregate hit count is expanded into additional units.
Only the explicit unique proxy/Inspector `tool_call_id` supports a strong join.
Kernel records do not establish a strong tool-call join. An exec observation is
distinct from the tool's self-reported exit code.

All five observation classifications remain `unknown`. Producer health reports
zero ring-buffer drops, but connect-only network coverage, an unavailable fork
tracepoint and absent SDK evidence remain limitations. The original policy
decision includes its unconstrained-tool warning. Nothing here proves activity
completeness, activity absence, provider outcomes, or independently authenticated
capture provenance. The actual Inspector UI capture was coordinator-observed.
See the [capture and review record](https://github.com/Rul1an/assay/issues/3345#issuecomment-6235848945).

### Minimal delivery and replay

Use the existing **Assay CLI**, built for the receiving Linux architecture,
plus the package and external context. No collector, eBPF, Node, Cargo, source
checkout or service is needed by the receiving verifier. The current build is
dynamically linked, so its runtime libraries must also be present. This recipe
uses the existing digest-pinned runtime from
`docker/assay-mcp-server/Dockerfile`; it does not claim compatibility with every
Linux distribution or parity with a separate leaf verifier.

From the repository root on Linux with Docker and a trusted Rust toolchain:

```bash
set -eu
cargo build --locked -p assay-cli --bin assay
export ASSAY_REPLAY_BINARY_SOURCE_SHA="$(git rev-parse HEAD)"
python3 -W error::ResourceWarning scripts/ci/test_verify_incident_replay.py --live \
  target/debug/assay /tmp/incident-replay-proof
```

Choose a new output directory for each run. The script pulls the pinned runtime
before isolation, mounts only the native CLI, original package and context,
then executes the CLI in a fresh read-only container with no network, no added
capabilities and UID/GID 65532. It records the actual binary hash, runtime image
ID, container configuration/state and execution output. A nonzero exit or any
difference from the full retained JSON report fails the run. The declared binary
source SHA is caller provenance, not cryptographic build attestation.

`CI / Build + Test (ubuntu-24.04)` executes this recipe on GitHub-hosted Linux
and uploads `incident-replay-<checkout-SHA>` with the receipt and fresh report.
A hosted success establishes replay on an independently hosted machine only
when its run URL and artifact have actually been inspected; this document alone
is not execution proof. Local Docker or another VM on the capture Mac does not
establish that distinction. The hosted x86_64 binary has its own measured hash;
it is not the original aarch64 capture binary or an independent implementation.

The context deliberately selects the historical `as_of` instant. Replay proves
verification under that explicit context, not current-time freshness. The
expected report is an equality oracle, never reusable verification authority.

### Fresh wrap handoff

The same recipe also runs the existing quickstart mock tool through `assay mcp
wrap`, retaining one allow and one deny in the original decision log and the
original request receipts via `--request-receipts-out`. It calls
`prepare-incident-package --request-receipts` with that sidecar, the decision log
and a context frozen before preparation. Expectations bind `wrap-decision-log`
and `wrap-request-receipts` to their original hashes. The sidecar is context only:
no extra action units, joins, authenticated session link or human approval claim.
`require_disclosure` only requires action units from that input to be examined.
The sidecar has none, so this flag adds no examination guarantee; its hash
expectation still applies. No human reading or approval is established.
This fixture produces request receipts, not server ask/answer observations.
Each receiver gets only the native CLI, package and context in a new offline
container. The producer checkout, mock tool, policy and decision-log path are not
mounted into the receiver. The pinned runtime supplies the dynamic libraries.

The positive receiver must reproduce the preparation report, with two examined
units, matched input expectations and unknown observation classifications. Eight
separate receiver copies must refuse: changed object bytes (`input_shape`, the
container's object-name hash check), a missing object (`digest`), a wrong input
expectation (`expectation_mismatch`) and an outer-byte budget one byte below the
package size (`resource_selected`), and changed assessment bytes with their
original inventory binding (`digest`). Three further copies target the sidecar:
changed object bytes (`input_shape`), a missing object (`digest`), and a wrong
sidecar expectation hash (`expectation_mismatch`). Object corruption tests
the container's object-name hash check, and removal tests the inventory's object
binding; the wrong expectation tests the separately supplied sidecar binding. Every refusal must exit 2; an arbitrary error
is not a passing negative test. The reason assertions also pin the existing
validation order, including context failures before stale-assessment checks.
Original evidence bytes remain unchanged.

`fresh/` retains the producer commands, requests, stdout/stderr, original decision
log, request sidecar and package, handoff hashes, and each receiver's report and container receipt.
The test checks the handoff, named refusals and container removal; cleanup failure
fails the recipe. Fixture processes have a 30-second deadline and owned-process-
group cleanup. This is a trusted synthetic fixture, not a hostile-code sandbox.

The receiving context is an agreed input, not a claim that a recipient can change
limits without changing the bound assessment. These are policy decisions, not
proof of tool outcomes or complete capture. The producer and receiver use the
same verifier implementation. Local VM execution is not independently hosted
execution; the existing CI job must actually run successfully to establish that.
The fixture uses MCP protocol version `2024-11-05`; this is not a protocol-version
matrix. Retained fresh packages can be replayed with their exact frozen `as_of`
context, but that does not establish current-time freshness.

### Mixed ask/answer handoff

`mixed-accept/` and `mixed-decline/` extend the same recipe with the legacy
`2025-11-25` elicitation fixture shared with `mcp_wrap_server_asks.rs`.
The synthetic client waits for the server ask before sending its answer; there
are no sleeps, hosted models or credentials. The outer producer deadline and
process-group cleanup cover the client, wrap and its fixture server.

Each session retains one allow receipt, one original ask and one original
answer. Their existing `call_link`/`call_id` fields are tested as observed producer
metadata, not authenticated session membership. The whole mixed sidecar is
passed through the same CLI prepare and offline receiver route. It adds no
units: the valid receiver still reports one examined policy unit, matched input
expectations and unknown surface observations. The answer's accept/decline is
context, not another policy decision or proof of human approval.

Each session also has an `observation-corrupt/` receiver copy. Only a byte in
the retained answer's `action` is changed, keeping the original object name;
the receiver must refuse with `input_shape`. The test independently locates the
answer row and checks that the corruption hit it, rather than a request receipt.
Original source and package bytes remain unchanged. The existing receipt-only
positive and eight refusals stay in place; the two mixed sessions add four
receivers, not another protocol-version matrix or verifier implementation.

## Synthetic five-surface vector

`valid_five_surfaces.tar` is a synthetic, hand-authored positive vector. The
source records and expected surfaces are written explicitly in
`incident_package_verify.rs::five_surface_package`; that builder uses
`BundleWriter` only for canonical inner-bundle framing and computes byte
digests and unit IDs mechanically. It does not ask the incident verifier or
exporter to supply its expected assessment.

The selected records are one proxy decision (`tool_call`), one canonical file
access (`filesystem`), one canonical network connection (`network`), one
canonical process execution (`process`), and one Inspector tools/call request
(`transcript_join`). The expected accounting is three input occurrences, three
objects, five units, five examined results, and one CAP-1 eligible/examined row
for each of the five surfaces. The canonical events' `hits` values 2, 3 and 1
are payload values, not multipliers for retained-unit counts. All observation
grades are `unknown`; no claim or join is asserted.

The evidence integration test requires exact archive bytes and these literal
counts. The CLI integration test exports the same fixture's source bytes,
compares the entire output archive byte for byte, and reverifies it. Rebound
negative variants change the network unit's surface or drop that unit while
balancing the recorded CAP-1 counts; each must still be refused by independent
source-record enumeration. This fixture proves selected-record accounting only,
not live capture or session completeness.

## Prepared attestation package

`attestation_verified.tar` is the output of `prepare_verified_incident_package`
for one selection: the bundle object of `valid_five_surfaces.tar`, the public key
of the deterministic test key `SigningKey::from_bytes(&[7; 32])`, a DSSE envelope
over that bundle signed with that key, and the matching attestation verify report.
`attestation_verified.context.json` is the receiver context it is bound to: the
receiver default with `keys` selecting that public key's SHA-256. Under the
default context, which selects no key, the package refuses `trust_input`.

`incident_fail_closed_properties.rs::attestation_fixture_is_the_prepare_output`
rebuilds the package from those inputs and requires these exact bytes, so a change
in prepare's output fails there first. The `incident_package` fuzz target uses this
package as a seed and this context as one of its fixed contexts, so fuzzed inputs
can reach signature and report verification. The DSSE is synthetic: it says nothing
about who would sign a real bundle or why.
