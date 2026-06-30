# Render Safety: no raw control rendered

This is the Assay runnable example for the narrow `noRawControlRendered`
property: untrusted content may contain terminal control bytes, but raw control
must not reach a rendered sink cell.

The example is deliberately smaller than Assay's full terminal-viewer proof. It
does not assert anything about chrome layout, header stability, cursor position,
or a particular terminal UI. It only asserts the reusable sink boundary:

- raw ESC (`0x1b`) does not survive rendering;
- raw BEL (`0x07`) does not survive rendering;
- raw C1 / 8-bit CSI (`0x9b`) does not survive rendering;
- the same property holds across Assay's stdout, JSON, SARIF, JUnit, Markdown,
  and OTel render sinks.

The test drives the real `assay-core` render-safety pipeline:

```bash
cargo test -p assay-core no_raw_control_rendered_blocks_esc_bel_and_c1_csi -- --nocapture
```

The broader release-gate witness is pinned separately as
`assay.render_safety_conformance.v0`. It runs the shared hostile/benign corpus
through every sink and requires `terminal_control_leak_count == 0` for each sink:

```bash
cargo test -p assay-core conformance_matches_golden_fixture -- --nocapture
```

The conformance corpus includes a C1 CSI probe, so the golden digest changes if
that coverage is removed.
