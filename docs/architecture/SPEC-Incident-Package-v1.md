# Incident Package v1 — bounded offline contract

Status: **contract proposal; implementation pending**. This document proposes the documentation
slice [#2853](https://github.com/Rul1an/assay/issues/2853) of
[#2493](https://github.com/Rul1an/assay/issues/2493). It does not describe an available package command.
Source baseline: `bf2659824e51c63edb433d2386b003f16a2da419`; its source differs from
`63cf7f34dc1c53b31da322363ec8c2b5701a50ae` only in CHANGELOG.

[ADR-042](ADR-042-evidence-first-positioning.md) and
[ADR-043](ADR-043-evidence-chain-integrity-invariants.md) control this contract.
The parent’s [accounting amendment](https://github.com/Rul1an/assay/issues/2493#issuecomment-5576189453)
and [contradiction correction](https://github.com/Rul1an/assay/issues/2493#issuecomment-5572070075)
control earlier unconditional statements. MUST, MUST NOT and MAY are normative here.

## 1. Scope and identities

The input is an operator-selected set of local regular files, not a query of a live host or store.
The package preserves those original bytes, declares their examination scope and carries enough
material to repeat bounded verification. Verification is passive: no replay, network retrieval,
package-supplied executable or model call. This is not a complete activity capture contract.

Five new document identities are reserved by this specification, **not yet production identities**:

| Identity | Role | Residence |
| --- | --- | --- |
| `assay.incident.inventory.v1` | Closed inventory/input declaration | `inventory.json` inside outer archive |
| `assay.incident.assessment.v1` | Recorded examination/accounting, coverage and correlation | `assessment.json` inside outer archive |
| `assay.incident.verify.v1` | Fresh verifier result bound to completed outer bytes | Sibling document outside the archive |
| `assay.incident.results.v1` | Explicit disclosed examination results | Original input object; never a top-level archive member |
| `assay.incident.context.v1` | Explicitly declared custody/retention and related context | Original input object; not independent authentication |

They follow [CLI JSON identities](CLI-JSON-IDENTITIES.md): discriminator `schema`, breaking generation
`.vN`, explicit typed fields. Unknown identity or unknown fields in these closed documents refuse.
No producer registry row is added until the corresponding producer exists. Foreign CAP-1, DSSE,
in-toto and inner bundle identities are preserved; they are not renamed as package identities.

The implementation MUST preserve public Rust structs and `VerifyResult`. It adds APIs rather than
changing existing public field shapes. The standalone reader remains a leaf, not an assay-evidence
consumer. A valid package may report incomplete observation; there is no aggregate safety verdict.

## 2. Container and determinism

The outer file is **uncompressed POSIX ustar**, regular files only. No PAX/GNU extensions, sparse
files, links, devices, directories or concatenated archives. The only paths are `inventory.json`,
`assessment.json` and `objects/<64 lowercase hex SHA-256>`. Paths are ASCII, case-sensitive, relative,
and unique. Headers use uid/gid/mtime zero, mode 0644, empty uname/gname/prefix, ustar magic/version,
fixed-width octal fields with zero fill and NUL terminator (mode/uid/gid 8 bytes,
size/mtime 12 bytes); checksum is six octal digits, NUL, space, computed with its eight bytes
replaced by spaces. typeflag is ASCII `0`; magic is `ustar`+NUL, version ASCII `00`.
All unused header bytes are zero, including device fields; data padding is zero. Exactly two trailing zero blocks
terminate the archive, with no suffix. Order is inventory, assessment, then object paths ascending.
These restrictions avoid alternate metadata encodings and path interpretation on extraction.

Each unique original byte string is stored once by SHA-256. **Input occurrences are not deduplicated**:
the inventory retains separate occurrence IDs even when their object digest is equal. Record identity
includes occurrence ID, original digest and format locator. Changing input-list order does not change
output when the caller supplies the same occurrence IDs. IDs are caller-declared labels, not provenance.
SHA-256 equality is byte identity under the usual collision-resistance assumption, not source identity.

Canonical package JSON is RFC 8785 JCS, UTF-8 with no BOM and one final LF. Strict parsing rejects
duplicate keys, non-finite numbers, invalid UTF-8 and lone surrogates before canonical comparison.
Package-controlled counts/offsets are integers in `0..9007199254740991`; bool is not integer.
No floating timestamps or coercion. Original member JSON need not be JCS; its bytes are never rewritten.
Any integer-valued JSON token outside the exact domain refuses this package profile before conversion,
including arbitrary payload values; noninteger numbers must be finite JCS-representable IEEE754.
This is an additional outer admission limit, not a weakening or reinterpretation of an inner parser.

Inventory references assessment by its exact-byte digest. Assessment references source objects and
occurrences, **not inventory or outer archive digest**. The fresh verification report lives outside
the archive and contains its full raw SHA-256. There is no self-referential archive hash. A retained
attestation verification report may be an object, but must describe its original inner artifact;
it cannot prove this new outer package. Fresh recomputation never trusts a retained OK string.

Export writes a same-directory exclusive temporary regular file, finishes all validations, flushes
and synchronizes it, then atomically publishes it with no-replace semantics to a destination that does not already exist. A precheck followed by replacing rename is insufficient; lack of a race-safe no-replace primitive refuses.
An existing destination refuses; no overwrite option in v1. On failure, existing files are untouched;
remove only the temporary file created by this invocation. Package creation never authorizes deleting
sources. Read each source once into bounded staging while hashing, compare staged size/digest, and
build from that exact staged copy; subsequent source changes do not change the selected staged bytes.

## 3. Closed admission and locators

A declared format is checked against original bytes; filenames alone never select it. Unknown format,
version or type refuses **package admission**, not an invented `unsupported_input` unit. A known
admitted unit whose specified examination fails receives its applicable disposition. Framing failure
that prevents enumerating identities refuses the package; it cannot create a fabricated denominator.

| Format token | Exact admitted form and examination | Locator |
| --- | --- | --- |
| `assay-bundle-v1` | Canonical manifest/events archive accepted by the baseline `verify_bundle_with_limits`; only the event types in §4. Full canonical verification precedes event identity use. | `event:<run_id>:<assayseq>` within a digest-bound input occurrence; run_id taken from verification, not filename |
| `assay-health-json` | One typed `assay.runner.observation_health.v0`, `assay.enforcement_health.v0` or `.v1` JSON document, parsed against its baseline typed shape; ObservationHealth also runs its existing validate rules. Enforcement-health carriers have no claimed common baseline semantic validator: v1 checks the stated consistency rules below; context only | `json:` |
| `assay-coverage-json` | One `assay.runner.coverage_descriptor.v0` document, parsed as baseline CoverageDescriptor with exact schema; semantic claim rules are applied separately; context only | `json:` |
| `incident-context-v1` | Closed `assay.incident.context.v1` declaration defined in §9; context only | `json:/entries/<index>` |
| `incident-results-v1` | Closed `assay.incident.results.v1` document defined in §7; context containing disclosed results, never extra action units | `json:/results/<index>` |
| `ed25519-public-key-pem` | One SubjectPublicKeyInfo PEM Ed25519 public key, as accepted by the baseline attestation CLI key parser; context, never a trust-root selection | `bytes:0:<length>` |
| `proxy-decision-ndjson` | Original DecisionEvent JSON records from the pinned `decision_next/emitters.rs` producer; exact `specversion=1.0`, `type=assay.tool.decision`, baseline DecisionEvent typed shape | `line:<ordinal>:<byte-start>:<byte-length>` |
| `policy-bytes` | Original UTF-8 YAML policy bytes, bounded and digest-checked; opaque context. Parsing a policy or establishing activation is NOT this examination. | `bytes:0:<length>` |
| `dsse-attestation` | Baseline typed DSSE/in-toto evidence-bundle attestation verified with an explicitly selected external Ed25519 key and original bundle; retains checked optional extent. | `json:` |
| `attestation-report` | `assay.evidence.attestation.verify.v1`; compare known fields with fresh canonical attestation verification against the original bundle/key. Missing required verification material cannot produce a match. | `json:` |
| `inspector-protocol-793d103` | MCP Inspector Protocol JSON export, source pin below; array of MessageEntry objects. Only explicit client `tools/call` requests are correlation units; other admitted messages are context. | `json:/<zero-based-array-index>` |

There is no admission of a Runner archive as if it were a canonical bundle, raw kernel structs without
a versioned format, arbitrary JSONL, or native Codex/Claude history in v1. This is a selected client
transcript format, not a claim that every vendor session format is supported. Raw sidecars are JSON
objects with the explicit identities above. Missing formats require a reviewed version extension.

The Inspector pin is commit `793d103db2538e1e8cf7e2eebee6330eeb51a02f`, package manifest version 2.5.0:
[export producer](https://github.com/modelcontextprotocol/inspector/blob/793d103db2538e1e8cf7e2eebee6330eeb51a02f/clients/web/src/hooks/useExportActions.ts),
[MessageEntry](https://github.com/modelcontextprotocol/inspector/blob/793d103db2538e1e8cf7e2eebee6330eeb51a02f/core/mcp/types.ts).
The export serializes its current array and supports filtered selections; it emits no download for
empty arrays. Thus a supplied `[]` is a valid empty declared input, not proof this UI exported it or
that the session had no calls. Version identity is a source pin, not a captured-runtime claim.

For this admitted subset, each MessageEntry requires nonblank `id`, RFC3339 UTC `timestamp`,
`direction` in request/response/notification and a JSON-RPC 2.0 `message`; optional keys are `origin`
(client/server), `response`, nonnegative finite `duration`, and string `clientError`. Unknown entry
keys refuse this version. `origin` absent is unknown origin; it cannot create a client-call unit.
A request unit requires direction=request, origin=client, message.method=`tools/call`, non-null
string/integer message.id, params.name nonblank and params.arguments absent or object. Its result is
not assumed successful; optional response must have matching id and exactly one result/error.
Other well-formed JSON-RPC messages are context. Duplicate MessageEntry IDs do not remove occurrences;
they make ID-based joins ambiguous. Argument text remains uninterpreted data.

Canonical bundle NDJSON framing is exactly its existing verified reader, including its BOM/blank-line,
LF/CRLF, bounded-line, sequence and ID checks. It is not replaced with the loose NDJSON iterator that
skips blank lines. A failed canonical archive earns no verified event locator. JSON arrays use their
original index including context rows; filtering does not renumber them. For proxy-decision-ndjson the ordinal is zero-based physical line number and byte-start is the
zero-based offset in the original object. Byte-length excludes LF and a preceding CR. Reject BOM,
blank/whitespace-only lines, lone CR, malformed JSON or trailing non-whitespace. Accept LF or CRLF,
including a final valid unterminated line. Empty file is present-empty, not a missing source. Count
every line once, including repeated IDs; duplicate IDs make ID correlation ambiguous, not a dropped
occurrence. Never parse a malformed line as an unsupported unit: framing/typed admission refuses
the package before accounting. No physical-line locator is admitted for Inspector arrays. Final newline on ordinary JSON is optional; trailing non-whitespace
refuses. Package JCS documents have the stricter exact LF rule in §2.


Foreign Assay shapes are pinned to the source baseline, not a future moving Rust type. Their
existing optional/default/unknown-field parsing behavior is preserved; package objects remain closed.
All JSON receives the shared strict syntax/depth/numeric preflight before the typed interpretation.
The concrete shape references are [ObservationHealth](https://github.com/Rul1an/assay/blob/bf2659824e51c63edb433d2386b003f16a2da419/crates/assay-runner-schema/src/health.rs),
[CoverageDescriptor](https://github.com/Rul1an/assay/blob/bf2659824e51c63edb433d2386b003f16a2da419/crates/assay-runner-schema/src/coverage.rs),
[DecisionEvent/DecisionData](https://github.com/Rul1an/assay/blob/bf2659824e51c63edb433d2386b003f16a2da419/crates/assay-core/src/mcp/decision_next/event_types.rs),
[v0 enforcement health](https://github.com/Rul1an/assay/blob/bf2659824e51c63edb433d2386b003f16a2da419/crates/assay-cli/src/cli/commands/monitor_next/enforcement_health.rs)
and [v1 enforcement health](https://github.com/Rul1an/assay/blob/bf2659824e51c63edb433d2386b003f16a2da419/crates/assay-cli/src/enforcement_health_v1.rs).
These references define wire types, not a claim that all have existing semantic validators.

V0 enforcement scope must be `ipv4_tcp_connect`. Active requires attach_confirmed=true and
class=strong. Absent/failed require attach_confirmed=false, class=basic, both counts=0.
Reserved not_applicable is admitted as context only, requires no attach, basic and zero counts.
Counts are exact nonnegative integers. These are new package admission checks against stored bytes;
constructor output alone is not validation.

V1 scope=`tcp_connect_landlock_port`, mechanism=landlock and policy_semantics=allowlist.
Active requires class=strong, failure absent/null, landlock.no_new_privs_confirmed=true and
restrict_self_confirmed=true, handled_access_net=["connect_tcp"], allowed_connect_tcp_ports present
as an array of u16, net_connect_tcp_supported absent/null. Failed requires class=basic,
failure with typed reason_code/detail, restrict_self_confirmed=false, net_connect_tcp_supported
boolean, handled_access_net/allowed_connect_tcp_ports/restriction_shedding absent/null, probe=null.
ABI is u32; no inference from ABI proves a probe occurred. The probe key is always required.
A non-null probe requires status=active and its exact baseline typed fields; blocked_errno=EACCES
and listener_reached=false are necessary to support a recorded real-block claim. Otherwise the
probe is retained but supplies no such support. restriction_shedding, when present, is one of
restrictions_held/restrictions_shed/inconclusive; inconclusive is not restrictions_held. No package
may infer current host enforcement from these historical assertions. ObservationHealth additionally
uses its existing validate rules. Unknown capture_origin remains Unsupported under that type's
explicit serde rule; it does not silently become Own.

## 4. Closed populations and context

Each admitted canonical event MUST match exactly one row below. Its original payload is retained;
canonical envelope validity alone does not assert that a producer observed all activity.

| Canonical event type | Surface / examination result |
| --- | --- |
| `assay.tool.decision` | tool_call; parsed policy decision, never execution/effect proof |
| `assay.fs.access`, `assay.sandbox.fs` | filesystem; parsed retained file-entry observation |
| `assay.net.connect` | network; parsed retained network-entry observation |
| `assay.process.exec`, `assay.sandbox.exec` | process; parsed retained process-entry observation |
| `assay.profile.started`, `assay.profile.finished`, `assay.sandbox.summary`, `assay.sandbox.degraded`, `assay.coding_agent.evidence_pack.v0` | context only; metadata, aggregate summary, degradation or coverage carrier |

Raw proxy-decision-ndjson records map only to tool_call. Inspector client tools/call requests map only to transcript_join. Inspector responses, notifications
and other requests, health, policies, attestations and reports are context. Each occurrence has exactly one population; copies declared as separate inputs are separate supplied occurrences, not proof of separate actions. Unknown canonical event type refuses this package version even if the inner bundle
verifier preserves it. That restriction does not alter the inner bundle format.

The examination operation for an action unit is: validate its baseline typed payload where one
exists; otherwise require object data and extract the source producer’s named entry fields from the
pinned exporter mapping. The extraction definition is part of the frozen baseline, not a guessed
field-name heuristic: profile strict nonnegative `hits` with event subject as retained identity (Observed mode omits
`file`/`host`/`cmd`; Full may include them), sandbox string `op,path,backend` / string `argv0`
and strict nonnegative `hits`, and tool DecisionEvent fields. Missing or wrongly typed required extraction fields gives `failed`; it is not
an unknown-type admission. Optional aggregate hits do not multiply units. Updating a producer mapping
requires rechecking this pinned contract before admitting its changed payload version.

All five surfaces are always listed in assessment. A source occurrence explicitly declares its
candidate surface set according to its format; canonical bundle detail capability is declared using
its retained summary/detail context. Missing source means status `unknown` and no corresponding CAP-1
stratum. Verified present-empty source may create zero supplied units, but never proves zero activity.
An empty network list with Absent observation stays unknown. All sources absent gives `cap1: null`,
unknown surfaces and no activity-absence claims, not a fabricated zero-strata CAP-1 document.

## 5. Six vocabulary mappings

The following mappings retain original producer fields. The package keeps distinct axes for source
class, observed scope/window/health, retained-unit accounting and claim support. No translation itself
upgrades trust. Unlisted enum values follow the pinned type
contract: explicit Unknown/Unsupported fallback remains unknown; otherwise admission refuses.

| Source vocabulary | Package interpretation |
| --- | --- |
| `coding_agent.rs`: Observed | Positive observation at stated source/scope; not complete by itself |
| Same: SelfReported | Producer assertion; no independent observation |
| Same: Unavailable, Absent, Partial | Unknown, absent or limited observation as reported; none proves activity absence |
| Same source classes: BoundaryObserved, IndependentlyObserved, ThirdPartyObserved, ProducerReported, IssuerAttested, ReceiverReceipt | Retain exact class; independently observed is not created by export, a signature or correlation |
| `coverage.rs`: Full | Full only for descriptor boundary/window and supported protocols |
| Same: OpenSyscallOnly, ConnectOnly, DatagramPeerObserved, ConnectAndDatagramPeerObserved, ExecOnly | Preserve those exact restrictions; do not infer unobserved non-open/datagram/non-exec units |
| `health.rs`: Complete, PartialRingbufDrops, Absent | Preserve health state with origin/window; drops limit claims but do not identify missing record IDs |
| leaf `schema.rs`: Complete, Partial, Absent | Retain coverage and its proposition scope |
| Same: ProducerReported, IssuerAttested, ReceiverReceipt, BoundaryObserved, ThirdPartyObserved, Unknown | Preserve class/unknown ceiling independently from coverage |
| `inventory_carrier.rs`: Complete, Partial, NotScanned, Unavailable, Unsupported | Scanner-scope evidence only; non-Complete cannot support scanner absence and Complete is not session completeness |
| `attestation/extent.rs`: producer_reported, not_stated; optional source_type/counts | Preserve checked extent’s stated basis; no inferred source/counts for not_stated; omitted sandbox-network remains omitted, never zero |

A successfully interpreted enforcement-Failed health document is not a failed ingestion unit. Its
applicability constrains activity-absence claims even beside a successful transcript. Proxy decision
logs are not call censuses: forwarding may occur with `decision_out=None`.

## 6. CAP-1 and disclosure

Pin the normative [CAP-1 schema](https://github.com/Certisyn-Inc/certisyn-drafts/blob/0980d3201aa2caab3cbad5c6e9bc99b422370b43/cap-1/src/CAP-1.schema.json)
at commit `0980d3201aa2caab3cbad5c6e9bc99b422370b43`, SHA-256
`4453f216089543780bfecc4295cc4a61462fdc585b88d1e35b7d1aba79716b4a`;
[rule text](https://github.com/Certisyn-Inc/certisyn-drafts/blob/0980d3201aa2caab3cbad5c6e9bc99b422370b43/cap-1/draft-hillier-coverage-attestation-00.txt)
SHA-256 `7a9eeb1fbdb1fee95697622546d2ae7efba762fff193d6ee34765233539ac353`.
This pins an Internet-Draft, not an adopted standard. No remote schema fetch occurs during verification.

Order is strict syntax/resource admission, normative JSON Schema 2020-12 validation, R0–R8, then
package claim gating. R0 requires unique stratum IDs; R1 accounts every eligible unit; R2 closes
dispositions; R3 binds withholding by digest; R4 justifies denominator basis; R5 bounds counts;
R6 requires an existing referenced stratum; R7 caps incomplete results; R8 requires supported claim
classes on a cited stratum. Existing relying-party fixtures do not replace schema validation.

CAP-1 is generated, not accepted as arbitrary producer accounting: profile=cap/1,
subject={kind:"collection",ref:"incident-selected-inputs"}, no subject digest (avoids inventory cycle).
One stratum per present surface uses its exact Surface name as id, population="retained supplied records",
supports=["positive_presence","bounded_negative","exhaustive_set"], and counts retained supplied occurrences, using basis.kind=enumeration
and enumeration_method=`incident-v1-digest-locator-enumeration`. Unit IDs bind occurrence/digest/locator.
Each stratum's eligible is the exact number of its Unit rows; examined counts examined rows;
unexamined is the sorted list {unit,disposition} with withheld_digest additionally required only
for withheld. Do not emit optional detail/catalogue fields. integrity has exactly complete,
statement="accounting of selected supplied records", and capped_to; complete=false and
capped_to="incomplete" if any unit is failed/resource_exhausted/unavailable, otherwise true/null.
No absence_assertions are emitted unless a supported bounded_negative claim exists; each then has
assertion equal to the claim ID and stratum equal to its Surface. producer/as_of/uncapped_verdict/
unaccounted are omitted in this profile. Every eligible unit has one assessment row. Accepted disclosed results are examined. Accepted withheld
results have withheld disposition and exact withheld-material SHA-256, never also examined. Results
are disclosed inline or in a same-package object with digest+JSON locator; unresolved references refuse.
Source class such as producer_reported/client assertion is not a withholding disposition.

Closed unexamined dispositions are not_applicable, disabled_by_policy, unsupported_input,
resource_exhausted, failed, unavailable, out_of_scope, withheld. Unknown package formats are refused
before this stage; unsupported_input can describe only a known admitted unit’s unsupported examination.
Resource exhaustion that prevents complete enumeration refuses the package; no estimated remainder.

CAP-1 integrity.complete describes dispatched examination accounting, not capture completeness.
Package activity coverage remains independently unknown/limited when applicable sources or health
are missing. Every activity-absence assertion names an existing supplied-record stratum and an
independently supported observation boundary. Unknown observation, applicable Failed enforcement or
insufficient window/identity evidence blocks it even if CAP-1 accounting is complete.

## 7. Report and document grammar

All package objects below are closed: every listed field is required unless explicitly marked
optional. Arrays have no implicit members; null is permitted only where stated. `Digest` is
64 lowercase hex SHA-256. `Id` matches `[a-z0-9][a-z0-9._-]{0,63}`. `Text` is a UTF-8 string
bounded by §8; no implicit truth is attached to prose. `Count` is §2's exact integer domain.
`Surface` is tool_call/filesystem/network/process/transcript_join, in that order. `Time` is
`YYYY-MM-DDTHH:MM:SS[.fraction]Z`, a valid Gregorian UTC date with 1–9 optional fractional digits,
no leap-second 60. Unless ordered input is preserved, arrays sort lexicographically by stated ID.
`non_claims` is always exactly ["no_activity_completeness","no_provider_outcome","no_automatic_trust"].

A `Ref` is exactly `{input_id,sha256,locator}`: Id, Digest, Text. Input ID and original SHA must both
match the inventory. A JSON locator is `json:` plus RFC 6901 pointer; root is **`json:`** (the empty
pointer), not `json:/` (the empty-name property). Array indices have no leading zeros except zero.
Event locators percent-encode run_id UTF-8 bytes outside RFC 3986 unreserved characters, with
uppercase hex; assayseq is a base-10 integer without leading zeros. `bytes:0:<length>` selects the
entire opaque input only. The line grammar is in §3. No URL resolution, `..`, wildcard or fuzzy
locator matching exists. A `Ref` to a subrecord still binds the entire original input digest.
Canonical event references additionally require the verified events.ndjson member digest in the
examination value; archive digest and semantic root remain separate.

### 7.1 Inventory

Inventory fields are `schema`=assay.incident.inventory.v1, `assessment_sha256`:Digest,
`inputs`:Input[], `context`:Context (§9), `non_claims`.
Input fields are `id`:Id, `sha256`:Digest, `bytes`:Count, `format`:§3 token,
`surfaces`:Surface[], `binding`:null or Binding. IDs are unique and sorted.
The original object path is `objects/<sha256>`; size must match its original bytes.
No unreferenced object is permitted; equal-digest occurrences remain individually listed.

Binding has exactly `bundle_input`:Id, `key_input`:Id, `attestation_input`:Id|null.
Only dsse-attestation and attestation-report use Binding; all other inputs require null.
A DSSE binding names a bundle and Ed25519-key input and has attestation_input=null.
A retained report additionally names the DSSE input whose binding must match its bundle/key.
These references are type-checked and cannot form cycles. A bundled key is material, not authority:
the externally selected verification context must independently select its exact digest.

Surfaces for a raw proxy input are [tool_call], for Inspector [transcript_join], for context-only
formats []; bundle surfaces may be a subset of the first four, but every action event's mapped
surface must be included. An empty declared bundle surface requires a retained matching profile or
sandbox summary as enumeration context, not a guessed absent population. A record mapped outside
the declaration refuses before accounting. A declaration never establishes capture completeness.

### 7.2 Results and assessment

An `ExaminationResult` has exactly `operation`:§3 format token, `source`:Ref, `value`:JSON value.
Value is the parsed original unit after the specified admission/examination, **not just a pointer**.
For JSON/NDJSON action units it is the original parsed complete record, preserving unknown foreign
fields; for a canonical event it is `{event,events_sha256}` with the complete parsed event and the
verified events.ndjson raw digest. No re-serialized projection replaces source bytes. Equality of
parsed values uses exact integer/string/boolean/null semantics, not lossy floating conversion.
Only a successful examination can produce this result; an error/denial inside a successfully parsed
record remains that recorded error/denial. Parsing it does not claim the requested action happened.

A result document has exactly `schema`=assay.incident.results.v1 and `results`:ExaminationResult[].
Its source refs point only to admitted action units, never result documents or assessment. The
original result-document bytes are retained and digested like other input. Array order is original,
so `json:/results/0` is stable. Result documents are context and do not add eligible action units.

Assessment fields are `schema`=assay.incident.assessment.v1,
`verification_context_sha256`:Digest, `units`:Unit[], `surfaces`:SurfaceResult[],
`cap1`:CAP-1|null, `joins`:Join[], `claims`:Claim[], `resolved_results`:Ref[].
The context digest is over §9's externally supplied canonical context bytes, without any package
or report digest. This avoids a cycle and prevents changing the interpretation context silently.

Unit fields are `id`:Digest, `source`:Ref, `surface`:Surface,
`disposition`:examined or §6 disposition, `result`:Disclosure|null, `withheld_digest`:Digest|null.
Unit ID is SHA-256 of JCS bytes **without LF** of `[input_id,sha256,locator]`; duplicate unit IDs
refuse. Repeated records in different occurrences or at different locators have different IDs.
Units sort by ID. `Disclosure` is the tagged union:

- `{mode:"inline",value:ExaminationResult}`; or
- `{mode:"reference",target:Ref}` where target selects exactly one ExaminationResult from an
  incident-results-v1 input, not merely an original source or arbitrary JSON object.

Examined requires a Disclosure, null withheld_digest, matching unit source and operation, and
fresh examination equality. Reference resolution verifies original object digest, pointer,
result shape, source binding and fresh value equality before counting examined. No chaining.
Unresolved, wrong-shape or unequal disclosed results refuse; they cannot become withheld.
Withheld requires result=null and a non-null digest of canonical ExaminationResult bytes (no LF),
and counts only as withheld. Other unexamined dispositions require both nullable fields null.
The choice to withhold/disable/exclude is recorded examination policy, not inferred from source class.
A digest alone does not make withheld material available. Where this v1's deterministic parsing
result can be recomputed from retained source bytes, compare its canonical digest as well; retain
the recorded withheld disposition and do not double-count it as examined. Withholding a derived
result while publishing its inputs does not guarantee confidentiality. If required source material
is unavailable, admission cannot fabricate a successful fresh examination. Required-disclosure
expectations may reject such a package through the external context.

Resolved_results is the sorted unique set of **reference-mode result targets actually resolved**,
ordered by input_id then locator then digest. Inline source refs are not mislabeled as disclosed
result references. Recompute it and `resolved_result_count`; a stored list is not evidence of a read.
Every eligible enumerated action occurrence must appear exactly once; no surplus units are permitted.
Context input parsing is admission, not an invented action denominator.

SurfaceResult has exactly `name`:Surface, `source_inputs`:Id[],
`observation`:unknown|limited|adequate, `basis_refs`:Ref[], `window`:Window|null.
Window is `{start:Time,end:Time,clock_basis:Text}`, start<=end.
All five rows are required in fixed Surface order. Source IDs are recomputed from inventory mappings.
No sources forces unknown, empty basis_refs and null window. Otherwise the external applicability
entry in §9 determines the maximum, further reduced by applicable retained loss/Failed evidence.
Adequate cannot be created by a package flag. Unknown source or an inapplicable clock/window cannot
support activity absence; stated retention is an assertion, not authenticated independent provenance.

### 7.3 Join and claim shape

A Join has exactly `id`:Id, `left`:Ref, `right`:Ref|null, `contract`:JoinContract,
`proposition`:Proposition, `result`:confirmed|contradicted|unobservable.
JoinContract retains the unchanged foreign identity and has exactly these fields:

| Field | Type/value |
| --- | --- |
| schema | assay.observability.join_result.v0 |
| left_artifact_role, right_artifact_role | otel_family_trace/measured_run_archive/joined_artifacts/external_receipt |
| join_key | tool_call_id/run_id/session_id/trace_span_id/timestamp_or_order |
| join_value | Text or null |
| join_grade | strong/weak/diagnostic/failed |
| scope | tool_call/run/session/trace_local/diagnostic |
| unique_within_scope, fallback_used | boolean |
| evidence_refs | nonempty Text[] encoding exact `input_id#locator`, resolved through inventory |
| notes | Text[] |

The pinned [join schema](../reference/observability/schema/join-result-v0.schema.json) and its
[scope rules](../reference/observability/join-contract-v0.md) also apply; unknown fields refuse.
Proposition is outside that closed foreign object. Canonical bundle roles are measured_run_archive;
other admitted single inputs are external_receipt. This v1 never assigns otel_family_trace to an
Inspector history. Both wrapper refs must appear in the contract evidence refs when non-null.

Strong tool-key extraction is closed: proxy and canonical decision records use
`/data/tool_call_id`; Inspector uses `/message/params/arguments/_meta/tool_call_id`. Values must be
nonempty exact strings and unique among the relevant run/window's admitted units. Inspector
JSON-RPC id is not silently promoted to this field; the proxy's `req_`/`gen_` fallback does not
establish independently propagated identity. A strong run join uses verified canonical
`/assayrunid` on both sides. No other foreign input supplies a strong run field in this version.
Session/trace keys are contextual declared refs only and never strong; timestamp/order is diagnostic.
The proposition pointers must correspond to these exact admitted key paths for counterpart search;
arbitrary argument/tool-name equality cannot become a correlation key. These restrictions preserve
the join contract's strength ceiling even when most current kernel records lack a per-tool key.

Proposition is exactly `{kind,left_pointer,right_pointer,expected,surface}`. Kind is
`equal_retained_value` or `counterpart_present`; pointers are RFC6901 strings into the parsed records,
expected is a JSON scalar (string/Count/boolean/null), surface is Surface. For equal_retained_value,
right must be non-null, both pointers must resolve to scalars, and expected=null; byte-bound equal
values confirm and unequal values contradict **only with a strong uniquely scoped join**. Otherwise
result is unobservable. For counterpart_present, left_pointer selects the correspondence key,
right_pointer names that key's location in candidate counterpart records, and expected is the
non-null expected exact key value. Search only declared target-surface units in the applicable
window. Exactly one matching key with strong tool/run scope confirms that narrow presence relation;
multiple matches are ambiguous/unobservable. Zero matches contradict only under externally
justified adequate capture/retention, a complete applicable window and unique source key;
otherwise zero matches are unobservable. A missing right makes contract.join_grade=failed,
join_value=null, fallback_used=false; this failed join does not erase a separately supported
absence-based contradiction. Session/time matches cannot supply per-tool uniqueness.

Claims have exactly `id`:Id, `kind`:positive_presence|bounded_negative|exhaustive_set,
`surface`:Surface, `stratum`:Id, `proposition`:Id referencing a Join, `evidence_refs`:Ref[],
`decision`:supported|degraded|blocked. These are package decision names, not renamed existing enums.
A positive claim requires confirmed positive retained evidence within the join's scope; otherwise
blocked. Bounded negative requires an adequately justified contradicted counterpart_present
proposition and permitted CAP-1 support; otherwise blocked. Exhaustive_set requires adequate
observation, full accounted supplied population and no ambiguous joins within the declared window;
otherwise degraded, or blocked if admission/CAP-1 fails. These rules do not assert execution outcome.
CAP-1 schema/R0–R8 and cited supports are necessary before any supported decision. Applicable Failed
health blocks bounded negative and exhaustive support, without converting health parsing into failure.
The recorded assessment is recomputed under its bound external context; mismatch is stale_assessment.

### 7.4 Fresh verification result

The external sibling result has exactly `schema`=assay.incident.verify.v1,
`outcome`:package_verified|package_refused|verification_unavailable, `reason`:null|§10 token,
`artifact_sha256`:Digest|null, `inventory_sha256`:Digest|null, `assessment_sha256`:Digest|null,
`expectation`:not_requested|matched|mismatched, `attestations`:AttestationCheck[],
`verification_context`:ContextInput|null, `verification_context_sha256`:Digest|null,
`resolved_results`:Ref[], `counts`:Counts|null, `non_claims`.
Counts has inputs/objects/units/examined/withheld/resolved_result_count, each Count.
Established digests are over complete raw bytes; interrupted reads cannot yield a final digest.
Refused/unavailable have a non-null reason, empty unresolved results and null unestablished counts;
verified has reason=null and complete counts/context. No single safe/clean boolean exists.

AttestationCheck is `{input_id:Id,key_sha256:Digest,status:verified|refused|unavailable,
signature_verified:boolean,subject_matched:boolean,artifact_sha256:Digest|null,
extent_stated:boolean,extent:EvidenceExtent|null}`. Checked EvidenceExtent is exactly the existing
canonical API value; absent/null remain null, omitted sandbox network stays omitted. Separate
signature and artifact-match outcomes are never flattened. No selected attestation means [], not
signature_verified=true. A selected key mismatch or canonical verifier refusal rejects verification;
missing required external key material is unavailable. Retained reports must agree with freshly
established original artifact/signature/subject/extent fields; a copied OK string is insufficient.
There is no signature over the new outer package in v1; checks concern original inner artifacts.

The existing [attestation verifier](https://github.com/Rul1an/assay/blob/bf2659824e51c63edb433d2386b003f16a2da419/crates/assay-cli/src/cli/commands/evidence/verify_attestation.rs)
from #2852 is the precedent for raw artifact binding and separate signature/subject/extent results,
not a package verifier. Its extent does not establish activity coverage. ADR-049's old pending
status is historical: the extent library and CLI are implemented at this baseline. No signed
support_ceiling field is added; artifact-bound support remains interpretation of the predicate.

## 8. Resource contract

All values below are finite hard maxima and defaults in v1; caller/package cannot increase them.
A separately selected lower budget may refuse earlier and is recorded with the verifier invocation.
Charge counters before allocation/retention and cumulatively across input occurrences, even when
objects share bytes. Checked addition overflow refuses. MiB is 1048576 bytes; KiB is 1024 bytes.

| Dimension | Limit | Charge point |
| --- | ---: | --- |
| Outer source bytes | 256 MiB | Stream before retaining header/data |
| Outer headers/members | 128 | Before admitting each header |
| Outer path bytes | 80 | Before interpreting path |
| Unique object bytes, total | 240 MiB | Before retaining staged object bytes |
| Input occurrences | 64 | Before inventory array allocation |
| Inner compressed bundle | 100 MiB each | Existing bounded reader before materialization |
| Decoded inner bytes | 512 MiB aggregate, never over inner default | While decoding, shared across bundles/occurrences |
| Nested archive depth | 1 inner bundle below outer | No recursive unknown archive traversal |
| Inventory / assessment | 1 MiB / 16 MiB | Before JSON materialization |
| Policy / transcript / health-context object | 1 MiB / 16 MiB / 1 MiB each | Before object parse |
| DSSE envelope / public key | 32 MiB / 16 KiB | Before parse, baseline CLI maxima |
| Retained reports | 16 MiB each | Before JSON materialization |
| Inner manifest/events | 10 MiB / 500 MiB, also aggregate decoded cap | Existing inner limit plus shared budget |
| All records and context entries | 100000 aggregate | Before retaining each enumerated entry |
| Line or array-entry bytes | 1 MiB | Before parsing/retaining entry |
| Inner path length / JSON depth | 256 bytes / 64 containers | Existing reader; strict structural scan includes unknown fields |
| JSON key / non-payload string | 256 bytes / 64 KiB | Before allocation; original opaque argument text still subject to entry/source ceilings |
| Same-package references / traversal depth | 100000 / 1 | No reference chains/cycles; resolve directly to original object |
| Diagnostic / fresh result output | 4 KiB / 16 MiB | Before writing; static reason tokens, no source values |

Original JSON strings are also capped at 1 MiB, so payload exceptions never remove a finite string
bound. Repeated references use cached verified original bytes but still charge reference/operation
counts. Nonregular inputs, unsafe paths, malformed framing and exceeded ceilings refuse without
extracting. Implementation must expose bounded consumption witnesses; semantic parity after oversized
allocation is insufficient. No precise throughput or runtime-isolation claim follows from these caps.

Assay-evidence uses the existing workspace jsonschema dependency for the pinned normative schema.
The standalone gateway-evidence-replay keeps a local minimal validator without new assay-evidence or
jsonschema dependency. Both validate identical pinned fixtures and resource limits before semantics.
Do not change `VerifyLimits` or loosen existing readers to fit this package. Existing stricter inner
limits remain effective; the minimum of nested and aggregate budgets applies.

## 9. Context, privacy and trust inputs

Inventory context has exactly these keys, each holding `{value,reason,refs}`: original_digests,
chain_gaps, custody, producer_invocation, clock, actor, correlation, policy_activation, recoverability,
resource_effects, context_provenance, denominator, missing_evidence, withholding_derivatives,
retention, offline_verification, signature_basis, schema_versions, windows. Value is a bounded string
or null; reason is stated/not_available/not_applicable; refs are digest-bound input/locator pairs.
Stated values require non-null Text and at least one retained source reference; not_available and not_applicable require null value. Ordered custody/chain/clock/policy facts remain in referenced original bytes; the summary string does not create a substitute claim or inferred order. These
are disclosures, not an executable identity/delegation layer or automatically trusted testimony.

Trust roots, expected source digests and observation applicability are relying-party inputs.
Package keys/context alone do not establish trust. `ContextInput` has exactly `schema_pin`:Digest,
`limits`:Limits, `keys`:Digest[], `expectations`:Expectation[], `applicability`:Applicability[],
`as_of`:Time, `valid_from`:Time, `valid_until`:Time, `invocation`:Invocation.
All key digests and expectation IDs are unique/sorted. Keys select original Ed25519 PEM bytes;
only externally selected digests can authorize the key used by a DSSE binding. The public bytes may
be bundled for repeatability, but selecting them because they were bundled is not external trust.

Expectation is `{input_id:Id,sha256:Digest,require_disclosure:boolean}`. Each ID must exist and
match; require_disclosure=true rejects any unexamined unit from that input. No expectations means
not_requested; all satisfied means matched; any failed equality/disclosure means mismatched and
package_refused. Equality to these declarations is not provenance authentication.
Invocation is `{tool:Text,version:Text,options:Text[]}`; no environment, credentials or wall-clock
substitution. Limits is the exact numeric key map below, each positive integer <= its hard value;
zero is not unlimited. Units and charging points remain those in §8.

| Limits key | Hard value |
| --- | ---: |
| outer_bytes | 268435456 |
| members | 128 |
| outer_path_bytes | 80 |
| object_bytes_total | 251658240 |
| inputs | 64 |
| inner_compressed_bytes | 104857600 |
| decoded_bytes_total | 536870912 |
| nested_archive_depth | 1 |
| inventory_bytes | 1048576 |
| assessment_bytes | 16777216 |
| policy_bytes | 1048576 |
| transcript_bytes | 16777216 |
| health_bytes | 1048576 |
| envelope_bytes | 33554432 |
| key_bytes | 16384 |
| retained_report_bytes | 16777216 |
| inner_manifest_bytes | 10485760 |
| inner_events_bytes | 524288000 |
| records | 100000 |
| record_bytes | 1048576 |
| inner_path_bytes | 256 |
| json_depth | 64 |
| json_key_bytes | 256 |
| non_payload_string_bytes | 65536 |
| payload_string_bytes | 1048576 |
| references | 100000 |
| reference_depth | 1 |
| diagnostic_bytes | 4096 |
| fresh_result_bytes | 16777216 |

Result documents use retained_report_bytes; raw proxy files use transcript_bytes; coverage
sidecars use health_bytes. Every original object must fit its format's per-object limit as well
as object_bytes_total; canonical bundles use inner_compressed_bytes. Aggregate counters count
repeated input processing, not only unique storage. The declared metadata/context row counts are
also charged against records; no unbounded side channel through prose arrays.

Applicability is exactly `{surface:Surface,input_refs:Ref[],window:Window,
coverage:adequate|limited|unknown,retention:adequate|limited|unknown,
correspondence:unique|ambiguous|unknown,source_class:Text}`. At most one row per surface, in fixed
surface order; nonempty refs must resolve to retained context sources. source_class is one of the
six CodingAgentSourceClass snake-case values or unknown and remains externally stated provenance,
not automatically independently observed. Caller selects and owns this context outside the package.
Adequate requires both coverage and retention adequate, correspondence unique and a nonempty
applicable window with supporting refs. Source-class strings alone never satisfy those conditions.
Existing retained partial/absent/Failed or inconsistent context caps the applicable surface to
limited/unknown despite an adequate caller flag. Missing applicability yields unknown.

The exact canonical ContextInput bytes (JCS plus LF) are embedded as the verification_context
object, recoverable by the same canonical encoding; verification_context_sha256 binds those bytes.
Assessment must name that same digest. No separate free-text digest field or package override is
accepted. schema_pin equals the CAP-1 digest in §6, and valid_from<=as_of<=valid_until; otherwise
trust_input refusal. This checks caller-selected validity, not authenticated time. No automatic
"fresh" status is inferred from a timestamp. A cold repeat must explicitly supply the same context
and retained public key material. The report is evidence of the selected interpretation inputs,
not independent authentication of those inputs. A different context is a different verification;
it cannot silently inherit the recorded assessment's result.

An original context document has exactly `schema`=assay.incident.context.v1 and `entries`:Entry[].
Entry has exactly `kind` (one of the nineteen Context keys), `value`:Text, `source_refs`:Ref[].
The kind states the disclosure obligation, not a trust class. source_refs may be empty for an
unsupported assertion; otherwise they resolve only to non-context, non-result original input, so
there are no context reference chains. Duplicate kinds remain distinct ordered entries. Inventory
Context may point to an entry with matching kind; the original entry and any underlying source
must remain distinguishable. This is explicitly operator-supplied context, not an invented native
vendor record. Context documents use health_bytes and all entry/reference budgets.

Redaction is a separately digested derivative. If original required bytes are withheld, disclose
which verification cannot repeat; do not attach the old signature to new bytes. Custody is asserted
history unless independently supported. UTC timestamps carry clock source/drift or explicit unknown;
no timestamp proximity upgrade. Retention statement is not legal hold. No secrets in diagnostics,
raw environment dumps or automatically collected principal credentials.

## 10. Future behavioral acceptance (not executed here)

Refusal reasons are input_shape, unknown_format, unknown_schema, locator, mapping, digest,
policy_binding, disclosure, stale_assessment, cap1_shape, cap1_rules, claim_boundary, resource,
atomic_output, trust_input, or io_unavailable. Failure reasons do not echo input strings.

| Named RED case | Effective mutation that must bite | Positive/control |
| --- | --- | --- |
| unknown_format_before_accounting | Map unknown format/type to unsupported_input | Valid admitted input |
| missing_source_is_not_empty | Fabricate zero stratum/adequate coverage | Verified present-empty retains unknown activity |
| adequate_spoof | Return confirmed OR unobservable for adequately established contradiction | Unique applicable paired observation |
| blinded_twin | Return contradicted from missing row under inadequate coverage | Same payload, adequate independently justified boundary |
| failed_health_blocks_absence | Permit activity absence beside applicable Failed health | Positive presence remains separately assessable |
| occurrence_accounting | Drop/dedupe an occurrence or expand hits | Duplicate IDs retained as distinct digest-bound occurrences |
| withholding_digest | Drop R3 digest requirement or double-count examined | Digest-bound withheld result only |
| replaced_member_or_policy | Bypass actual digest/binding | Original bytes unchanged |
| stale_result | Trust recorded OK/counts without recomputation | Fresh equal recomputation |
| missing_surplus_member | Skip membership check, including inner extra member | Exact allowlist |
| unresolved_reference | Treat unresolved as examined/withheld | Direct matched reference |
| schema_rules_then_claims | Remove schema, R0–R8 or claim stage separately | Pinned positive CAP-1 fixtures |
| budget_parity | Move each cap after allocation, reset aggregate, or diverge leaf | Exact limit and limit+1, counting-reader witness |
| atomic_output | Truncate prior destination before validation | Refusal preserves prior bytes; valid new output completes |
| deterministic_and_cold | Include wall clock/unordered metadata or rely on workspace/network | Identical selected inputs; standalone passive repeat |

Every mutation is on the effective production boundary, with retained before/after bytes, named
assertion failure and no-op/restored controls. Compile/setup/infrastructure errors are not RED.
One shared fixture matrix compares Assay and leaf schema, R0–R8, claim results and each resource
boundary, including nested aggregate exhaustion and unknown-field depth. No package executable is
run. These are downstream test requirements, not tests accomplished by this document.

## 11. Completion and exclusions

This contract cannot be marked frozen while a required report/input relationship or admitted
interpretation remains deferred. All examples must resolve against the closed grammar and all
required trust inputs must be replayable. A partial design is not implementation permission.

Delivery order remains contract → CAP-1/parity → A original-byte packaging → B all-five-surface
session integration → C standalone artifact and cold-machine demonstration. Operator local inputs
avoid a dependency on a retention index for A, not the #2492 dependency for store-backed acquisition.
A alone does not close #2493. Shared service requires an accepted authorization/tenant ADR first.

No trust score, whole-action verdict, provider-outcome proof, exhaustive-capture, certification,
compliance, legal hold, tenant-isolation or safe-agent claim. Local verification, CI, merge, release,
distribution and installed-user/cold-machine proof remain distinct.

## 12. Complete inert examples

These are synthetic document examples, not host evidence, executed exporter results or a packaged
release. A1–A3 form a complete present-empty selection: its only original object is the three bytes
`[]` followed by LF. It has zero supplied transcript units and unknown observation on all five
surfaces, not a zero-activity conclusion. No CAP-1 stratum exists for the four absent sources.
The display is indented for reading; hash targets use the canonical JCS+LF encoding in §2, not
this Markdown whitespace. All numbers/keys in these examples are in the exact ASCII JCS subset.
An outer archive and a successful fresh verification are deliberately not claimed by these examples.

B1–B3 are complete component examples for the disclosed-result relation, separate from A1–A3.
Their required inventory occurrence IDs are `one-history` and `disclosed-results`, each storing
its canonical JSON+LF bytes; B3's result references actual B2 result content, not merely B1.
B4 differs from B3 at exactly the result target locator: array index 1 does not exist, so
`disclosure` refusal is required before examined accounting. It must never become withheld.
C is a complete valid refusal-report shape for an input rejected before a complete outer digest
is established; its nulls are unestablished results, not passed checks.

### Example A1 — external context

```json
{
  "schema_pin": "4453f216089543780bfecc4295cc4a61462fdc585b88d1e35b7d1aba79716b4a",
  "limits": {
    "outer_bytes": 268435456,
    "members": 128,
    "outer_path_bytes": 80,
    "object_bytes_total": 251658240,
    "inputs": 64,
    "inner_compressed_bytes": 104857600,
    "decoded_bytes_total": 536870912,
    "nested_archive_depth": 1,
    "inventory_bytes": 1048576,
    "assessment_bytes": 16777216,
    "policy_bytes": 1048576,
    "transcript_bytes": 16777216,
    "health_bytes": 1048576,
    "envelope_bytes": 33554432,
    "key_bytes": 16384,
    "retained_report_bytes": 16777216,
    "inner_manifest_bytes": 10485760,
    "inner_events_bytes": 524288000,
    "records": 100000,
    "record_bytes": 1048576,
    "inner_path_bytes": 256,
    "json_depth": 64,
    "json_key_bytes": 256,
    "non_payload_string_bytes": 65536,
    "payload_string_bytes": 1048576,
    "references": 100000,
    "reference_depth": 1,
    "diagnostic_bytes": 4096,
    "fresh_result_bytes": 16777216
  },
  "keys": [],
  "expectations": [],
  "applicability": [],
  "as_of": "2026-09-08T00:00:00Z",
  "valid_from": "2026-09-08T00:00:00Z",
  "valid_until": "2026-09-09T00:00:00Z",
  "invocation": {
    "tool": "illustrative-reader",
    "version": "example-only",
    "options": []
  }
}
```

### Example A2 — complete recorded assessment

```json
{
  "schema": "assay.incident.assessment.v1",
  "verification_context_sha256": "919787b66b4dc0688944d7c76218c6da168890f8251985ccfb12e2a92cfcb814",
  "units": [],
  "surfaces": [
    {
      "name": "tool_call",
      "source_inputs": [],
      "observation": "unknown",
      "basis_refs": [],
      "window": null
    },
    {
      "name": "filesystem",
      "source_inputs": [],
      "observation": "unknown",
      "basis_refs": [],
      "window": null
    },
    {
      "name": "network",
      "source_inputs": [],
      "observation": "unknown",
      "basis_refs": [],
      "window": null
    },
    {
      "name": "process",
      "source_inputs": [],
      "observation": "unknown",
      "basis_refs": [],
      "window": null
    },
    {
      "name": "transcript_join",
      "source_inputs": [
        "empty-history"
      ],
      "observation": "unknown",
      "basis_refs": [],
      "window": null
    }
  ],
  "cap1": {
    "profile": "cap/1",
    "subject": {
      "kind": "collection",
      "ref": "incident-selected-inputs"
    },
    "strata": [
      {
        "id": "transcript_join",
        "population": "retained supplied records",
        "basis": {
          "kind": "enumeration",
          "enumeration_method": "incident-v1-digest-locator-enumeration"
        },
        "eligible": 0,
        "examined": 0,
        "unexamined": [],
        "supports": [
          "positive_presence",
          "bounded_negative",
          "exhaustive_set"
        ]
      }
    ],
    "integrity": {
      "complete": true,
      "statement": "accounting of selected supplied records",
      "capped_to": null
    }
  },
  "joins": [],
  "claims": [],
  "resolved_results": []
}
```

### Example A3 — complete inventory

```json
{
  "schema": "assay.incident.inventory.v1",
  "assessment_sha256": "921dda0ca89d242f89ed1450748a3934335103cc4fdb34dc731969f53bd7397c",
  "inputs": [
    {
      "id": "empty-history",
      "sha256": "37517e5f3dc66819f61f5a7bb8ace1921282415f10551d2defa5c3eb0985b570",
      "bytes": 3,
      "format": "inspector-protocol-793d103",
      "surfaces": [
        "transcript_join"
      ],
      "binding": null
    }
  ],
  "context": {
    "original_digests": {
      "value": null,
      "reason": "not_available",
      "refs": []
    },
    "chain_gaps": {
      "value": null,
      "reason": "not_available",
      "refs": []
    },
    "custody": {
      "value": null,
      "reason": "not_available",
      "refs": []
    },
    "producer_invocation": {
      "value": null,
      "reason": "not_available",
      "refs": []
    },
    "clock": {
      "value": null,
      "reason": "not_available",
      "refs": []
    },
    "actor": {
      "value": null,
      "reason": "not_available",
      "refs": []
    },
    "correlation": {
      "value": null,
      "reason": "not_available",
      "refs": []
    },
    "policy_activation": {
      "value": null,
      "reason": "not_available",
      "refs": []
    },
    "recoverability": {
      "value": null,
      "reason": "not_available",
      "refs": []
    },
    "resource_effects": {
      "value": null,
      "reason": "not_available",
      "refs": []
    },
    "context_provenance": {
      "value": null,
      "reason": "not_available",
      "refs": []
    },
    "denominator": {
      "value": null,
      "reason": "not_available",
      "refs": []
    },
    "missing_evidence": {
      "value": null,
      "reason": "not_available",
      "refs": []
    },
    "withholding_derivatives": {
      "value": null,
      "reason": "not_available",
      "refs": []
    },
    "retention": {
      "value": null,
      "reason": "not_available",
      "refs": []
    },
    "offline_verification": {
      "value": null,
      "reason": "not_available",
      "refs": []
    },
    "signature_basis": {
      "value": null,
      "reason": "not_available",
      "refs": []
    },
    "schema_versions": {
      "value": null,
      "reason": "not_available",
      "refs": []
    },
    "windows": {
      "value": null,
      "reason": "not_available",
      "refs": []
    }
  },
  "non_claims": [
    "no_activity_completeness",
    "no_provider_outcome",
    "no_automatic_trust"
  ]
}
```

### Example B1 — original one-entry history

```json
[
  {
    "id": "entry-1",
    "timestamp": "2026-09-08T00:00:00Z",
    "direction": "request",
    "origin": "client",
    "message": {
      "jsonrpc": "2.0",
      "id": "rpc-1",
      "method": "tools/call",
      "params": {
        "name": "list_files",
        "arguments": {}
      }
    }
  }
]
```

### Example B2 — disclosed result document

```json
{
  "schema": "assay.incident.results.v1",
  "results": [
    {
      "operation": "inspector-protocol-793d103",
      "source": {
        "input_id": "one-history",
        "sha256": "bdcc86fa294237f869226095e26769922719ecccf5d159101b95f447dca5c9e8",
        "locator": "json:/0"
      },
      "value": {
        "id": "entry-1",
        "timestamp": "2026-09-08T00:00:00Z",
        "direction": "request",
        "origin": "client",
        "message": {
          "jsonrpc": "2.0",
          "id": "rpc-1",
          "method": "tools/call",
          "params": {
            "name": "list_files",
            "arguments": {}
          }
        }
      }
    }
  ]
}
```

### Example B3 — reference-mode unit

```json
{
  "id": "6b79921c1021c5ba3e32b5cb5a739c0f7374ba16e0e4f9324b00b0aa485e0985",
  "source": {
    "input_id": "one-history",
    "sha256": "bdcc86fa294237f869226095e26769922719ecccf5d159101b95f447dca5c9e8",
    "locator": "json:/0"
  },
  "surface": "transcript_join",
  "disposition": "examined",
  "result": {
    "mode": "reference",
    "target": {
      "input_id": "disclosed-results",
      "sha256": "2e5e9cc3b58dd7a6dda803c2e89e9b7a05b361da5d0bf3dff3792c276044add7",
      "locator": "json:/results/0"
    }
  },
  "withheld_digest": null
}
```

### Example B4 — invalid unresolved result target

```json
{
  "id": "6b79921c1021c5ba3e32b5cb5a739c0f7374ba16e0e4f9324b00b0aa485e0985",
  "source": {
    "input_id": "one-history",
    "sha256": "bdcc86fa294237f869226095e26769922719ecccf5d159101b95f447dca5c9e8",
    "locator": "json:/0"
  },
  "surface": "transcript_join",
  "disposition": "examined",
  "result": {
    "mode": "reference",
    "target": {
      "input_id": "disclosed-results",
      "sha256": "2e5e9cc3b58dd7a6dda803c2e89e9b7a05b361da5d0bf3dff3792c276044add7",
      "locator": "json:/results/1"
    }
  },
  "withheld_digest": null
}
```

### Example C — complete refusal-report shape

```json
{
  "schema": "assay.incident.verify.v1",
  "outcome": "package_refused",
  "reason": "unknown_format",
  "artifact_sha256": null,
  "inventory_sha256": null,
  "assessment_sha256": null,
  "expectation": "not_requested",
  "attestations": [],
  "verification_context": null,
  "verification_context_sha256": null,
  "resolved_results": [],
  "counts": null,
  "non_claims": [
    "no_activity_completeness",
    "no_provider_outcome",
    "no_automatic_trust"
  ]
}
```

Additional single-change negative examples are specified relative to these complete values:
A2 integrity.complete=false without non-null capped_to violates R7; A2 adding an absence assertion
citing filesystem violates R6 because that stratum is absent; B3 changing withheld_digest from
null to a digest while remaining examined violates exclusive disposition; B2 changing the retained
request method while leaving its source reference fixed violates fresh result equality. Each
negative retains every unrelated byte-bound input and has the matching original as its control.
