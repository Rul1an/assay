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
pub struct Context<'a> {
    pub suite: &'a str,
    pub model: &'a str,
    pub test_id: &'a str,
    pub prompt: &'a str,
    pub context: Option<&'a [String]>,
    pub expected_canonical: &'a str,
    pub policy_hash: Option<&'a str>,
    pub metric_versions: &'a [(&'a str, &'a str)],
}

/// Computes a deterministic fingerprint for a test case execution context.
///
/// Inputs are canonicalized (sorted map keys via serde_json where applicable)
/// to ensure stable hashing.
pub fn compute(ctx: Context<'_>) -> Fingerprint {
    let mut parts = Vec::new();

    // Core Identity
    parts.push(format!("suite={}", ctx.suite));
    parts.push(format!("model={}", ctx.model));
    parts.push(format!("test_id={}", ctx.test_id));

    // Input (Exact text match required)
    parts.push(format!("prompt={}", ctx.prompt));
    if let Some(c) = ctx.context {
        parts.push(format!("context={}", c.join("\n")));
    } else {
        parts.push("context=".to_string());
    }

    // Expected (Outcome logic)
    parts.push(format!("expected={}", ctx.expected_canonical));
    if let Some(ph) = ctx.policy_hash {
        parts.push(format!("policy_hash={}", ph));
    }

    // Metric Logic Versions (Code change invalidation)
    let mut mv = ctx.metric_versions.to_vec();
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
