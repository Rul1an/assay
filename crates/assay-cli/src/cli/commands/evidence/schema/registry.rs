use super::reports::{SchemaKind, SchemaMetadata};
use anyhow::{bail, Context, Result};
use serde_json::Value;

macro_rules! include_packaged_schema {
    ($relative_path:literal) => {
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/receipt-schemas/",
            $relative_path
        ))
    };
}

const PROMPTFOO_RECEIPT_SCHEMA: &str =
    include_packaged_schema!("receipts/promptfoo.assertion-component.v1.schema.json");
const OPENFEATURE_RECEIPT_SCHEMA: &str =
    include_packaged_schema!("receipts/openfeature.evaluation-details.v1.schema.json");
const CYCLONEDX_RECEIPT_SCHEMA: &str =
    include_packaged_schema!("receipts/cyclonedx.mlbom-model-component.v1.schema.json");
const MASTRA_RECEIPT_SCHEMA: &str =
    include_packaged_schema!("receipts/mastra.score-event.v1.schema.json");
const PYDANTIC_RECEIPT_SCHEMA: &str =
    include_packaged_schema!("receipts/pydantic.case-result.v1.schema.json");
const LIVEKIT_RECEIPT_SCHEMA: &str =
    include_packaged_schema!("receipts/livekit.tool-action.v1.schema.json");
const PROMPTFOO_INPUT_SCHEMA: &str =
    include_packaged_schema!("inputs/promptfoo-cli-jsonl-component-result.v1.schema.json");
const OPENFEATURE_INPUT_SCHEMA: &str =
    include_packaged_schema!("inputs/openfeature-evaluation-details-export.v1.schema.json");
const CYCLONEDX_INPUT_SCHEMA: &str =
    include_packaged_schema!("inputs/cyclonedx-mlbom-model-component-input.v1.schema.json");
const MASTRA_INPUT_SCHEMA: &str =
    include_packaged_schema!("inputs/mastra-score-event-export.v1.schema.json");
const PYDANTIC_INPUT_SCHEMA: &str =
    include_packaged_schema!("inputs/pydantic-case-result-export.v1.schema.json");
const LIVEKIT_INPUT_SCHEMA: &str =
    include_packaged_schema!("inputs/livekit-function-tools-executed-export.v1.schema.json");

#[derive(Clone, Copy)]
pub(super) struct SchemaDescriptor {
    pub(super) name: &'static str,
    aliases: &'static [&'static str],
    pub(super) kind: SchemaKind,
    status: &'static str,
    family: &'static str,
    source_path: &'static str,
    description: &'static str,
    trust_basis_claim: Option<&'static str>,
    importer_only: bool,
    schema_json: &'static str,
}

impl SchemaDescriptor {
    pub(super) fn json_schema_value(&self) -> Result<Value> {
        serde_json::from_str(self.schema_json)
            .with_context(|| format!("failed to parse embedded schema {}", self.name))
    }

    fn json_schema_id(&self) -> Result<String> {
        Ok(self
            .json_schema_value()?
            .get("$id")
            .and_then(Value::as_str)
            .unwrap_or(self.name)
            .to_string())
    }

    fn matches(&self, needle: &str) -> Result<bool> {
        if self.name == needle || self.source_path == needle || self.aliases.contains(&needle) {
            return Ok(true);
        }
        Ok(self.json_schema_id()? == needle)
    }
}

