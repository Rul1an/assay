//! Runnable example for the Bombadil-style `noRawControlRendered` property.
//!
//! The property is intentionally smaller than Assay's full chrome/viewer proof: after untrusted
//! content is rendered for a sink, raw terminal-control bytes must not reach the rendered cell.

use assay_core::render_safety::{has_residual_control, render_safe, Sink, MAX_RENDER_FIELD};

#[test]
fn no_raw_control_rendered_blocks_esc_bel_and_c1_csi() {
    let probes = [
        ("ansi_esc_csi", "\u{1b}[31mred\u{1b}[0m"),
        ("bel", "status\u{7}done"),
        ("c1_csi", "\u{009b}31mred"),
    ];

    for sink in Sink::ALL {
        for (name, input) in probes {
            let rendered = render_safe(sink, input, MAX_RENDER_FIELD);

            assert!(
                !rendered.contains('\u{1b}'),
                "{name} leaked raw ESC in {}: {rendered:?}",
                sink.as_str()
            );
            assert!(
                !rendered.contains('\u{7}'),
                "{name} leaked raw BEL in {}: {rendered:?}",
                sink.as_str()
            );
            assert!(
                !rendered.contains('\u{009b}'),
                "{name} leaked raw 8-bit CSI in {}: {rendered:?}",
                sink.as_str()
            );
            assert!(
                !has_residual_control(&rendered),
                "{name} left residual terminal control in {}: {rendered:?}",
                sink.as_str()
            );
        }
    }
}
