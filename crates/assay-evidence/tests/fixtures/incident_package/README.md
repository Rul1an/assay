# Incident-package fixtures

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
