//! One evaluation of the sequence-rule language, and a record of what it evaluated.
//!
//! Two things live here, and the second is the reason the first moved.
//!
//! **One implementation, most of the way.** The rule language had two evaluators. `assay-metrics`'
//! `sequence_valid` handled `Require`, `Before` and `Blocklist` and resolved no aliases;
//! `assay-mcp-server`'s `check_sequence` handled all eight variants and did resolve them.
//! The same suite YAML therefore got two answers, and the silent one was the pass: a
//! `never_after` rule, which is the shape a credential-read-then-egress policy is written
//! in, fell through the metric's `_` arm and reported a clean run. `assay-metrics` cannot
//! call `assay-mcp-server` (the dependency runs the other way), so the shared home is here.
//!
//! `assay-mcp-server` has not called through yet: its JSON violation shape is a published tool
//! contract and porting it means preserving message text field by field. Until it does the two
//! are guarded by `assay-mcp-server/tests/sequence_eval_parity.rs` rather than by a shared call,
//! which is the fallback CLAUDE.md sanctions and the weaker of the two options. What that test
//! guards is not hypothetical: a differential over every trace of length <= 5 on a three-symbol
//! alphabet found 213 `after` disagreements between the copies before [`TraceExtent`] existed.
//!
//! **A record, not a verdict.** Each rule yields a [`RuleEvaluation`] naming the rule, the
//! call indices it read, and what it found. A consumer recomputes the conclusion from the
//! carried span rather than accepting a severity, which is what ADR-042 requires of a claim:
//! bounded, and checkable by someone who does not trust the producer. Nothing here
//! aggregates: there is no score, no whole-run verdict, and a caller that wants one has to
//! write the reduction itself and own it.
//!
//! [`RuleOutcome::NotExercised`] is the member that makes the record worth carrying. A
//! `before` rule whose `then` tool never appears passes without its antecedent ever firing,
//! and a rule kind this build does not implement passes for a different reason entirely.
//! Both used to be indistinguishable from a rule that ran and held. They are separate values
//! now, for the same reason [`crate::metrics_api::Exercised`] exists one layer down.

use crate::model::{Policy, SequenceRule};

/// Whether more calls may still arrive.
///
/// The rule language was first evaluated by a live proxy checking history-so-far, where a
/// deadline not yet met may still be met by the next call. A metric evaluates a finished run,
/// where it cannot. The two readings disagree on every rule with a window, and the difference is
/// invisible in the rules and the trace -- it is only in who is asking. So the caller states it
/// rather than the evaluator assuming it. Porting the proxy's reading into the metric silently is
/// what made completed runs with an unmet deadline report as undecided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceExtent {
    /// The run is over. An unmet deadline is a violation, because nothing further is coming.
    Complete,
    /// More calls may follow. An unmet deadline whose window is still open has not decided.
    Partial,
}

/// What one rule found. Deliberately three values: a rule that did not run is not a rule
/// that passed, and folding them loses the distinction this module exists to keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleOutcome {
    /// The rule's antecedent fired and the constraint held.
    Held,
    /// The rule's antecedent fired and the constraint did not hold.
    Violated,
    /// The rule never got a chance to decide. Either its antecedent never fired, or this
    /// build has no implementation for the rule kind. `reason` says which.
    NotExercised,
}

impl RuleOutcome {
    /// The stable string for this value. It reaches `details` in a `MetricResult` and any
    /// evidence projection built over one, so it is an interface rather than a `Debug` view.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Held => "held",
            Self::Violated => "violated",
            Self::NotExercised => "not_exercised",
        }
    }
}

/// One rule's evaluation against one call sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleEvaluation {
    /// Stable identity for this rule within a policy: kind plus its operands, so a consumer
    /// can key on it across runs without depending on list position.
    pub rule_id: String,
    /// The rule kind, as written in the policy vocabulary.
    pub kind: &'static str,
    pub outcome: RuleOutcome,
    /// The call indices this rule actually read to reach its outcome. Empty when the rule
    /// read nothing, which is the honest span for a kind that is not implemented.
    pub spanned: Vec<usize>,
    /// Why, in the producer's words. Present for `Violated` and `NotExercised`; a held rule
    /// needs no prose, and inventing one would invite readers to parse it.
    pub reason: Option<String>,
}

