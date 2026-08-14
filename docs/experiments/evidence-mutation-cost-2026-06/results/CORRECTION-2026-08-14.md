# Correction (2026-08-14)

Correction (2026-08-14): the shipped `run_root` is SHA-256 over newline-delimited
event content-hash strings, with a trailing newline, in event sequence order —
not a tree root, and not `event_id` bytes. References below to the historical
tree proposal describe the model used at the time and are not claims about the
shipped evidence format.

`cost.json` and `cost.md` in this directory are frozen 2026-06 measurement
bytes. They are not regenerated. The recorded numeric column is a
`ceil(log2(N))` comparison model only; production verification does not
consume it. Source generators may use corrected names for future runs.
