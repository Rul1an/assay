use serde::Serialize;

#[derive(Debug, Serialize)]
pub(super) struct PairingReport {
    pub(super) schema: &'static str,
    pub(super) ok: bool,
    pub(super) canonicalization: &'static str,
    pub(super) verification_scope: VerificationScope,
    pub(super) binding: BindingReport,
    pub(super) attestation: Option<AttestationReport>,
    pub(super) decision: DecisionReport,
    pub(super) outcome: Option<OutcomeReport>,
    pub(super) checks: Vec<CheckReport>,
    pub(super) claims_not_made: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
pub(super) struct VerificationScope {
    pub(super) role: &'static str,
    pub(super) note: &'static str,
}

#[derive(Debug, Serialize)]
pub(super) struct AttestationReport {
    pub(super) digest: String,
    pub(super) nonce: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct BindingReport {
    pub(super) mode: &'static str,
    pub(super) digest: String,
    pub(super) digest_source: &'static str,
    pub(super) projection: Option<&'static str>,
    pub(super) nonce: Option<String>,
    pub(super) nonce_source: &'static str,
}

#[derive(Debug, Serialize)]
pub(super) struct DecisionReport {
    pub(super) decision: Option<String>,
    pub(super) decided_at: Option<String>,
    pub(super) backlink: BackLinkReport,
    pub(super) signature_present: bool,
}

#[derive(Debug, Serialize)]
pub(super) struct OutcomeReport {
    pub(super) status: Option<String>,
    pub(super) completed_at: Option<String>,
    pub(super) decision_digest: Option<String>,
    pub(super) backlink: BackLinkReport,
    pub(super) signature_present: bool,
}

#[derive(Debug, Serialize)]
pub(super) struct BackLinkReport {
    pub(super) attestation_digest: Option<String>,
    pub(super) attestation_nonce: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct CheckReport {
    pub(super) id: &'static str,
    pub(super) ok: bool,
    pub(super) detail: String,
}

pub(super) fn print_table_report(report: &PairingReport) {
    println!("MCP Execution Record Pairing Report");
    println!("===================================");
    println!("OK:               {}", if report.ok { "yes" } else { "no" });
    println!("Role:             {}", report.verification_scope.role);
    println!("Binding:          {}", report.binding.mode);
    println!("Binding digest:   {}", report.binding.digest);
    println!(
        "Binding nonce:    {}",
        report.binding.nonce.as_deref().unwrap_or("-")
    );
    println!(
        "Decision:         {}",
        report.decision.decision.as_deref().unwrap_or("-")
    );
    if let Some(outcome) = &report.outcome {
        println!(
            "Outcome:          {}",
            outcome.status.as_deref().unwrap_or("-")
        );
    } else {
        println!("Outcome:          -");
    }
    println!();
    for check in &report.checks {
        println!(
            "{:<36} {:<4} {}",
            check.id,
            if check.ok { "ok" } else { "fail" },
            check.detail
        );
    }
    println!();
    println!("Claims not made: {}", report.claims_not_made.join(", "));
}