impl RuleEvaluation {
    fn held(rule_id: String, kind: &'static str, spanned: Vec<usize>) -> Self {
        Self {
            rule_id,
            kind,
            outcome: RuleOutcome::Held,
            spanned,
            reason: None,
        }
    }
    fn violated(rule_id: String, kind: &'static str, spanned: Vec<usize>, reason: String) -> Self {
        Self {
            rule_id,
            kind,
            outcome: RuleOutcome::Violated,
            spanned,
            reason: Some(reason),
        }
    }
    fn not_exercised(rule_id: String, kind: &'static str, reason: String) -> Self {
        Self {
            rule_id,
            kind,
            outcome: RuleOutcome::NotExercised,
            spanned: Vec::new(),
            reason: Some(reason),
        }
    }

    /// Whether this evaluation should fail the run.
    pub const fn is_violation(&self) -> bool {
        matches!(self.outcome, RuleOutcome::Violated)
    }
}

fn resolve(policy: Option<&Policy>, tool: &str) -> Vec<String> {
    match policy {
        Some(p) => p.resolve_alias(tool),
        None => vec![tool.to_string()],
    }
}

fn matches_any(name: &str, targets: &[String]) -> bool {
    targets.iter().any(|t| t == name)
}

fn indices_matching(names: &[String], targets: &[String]) -> Vec<usize> {
    names
        .iter()
        .enumerate()
        .filter(|(_, n)| matches_any(n, targets))
        .map(|(i, _)| i)
        .collect()
}

/// Evaluate every rule against the ordered tool-call names, returning one record per rule.
///
/// Every rule is evaluated. An earlier caller returned on the first violation, which made the
/// records for later rules unobtainable rather than empty — a reader could not tell a rule
/// that held from one that was never reached. Callers that want fail-fast semantics reduce
/// over the result; the reduction is theirs to state.
pub fn evaluate_rules(
    rules: &[SequenceRule],
    actual_names: &[String],
    policy: Option<&Policy>,
    extent: TraceExtent,
) -> Vec<RuleEvaluation> {
    rules
        .iter()
        .map(|r| evaluate_rule(r, actual_names, policy, extent))
        .collect()
}

