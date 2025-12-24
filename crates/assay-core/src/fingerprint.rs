use sha2::{Digest, Sha256};

#[derive(Debug, Clone)]
pub struct Fingerprint {
    pub hex: String,
    pub components: Vec<String>,
}

pub fn sha256_hex(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    hex::encode(h.finalize())
}

/// Computes a deterministic fingerprint for a test case execution context.
///
/// Inputs are canonicalized (sorted map keys via serde_json where applicable)
/// to ensure stable hashing.
pub fn compute(
    suite: &str,
    model: &str,
    test_id: &str,
    prompt: &str,
    context: Option<&[String]>,
    expected_canonical: &str,
    metric_versions: &[(&str, &str)],
) -> Fingerprint {
    let mut parts = Vec::new();

    // Core Identity
    parts.push(format!("suite={suite}"));
    parts.push(format!("model={model}"));
    parts.push(format!("test_id={test_id}"));

    // Input (Exact text match required)
    parts.push(format!("prompt={}", prompt));
    if let Some(ctx) = context {
        parts.push(format!("context={}", ctx.join("\n")));
    } else {
        parts.push("context=".to_string());
    }

    // Expected (Outcome logic)
    parts.push(format!("expected={expected_canonical}"));

    // Metric Logic Versions (Code change invalidation)
    let mut mv = metric_versions.to_vec();
    mv.sort_by_key(|(name, _)| *name);
    let mv_str = mv
        .into_iter()
        .map(|(n, v)| format!("{n}:{v}"))
        .collect::<Vec<_>>()
        .join(",");
    parts.push(format!("metrics={}", mv_str));

    // Assay Version (Invalidate all on update)
    // Optional: We can include this or rely on metric_versions for granular invalidation.
    // Putting it here ensures safety on logic changes in runner itself.
    parts.push(format!("assay_version={}", env!("CARGO_PKG_VERSION")));

    let raw = parts.join("\n");
    let hex = sha256_hex(&raw);

    Fingerprint {
        hex,
        components: parts,
    }
}
