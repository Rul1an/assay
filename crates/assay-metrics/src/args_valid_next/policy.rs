use crate::policy_warning::should_emit_deprecated_policy_warning;
use serde_json::Value;
use std::{borrow::Cow, path::Path};

pub(super) fn policy_input<'a>(
    inline: Option<&'a Value>,
    path: Option<&str>,
    metric: &str,
) -> anyhow::Result<Cow<'a, Value>> {
    if let Some(schema) = inline {
        return Ok(Cow::Borrowed(schema));
    }
    let path = path
        .ok_or_else(|| anyhow::anyhow!("config error: args_valid requires `policy` or `schema`"))?;
    if should_emit_deprecated_policy_warning(metric, path) {
        eprintln!(
            "WARN: Deprecated policy file '{}' detected. Please migrate to inline usage.",
            path
        );
        eprintln!("      To suppress this, set MCP_CONFIG_LEGACY=1 or run 'assay migrate'.");
    }
    assay_core::model::read_args_policy_file(Path::new(path))
        .map(Cow::Owned)
        .map_err(|e| anyhow::anyhow!("config error: {e}"))
}
