//! Same-run observation control for the mock upstream method log.

use std::path::Path;

pub(crate) fn read_methods(log: &Path) -> Vec<String> {
    let body = std::fs::read_to_string(log)
        .unwrap_or_else(|err| panic!("upstream observer log {} unreadable: {err}", log.display()));
    let methods: Vec<String> = body.lines().map(str::to_string).collect();
    // Every caller received initialize's reply. The mock logs and flushes that
    // method before replying, so this is a same-run observation control.
    assert!(
        methods.iter().any(|method| method == "initialize"),
        "upstream observer log {} has no initialize: absent tools/call is not evidence of no forwarding",
        log.display()
    );
    methods
}