fn evaluate_rule(
    rule: &SequenceRule,
    names: &[String],
    policy: Option<&Policy>,
    extent: TraceExtent,
) -> RuleEvaluation {
    match rule {
        SequenceRule::Require { tool } => {
            let id = format!("require:{tool}");
            let targets = resolve(policy, tool);
            let hits = indices_matching(names, &targets);
            if hits.is_empty() {
                RuleEvaluation::violated(
                    id,
                    "require",
                    Vec::new(),
                    format!("required tool '{tool}' not found in trace"),
                )
            } else {
                RuleEvaluation::held(id, "require", hits)
            }
        }

        SequenceRule::Blocklist { pattern } => {
            let id = format!("blocklist:{pattern}");
            let hits: Vec<usize> = names
                .iter()
                .enumerate()
                .filter(|(_, n)| n.contains(pattern))
                .map(|(i, _)| i)
                .collect();
            if let Some(&idx) = hits.first() {
                RuleEvaluation::violated(
                    id,
                    "blocklist",
                    hits.clone(),
                    format!(
                        "tool '{}' matches blocklist pattern '{pattern}'",
                        names[idx]
                    ),
                )
            } else {
                // A blocklist reads every name, so it is exercised even when nothing matches.
                RuleEvaluation::held(id, "blocklist", (0..names.len()).collect())
            }
        }

        SequenceRule::Before { first, then } => {
            let id = format!("before:{first}->{then}");
            let first_t = resolve(policy, first);
            let then_t = resolve(policy, then);
            let first_idx = names.iter().position(|n| matches_any(n, &first_t));
            let Some(t_idx) = names.iter().position(|n| matches_any(n, &then_t)) else {
                // The antecedent never fired. Syntactically fine, vacuous for this trace.
                return RuleEvaluation::not_exercised(
                    id,
                    "before",
                    format!("'{then}' never appeared, so the ordering was never constrained"),
                );
            };
            match first_idx {
                Some(f_idx) if f_idx > t_idx => RuleEvaluation::violated(
                    id,
                    "before",
                    vec![f_idx, t_idx],
                    format!(
                        "tool '{first}' appeared at index {f_idx} but was required before tool '{then}' (index {t_idx})"
                    ),
                ),
                Some(f_idx) => RuleEvaluation::held(id, "before", vec![f_idx, t_idx]),
                None => RuleEvaluation::violated(
                    id,
                    "before",
                    vec![t_idx],
                    format!(
                        "tool '{then}' was found (index {t_idx}) but required preceding tool '{first}' was missing"
                    ),
                ),
            }
        }

        SequenceRule::NeverAfter { trigger, forbidden } => {
            let id = format!("never_after:{trigger}->{forbidden}");
            let trig_t = resolve(policy, trigger);
            let forb_t = resolve(policy, forbidden);
            let Some(trig_idx) = names.iter().position(|n| matches_any(n, &trig_t)) else {
                return RuleEvaluation::not_exercised(
                    id,
                    "never_after",
                    format!("'{trigger}' never appeared, so nothing was forbidden"),
                );
            };
            match names
                .iter()
                .enumerate()
                .skip(trig_idx + 1)
                .find(|(_, n)| matches_any(n, &forb_t))
            {
                Some((idx, _)) => RuleEvaluation::violated(
                    id,
                    "never_after",
                    vec![trig_idx, idx],
                    format!(
                        "tool '{forbidden}' at index {idx} is forbidden after '{trigger}' (triggered at index {trig_idx})"
                    ),
                ),
                None => RuleEvaluation::held(id, "never_after", vec![trig_idx]),
            }
        }

        SequenceRule::MaxCalls { tool, max } => {
            let id = format!("max_calls:{tool}<={max}");
            let targets = resolve(policy, tool);
            let hits = indices_matching(names, &targets);
            if hits.is_empty() {
                return RuleEvaluation::not_exercised(
                    id,
                    "max_calls",
                    format!("'{tool}' never appeared, so no ceiling was tested"),
                );
            }
            let count = hits.len() as u32;
            if count > *max {
                RuleEvaluation::violated(
                    id,
                    "max_calls",
                    hits,
                    format!("tool '{tool}' exceeded max calls ({count} > {max})"),
                )
            } else {
                RuleEvaluation::held(id, "max_calls", hits)
            }
        }

        SequenceRule::Eventually { tool, within } => {
            let id = format!("eventually:{tool}@{within}");
            let targets = resolve(policy, tool);
            match names.iter().position(|n| matches_any(n, &targets)) {
                Some(idx) if (idx as u32) >= *within => RuleEvaluation::violated(
                    id,
                    "eventually",
                    vec![idx],
                    format!(
                        "tool '{tool}' appeared at index {idx} but must appear within first {within} calls"
                    ),
                ),
                Some(idx) => RuleEvaluation::held(id, "eventually", vec![idx]),
                None if (names.len() as u32) > *within => RuleEvaluation::violated(
                    id,
                    "eventually",
                    (0..names.len()).collect(),
                    format!(
                        "tool '{tool}' required within first {within} calls but not found (trace length: {})",
                        names.len()
                    ),
                ),
                None if extent == TraceExtent::Complete => RuleEvaluation::violated(
                    id,
                    "eventually",
                    (0..names.len()).collect(),
                    format!(
                        "tool '{tool}' required within the first {within} calls but the run ended after {} without it",
                        names.len()
                    ),
                ),
                // Only a trace that may still grow leaves this undecided.
                None => RuleEvaluation::not_exercised(
                    id,
                    "eventually",
                    format!(
                        "'{tool}' has not appeared and the trace is {} call(s) long, still within the {within}-call deadline",
                        names.len()
                    ),
                ),
            }
        }

        SequenceRule::After {
            trigger,
            then,
            within,
        } => {
            let id = format!("after:{trigger}->{then}@{within}");
            let trig_t = resolve(policy, trigger);
            let then_t = resolve(policy, then);
            // Every trigger arms its own deadline, and a later trigger re-arms after an
            // earlier one was satisfied. An earlier version of this arm took only the first
            // trigger, which reported `held` for `[t, a, t, x, x]` at `within: 1`: the second
            // `t` is never answered and the trace runs two calls past its deadline. One
            // satisfied obligation does not discharge the next one.
            let mut spanned = Vec::new();
            let mut pending: Option<(usize, usize)> = None; // (trigger index, deadline)
            for (idx, name) in names.iter().enumerate() {
                if let Some((trig_idx, deadline)) = pending {
                    if matches_any(name, &then_t) {
                        spanned.push(idx);
                        pending = None;
                    } else if idx > deadline {
                        spanned.push(idx);
                        return RuleEvaluation::violated(
                            id,
                            "after",
                            spanned,
                            format!(
                                "tool '{then}' required within {within} calls after '{trigger}' (triggered at index {trig_idx}) but was not called by index {deadline}"
                            ),
                        );
                    }
                }
                if matches_any(name, &trig_t) {
                    spanned.push(idx);
                    pending = Some((idx, idx + (*within as usize)));
                }
            }
            match pending {
                Some((trig_idx, deadline)) if names.len() > deadline => {
                    RuleEvaluation::violated(
                        id,
                        "after",
                        spanned,
                        format!(
                            "tool '{then}' required within {within} calls after '{trigger}' (triggered at index {trig_idx}) but trace exceeded deadline"
                        ),
                    )
                }
                // A trigger fired but the trace has not yet reached its deadline, so the rule
                // has not been able to decide. Distinct from a trace with no trigger at all.
                // A trigger fired and `then` never followed. On a finished run that is the
                // violation the rule names: no later call can satisfy it. Only a trace that may
                // still grow leaves it undecided.
                Some((trig_idx, _)) if extent == TraceExtent::Complete => RuleEvaluation::violated(
                    id,
                    "after",
                    spanned,
                    format!(
                        "tool '{then}' required within {within} calls after '{trigger}' (triggered at index {trig_idx}) and the run ended without it"
                    ),
                ),
                Some((trig_idx, _)) => RuleEvaluation::not_exercised(
                    id,
                    "after",
                    format!(
                        "'{trigger}' fired at index {trig_idx} and the trace may still satisfy the {within}-call deadline"
                    ),
                ),
                None if spanned.is_empty() => RuleEvaluation::not_exercised(
                    id,
                    "after",
                    format!("'{trigger}' never appeared, so no deadline started"),
                ),
                None => RuleEvaluation::held(id, "after", spanned),
            }
        }

        SequenceRule::Sequence { tools, strict } => {
            let id = format!(
                "sequence{}:{}",
                if *strict { ":strict" } else { "" },
                tools.join(">")
            );
            let targets: Vec<Vec<String>> = tools.iter().map(|t| resolve(policy, t)).collect();
            if targets.is_empty() {
                return RuleEvaluation::not_exercised(
                    id,
                    "sequence",
                    "the rule names no tools".to_string(),
                );
            }
            let mut seq_idx = 0usize;
            let mut spanned = Vec::new();
            for (idx, name) in names.iter().enumerate() {
                if seq_idx < targets.len() && matches_any(name, &targets[seq_idx]) {
                    spanned.push(idx);
                    seq_idx += 1;
                    continue;
                }
                if *strict && !spanned.is_empty() && seq_idx < targets.len() {
                    return RuleEvaluation::violated(
                        id,
                        "sequence",
                        {
                            let mut s = spanned.clone();
                            s.push(idx);
                            s
                        },
                        format!(
                            "strict sequence violated: expected '{}' at index {idx} but found '{name}'",
                            tools[seq_idx]
                        ),
                    );
                }
                if !*strict
                    && seq_idx < targets.len()
                    && targets
                        .iter()
                        .skip(seq_idx + 1)
                        .any(|t| matches_any(name, t))
                {
                    let mut s = spanned.clone();
                    s.push(idx);
                    return RuleEvaluation::violated(
                        id,
                        "sequence",
                        s,
                        format!(
                            "sequence out of order: '{name}' at index {idx} appears before '{}'",
                            tools[seq_idx]
                        ),
                    );
                }
            }
            if !spanned.is_empty() && seq_idx < targets.len() && extent == TraceExtent::Complete {
                // Some members ran and the rest never did. `Held` would say the ordering was
                // satisfied; it was only untested past where the trace stopped.
                return RuleEvaluation::violated(
                    id,
                    "sequence",
                    spanned,
                    format!(
                        "sequence reached '{}' and the run ended before '{}'",
                        tools[seq_idx.saturating_sub(1)],
                        tools[seq_idx]
                    ),
                );
            }
            if spanned.is_empty() {
                // Nothing in the sequence appeared at all; the ordering was never tested.
                RuleEvaluation::not_exercised(
                    id,
                    "sequence",
                    "no tool named by the sequence appeared".to_string(),
                )
            } else {
                RuleEvaluation::held(id, "sequence", spanned)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    /// The demonstration in #2105: three individually-legitimate calls, one finding across
    /// them. Before this module the metric had no `never_after` arm and reported a clean run.
    #[test]
    fn never_after_catches_credential_read_then_egress() {
        let rules = vec![SequenceRule::NeverAfter {
            trigger: "read_credentials".into(),
            forbidden: "http_post".into(),
        }];
        let seq = names(&["list_dir", "read_credentials", "http_post"]);
        let ev = evaluate_rules(&rules, &seq, None, TraceExtent::Complete);

        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].outcome, RuleOutcome::Violated);
        assert_eq!(ev[0].kind, "never_after");
        assert_eq!(ev[0].rule_id, "never_after:read_credentials->http_post");
        // The span is the whole claim: a consumer recomputes from these two indices.
        assert_eq!(ev[0].spanned, vec![1, 2]);
    }

    /// Same rule, egress before the credential read. Held, and it says which call armed it.
    #[test]
    fn never_after_holds_when_order_is_reversed() {
        let rules = vec![SequenceRule::NeverAfter {
            trigger: "read_credentials".into(),
            forbidden: "http_post".into(),
        }];
        let ev = evaluate_rules(
            &rules,
            &names(&["http_post", "read_credentials"]),
            None,
            TraceExtent::Complete,
        );
        assert_eq!(ev[0].outcome, RuleOutcome::Held);
        assert_eq!(ev[0].spanned, vec![1]);
    }

    /// The trigger never fires. Not a pass: nothing was forbidden, so nothing was tested.
    #[test]
    fn never_after_without_its_trigger_is_not_exercised() {
        let rules = vec![SequenceRule::NeverAfter {
            trigger: "read_credentials".into(),
            forbidden: "http_post".into(),
        }];
        let ev = evaluate_rules(
            &rules,
            &names(&["list_dir", "http_post"]),
            None,
            TraceExtent::Complete,
        );
        assert_eq!(ev[0].outcome, RuleOutcome::NotExercised);
        assert!(ev[0].spanned.is_empty());
        assert!(ev[0].reason.as_deref().unwrap().contains("never appeared"));
    }

    /// A `before` rule whose `then` never appears is syntactically perfect and vacuous.
    #[test]
    fn before_without_its_consequent_is_not_exercised() {
        let rules = vec![SequenceRule::Before {
            first: "auth".into(),
            then: "write".into(),
        }];
        let ev = evaluate_rules(
            &rules,
            &names(&["auth", "read"]),
            None,
            TraceExtent::Complete,
        );
        assert_eq!(ev[0].outcome, RuleOutcome::NotExercised);
    }

    /// A blocklist reads every name, so it is exercised even when nothing matches. The two
    /// no-match cases are different and the record keeps them apart.
    #[test]
    fn blocklist_is_exercised_when_nothing_matches() {
        let rules = vec![SequenceRule::Blocklist {
            pattern: "danger".into(),
        }];
        let ev = evaluate_rules(&rules, &names(&["a", "b"]), None, TraceExtent::Complete);
        assert_eq!(ev[0].outcome, RuleOutcome::Held);
        assert_eq!(ev[0].spanned, vec![0, 1]);
    }

    /// Every rule is evaluated. An earlier caller returned on the first violation, which left
    /// later rules with no record at all rather than a record saying they were not reached.
    #[test]
    fn every_rule_gets_a_record_even_after_a_violation() {
        let rules = vec![
            SequenceRule::Require {
                tool: "missing".into(),
            },
            SequenceRule::Blocklist {
                pattern: "danger".into(),
            },
        ];
        let ev = evaluate_rules(&rules, &names(&["a"]), None, TraceExtent::Complete);
        assert_eq!(ev.len(), 2);
        assert_eq!(ev[0].outcome, RuleOutcome::Violated);
        assert_eq!(ev[1].outcome, RuleOutcome::Held);
    }

    #[test]
    fn max_calls_without_the_tool_is_not_exercised() {
        let rules = vec![SequenceRule::MaxCalls {
            tool: "spend".into(),
            max: 2,
        }];
        let ev = evaluate_rules(&rules, &names(&["read"]), None, TraceExtent::Complete);
        assert_eq!(ev[0].outcome, RuleOutcome::NotExercised);
    }

    #[test]
    fn max_calls_violation_spans_every_offending_index() {
        let rules = vec![SequenceRule::MaxCalls {
            tool: "spend".into(),
            max: 1,
        }];
        let ev = evaluate_rules(
            &rules,
            &names(&["spend", "read", "spend"]),
            None,
            TraceExtent::Complete,
        );
        assert_eq!(ev[0].outcome, RuleOutcome::Violated);
        assert_eq!(ev[0].spanned, vec![0, 2]);
    }

    /// Rule ids are stable and operand-derived, so a consumer can key on one across runs
    /// without depending on the rule's position in the policy list.
    #[test]
    fn rule_ids_are_operand_derived() {
        let ev = evaluate_rules(
            &[SequenceRule::Eventually {
                tool: "audit".into(),
                within: 3,
            }],
            &names(&["audit"]),
            None,
            TraceExtent::Complete,
        );
        assert_eq!(ev[0].rule_id, "eventually:audit@3");
    }
    /// Every trigger arms its own deadline. An earlier version took only the first trigger
    /// and reported `held` here: the second `t` is never answered and the trace runs two
    /// calls past its deadline. One satisfied obligation does not discharge the next.
    #[test]
    fn after_re_arms_on_every_trigger() {
        let rules = vec![SequenceRule::After {
            trigger: "t".into(),
            then: "a".into(),
            within: 1,
        }];
        let ev = evaluate_rules(
            &rules,
            &names(&["t", "a", "t", "x", "x"]),
            None,
            TraceExtent::Complete,
        );
        assert_eq!(ev[0].outcome, RuleOutcome::Violated);
    }

    /// The same trace read two ways. A live proxy checking history-so-far cannot yet say the
    /// deadline was missed; a finished run can, because nothing further is coming. Rules and
    /// trace are identical here -- only the extent differs, which is why the caller states it.
    #[test]
    fn after_decides_differently_on_a_finished_run_than_a_partial_one() {
        let rules = vec![SequenceRule::After {
            trigger: "t".into(),
            then: "a".into(),
            within: 5,
        }];
        let trace = names(&["x", "t"]);
        let partial = evaluate_rules(&rules, &trace, None, TraceExtent::Partial);
        assert_eq!(partial[0].outcome, RuleOutcome::NotExercised);
        let complete = evaluate_rules(&rules, &trace, None, TraceExtent::Complete);
        assert_eq!(complete[0].outcome, RuleOutcome::Violated);
        assert!(complete[0]
            .reason
            .as_deref()
            .unwrap()
            .contains("run ended without it"));
    }

    /// A finished run that never called the tool missed its window, however long the window was.
    #[test]
    fn eventually_violates_on_a_finished_run_that_never_called_it() {
        let rules = vec![SequenceRule::Eventually {
            tool: "audit".into(),
            within: 10,
        }];
        let trace = names(&["a", "b", "c", "d"]);
        assert_eq!(
            evaluate_rules(&rules, &trace, None, TraceExtent::Complete)[0].outcome,
            RuleOutcome::Violated
        );
        assert_eq!(
            evaluate_rules(&rules, &trace, None, TraceExtent::Partial)[0].outcome,
            RuleOutcome::NotExercised
        );
    }

    /// Half a sequence is not a held sequence. `Held` would say the ordering was satisfied.
    #[test]
    fn truncated_sequence_is_not_held_on_a_finished_run() {
        let rules = vec![SequenceRule::Sequence {
            tools: vec!["auth".into(), "validate".into(), "commit".into()],
            strict: true,
        }];
        let ev = evaluate_rules(&rules, &names(&["auth"]), None, TraceExtent::Complete);
        assert_eq!(ev[0].outcome, RuleOutcome::Violated);
        assert!(ev[0]
            .reason
            .as_deref()
            .unwrap()
            .contains("run ended before"));
    }

    /// Aliases are resolved when a policy is supplied. Without one an aliased rule reads the
    /// literal name and misses the call it means, so the caller must pass its policy through.
    #[test]
    fn aliases_are_resolved_when_a_policy_is_supplied() {
        let policy: Policy = serde_yaml::from_str(
            "version: \"1\"\naliases:\n  Egress: [http_post, curl]\nsequences: []\n",
        )
        .expect("policy parses");
        let rules = vec![SequenceRule::NeverAfter {
            trigger: "read_credentials".into(),
            forbidden: "Egress".into(),
        }];
        let trace = names(&["read_credentials", "curl"]);
        let with = evaluate_rules(&rules, &trace, Some(&policy), TraceExtent::Complete);
        assert_eq!(
            with[0].outcome,
            RuleOutcome::Violated,
            "curl is an Egress member"
        );
        let without = evaluate_rules(&rules, &trace, None, TraceExtent::Complete);
        assert_eq!(
            without[0].outcome,
            RuleOutcome::Held,
            "the literal name never appears"
        );
    }

    /// The labels reach `details` and every projection over it, so they are interface.
    #[test]
    fn outcome_labels_are_pinned() {
        assert_eq!(RuleOutcome::Held.label(), "held");
        assert_eq!(RuleOutcome::Violated.label(), "violated");
        assert_eq!(RuleOutcome::NotExercised.label(), "not_exercised");
    }

    /// Rule ids are operand-derived for every kind, not only the two spot-checked elsewhere.
    #[test]
    fn every_rule_id_carries_its_operands() {
        let cases: Vec<(SequenceRule, &str)> = vec![
            (SequenceRule::Require { tool: "t".into() }, "require:t"),
            (
                SequenceRule::Blocklist {
                    pattern: "p".into(),
                },
                "blocklist:p",
            ),
            (
                SequenceRule::Before {
                    first: "a".into(),
                    then: "b".into(),
                },
                "before:a->b",
            ),
            (
                SequenceRule::MaxCalls {
                    tool: "t".into(),
                    max: 2,
                },
                "max_calls:t<=2",
            ),
            (
                SequenceRule::After {
                    trigger: "a".into(),
                    then: "b".into(),
                    within: 3,
                },
                "after:a->b@3",
            ),
            (
                SequenceRule::Sequence {
                    tools: vec!["a".into(), "b".into()],
                    strict: true,
                },
                "sequence:strict:a>b",
            ),
        ];
        for (rule, want) in cases {
            let ev = evaluate_rules(&[rule], &names(&["z"]), None, TraceExtent::Complete);
            assert_eq!(ev[0].rule_id, want);
        }
    }
}