pub(super) const SCHEMAS: &[SchemaDescriptor] = &[
    SchemaDescriptor {
        name: "promptfoo.assertion-component.v1",
        aliases: &["assay.receipt.promptfoo.assertion-component.v1"],
        kind: SchemaKind::Receipt,
        status: "stable",
        family: "external_eval_receipts",
        source_path: "docs/reference/receipt-schemas/receipts/promptfoo.assertion-component.v1.schema.json",
        description: "Assay receipt payload for one selected Promptfoo assertion component result.",
        trust_basis_claim: Some("external_eval_receipt_boundary_visible"),
        importer_only: false,
        schema_json: PROMPTFOO_RECEIPT_SCHEMA,
    },
    SchemaDescriptor {
        name: "openfeature.evaluation-details.v1",
        aliases: &["assay.receipt.openfeature.evaluation_details.v1"],
        kind: SchemaKind::Receipt,
        status: "stable",
        family: "external_decision_receipts",
        source_path: "docs/reference/receipt-schemas/receipts/openfeature.evaluation-details.v1.schema.json",
        description: "Assay receipt payload for one bounded OpenFeature boolean EvaluationDetails decision.",
        trust_basis_claim: Some("external_decision_receipt_boundary_visible"),
        importer_only: false,
        schema_json: OPENFEATURE_RECEIPT_SCHEMA,
    },
    SchemaDescriptor {
        name: "cyclonedx.mlbom-model-component.v1",
        aliases: &["assay.receipt.cyclonedx.mlbom-model-component.v1"],
        kind: SchemaKind::Receipt,
        status: "stable",
        family: "external_inventory_receipts",
        source_path: "docs/reference/receipt-schemas/receipts/cyclonedx.mlbom-model-component.v1.schema.json",
        description: "Assay receipt payload for one selected CycloneDX ML-BOM machine-learning-model component.",
        trust_basis_claim: Some("external_inventory_receipt_boundary_visible"),
        importer_only: false,
        schema_json: CYCLONEDX_RECEIPT_SCHEMA,
    },
    SchemaDescriptor {
        name: "mastra.score-event.v1",
        aliases: &["assay.receipt.mastra.score_event.v1"],
        kind: SchemaKind::Receipt,
        status: "experimental",
        family: "score_receipts",
        source_path: "docs/reference/receipt-schemas/receipts/mastra.score-event.v1.schema.json",
        description: "Assay receipt payload for one bounded Mastra ScoreEvent-derived score artifact.",
        trust_basis_claim: None,
        importer_only: true,
        schema_json: MASTRA_RECEIPT_SCHEMA,
    },
    SchemaDescriptor {
        name: "pydantic.case-result.v1",
        aliases: &["assay.receipt.pydantic.case_result.v1"],
        kind: SchemaKind::Receipt,
        status: "experimental",
        family: "case_result_receipts",
        source_path: "docs/reference/receipt-schemas/receipts/pydantic.case-result.v1.schema.json",
        description: "Assay receipt payload for one bounded Pydantic Evals reduced case-result artifact.",
        trust_basis_claim: None,
        importer_only: true,
        schema_json: PYDANTIC_RECEIPT_SCHEMA,
    },
    SchemaDescriptor {
        name: "livekit.tool-action.v1",
        aliases: &["assay.receipt.livekit.tool-action.v1"],
        kind: SchemaKind::Receipt,
        status: "experimental",
        family: "acted_receipts_candidate",
        source_path: "docs/reference/receipt-schemas/receipts/livekit.tool-action.v1.schema.json",
        description: "Assay receipt payload for one bounded LiveKit function tool action artifact.",
        trust_basis_claim: None,
        importer_only: true,
        schema_json: LIVEKIT_RECEIPT_SCHEMA,
    },
    SchemaDescriptor {
        name: "promptfoo-cli-jsonl-component-result.v1",
        aliases: &[],
        kind: SchemaKind::Input,
        status: "stable",
        family: "external_eval_receipts",
        source_path: "docs/reference/receipt-schemas/inputs/promptfoo-cli-jsonl-component-result.v1.schema.json",
        description: "Supported Promptfoo CLI JSONL row shape containing assertion component results.",
        trust_basis_claim: Some("external_eval_receipt_boundary_visible"),
        importer_only: false,
        schema_json: PROMPTFOO_INPUT_SCHEMA,
    },
    SchemaDescriptor {
        name: "openfeature-evaluation-details-export.v1",
        aliases: &[],
        kind: SchemaKind::Input,
        status: "stable",
        family: "external_decision_receipts",
        source_path: "docs/reference/receipt-schemas/inputs/openfeature-evaluation-details-export.v1.schema.json",
        description: "Supported reduced OpenFeature boolean EvaluationDetails JSONL row shape.",
        trust_basis_claim: Some("external_decision_receipt_boundary_visible"),
        importer_only: false,
        schema_json: OPENFEATURE_INPUT_SCHEMA,
    },
    SchemaDescriptor {
        name: "cyclonedx-mlbom-model-component-input.v1",
        aliases: &[],
        kind: SchemaKind::Input,
        status: "stable",
        family: "external_inventory_receipts",
        source_path: "docs/reference/receipt-schemas/inputs/cyclonedx-mlbom-model-component-input.v1.schema.json",
        description: "Supported CycloneDX ML-BOM input shape for selecting one machine-learning-model component.",
        trust_basis_claim: Some("external_inventory_receipt_boundary_visible"),
        importer_only: false,
        schema_json: CYCLONEDX_INPUT_SCHEMA,
    },
    SchemaDescriptor {
        name: "mastra-score-event-export.v1",
        aliases: &[],
        kind: SchemaKind::Input,
        status: "experimental",
        family: "score_receipts",
        source_path: "docs/reference/receipt-schemas/inputs/mastra-score-event-export.v1.schema.json",
        description: "Supported reduced Mastra ScoreEvent JSONL row shape.",
        trust_basis_claim: None,
        importer_only: true,
        schema_json: MASTRA_INPUT_SCHEMA,
    },
    SchemaDescriptor {
        name: "pydantic-case-result-export.v1",
        aliases: &[],
        kind: SchemaKind::Input,
        status: "experimental",
        family: "case_result_receipts",
        source_path: "docs/reference/receipt-schemas/inputs/pydantic-case-result-export.v1.schema.json",
        description: "Supported reduced Pydantic Evals case-result JSONL row shape.",
        trust_basis_claim: None,
        importer_only: true,
        schema_json: PYDANTIC_INPUT_SCHEMA,
    },
    SchemaDescriptor {
        name: "livekit-function-tools-executed-export.v1",
        aliases: &[],
        kind: SchemaKind::Input,
        status: "experimental",
        family: "acted_receipts_candidate",
        source_path: "docs/reference/receipt-schemas/inputs/livekit-function-tools-executed-export.v1.schema.json",
        description: "Supported reduced LiveKit FunctionToolsExecutedEvent artifact shape.",
        trust_basis_claim: None,
        importer_only: true,
        schema_json: LIVEKIT_INPUT_SCHEMA,
    },
];

pub(super) fn find_schema(raw: &str) -> Result<&'static SchemaDescriptor> {
    let needle = raw.trim();
    if needle.is_empty() {
        bail!("schema name is empty");
    }

    for descriptor in SCHEMAS {
        if descriptor.matches(needle)? {
            return Ok(descriptor);
        }
    }

    bail!("unknown schema {needle:?}; run `assay evidence schema list` for supported schemas");
}

pub(super) fn schema_metadata(descriptor: &SchemaDescriptor) -> Result<SchemaMetadata> {
    Ok(SchemaMetadata {
        name: descriptor.name.to_string(),
        kind: descriptor.kind,
        status: descriptor.status.to_string(),
        family: descriptor.family.to_string(),
        json_schema_id: descriptor.json_schema_id()?,
        source_path: descriptor.source_path.to_string(),
        description: descriptor.description.to_string(),
        trust_basis_claim: descriptor.trust_basis_claim.map(str::to_string),
        importer_only: descriptor.importer_only,
        aliases: descriptor
            .aliases
            .iter()
            .map(|alias| (*alias).to_string())
            .collect(),
    })
}
