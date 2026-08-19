//! Mapping lock: `check_sequence` calls `assay_core::sequence_eval` and must not drop a
//! violation the owner reports on the proxy's `TraceExtent::Partial` reading.
//!
//! `assay-metrics` already called the shared function. This crate already depended on
//! `assay-core`. The second copy is gone; what remains is the published JSON envelope.
//! A differential over every sequence of length <= 5 on a three-symbol alphabet found 213
//! `after` disagreements before the call-through. This walk is what stops a mapping regression
//! from reopening that gap on the verdict.
//!
//! `TraceExtent::Partial` is the extent under test, because `check_sequence` validates a live
//! proxy's history-so-far: it asks whether what has happened *so far* violates the rule, with more
//! calls still possible. The metric asks the finished-run question. They are different questions
//! and only the first one is this file's business.

use assay_core::model::{Policy, SequenceRule};
use assay_core::sequence_eval::{evaluate_rules, TraceExtent};

/// Every sequence of length <= 4 over a fixed alphabet, so a divergence anywhere in the space is
/// found rather than only at the cases someone thought to write down.
fn all_traces(alphabet: &[&str], max_len: usize) -> Vec<Vec<String>> {
    let mut out = vec![Vec::new()];
    let mut frontier = vec![Vec::<String>::new()];
    for _ in 0..max_len {
        let mut next = Vec::new();
        for t in &frontier {
            for sym in alphabet {
                let mut c = t.clone();
                c.push((*sym).to_string());
                next.push(c);
            }
        }
        out.extend(next.iter().cloned());
        frontier = next;
    }
    out
}

fn rules_under_test() -> Vec<SequenceRule> {
    vec![
        SequenceRule::Require { tool: "A".into() },
        SequenceRule::Blocklist {
            pattern: "B".into(),
        },
        SequenceRule::Before {
            first: "A".into(),
            then: "B".into(),
        },
        SequenceRule::NeverAfter {
            trigger: "A".into(),
            forbidden: "B".into(),
        },
        SequenceRule::MaxCalls {
            tool: "A".into(),
            max: 1,
        },
        SequenceRule::Eventually {
            tool: "A".into(),
            within: 2,
        },
        SequenceRule::After {
            trigger: "A".into(),
            then: "B".into(),
            within: 1,
        },
        SequenceRule::After {
            trigger: "A".into(),
            then: "B".into(),
            within: 2,
        },
        SequenceRule::Sequence {
            tools: vec!["A".into(), "B".into()],
            strict: false,
        },
        SequenceRule::Sequence {
            tools: vec!["A".into(), "B".into()],
            strict: true,
        },
    ]
}

#[test]
fn both_evaluators_agree_on_whether_a_rule_is_violated() {
    // No aliases: both sides resolve them through the same `Policy` method, so an alias
    // table would test that method rather than the mapping.
    let policy: Policy =
        serde_yaml::from_str("version: \"1\"\nsequences: []\n").expect("minimal policy parses");
    let traces = all_traces(&["A", "B", "C"], 4);
    let rules = rules_under_test();

    let mut permissive = Vec::new();
    let mut stricter = 0usize;
    for rule in &rules {
        for trace in &traces {
            // The owner reads calls; the tool maps names into that record (#2124).
            let as_calls: Vec<assay_core::sequence_eval::SequenceCall> = trace
                .iter()
                .map(|n| assay_core::sequence_eval::SequenceCall::named(n.clone()))
                .collect();
            let shared = evaluate_rules(
                std::slice::from_ref(rule),
                &as_calls,
                Some(&policy),
                TraceExtent::Partial,
            );
            let shared_violates = shared[0].is_violation();
            let published = assay_mcp_server::tools::check_sequence::validate_rules_for_parity(
                std::slice::from_ref(rule),
                trace,
                Some(&policy),
            );

            match (shared_violates, published) {
                // A mapping bug: the tool reports a violation the owner did not.
                (false, true) => permissive.push(format!(
                    "{rule:?} on {trace:?}: published=violation shared={:?}",
                    shared[0].outcome
                )),
                // A mapping drop: the owner violated and the published JSON did not.
                (true, false) => stricter += 1,
                _ => {}
            }
        }
    }

    assert!(
        permissive.is_empty(),
        "the published JSON reports a violation the owner did not on {} case(s):\n{}",
        permissive.len(),
        permissive.join("\n")
    );
    assert_eq!(
        stricter, 0,
        "the published JSON dropped {stricter} owner violation(s), expected 0."
    );
}
