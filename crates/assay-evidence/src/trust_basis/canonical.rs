use super::TrustBasis;
use anyhow::Result;
use serde::Serialize;

pub(super) fn to_canonical_json_bytes(trust_basis: &TrustBasis) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"  ");
    let mut serializer = serde_json::Serializer::with_formatter(&mut output, formatter);
    trust_basis.serialize(&mut serializer)?;
    output.push(b'\n');
    Ok(output)
}
