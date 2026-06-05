use crate::cli::commands::evidence::cyclonedx_mlbom_model::constants::{
    MAX_BOUNDARY_STRING_CHARS, MAX_REF_COUNT, RECEIPT_SCHEMA, REDUCER_VERSION, SOURCE_SURFACE,
    SOURCE_SYSTEM,
};
use crate::cli::commands::evidence::cyclonedx_mlbom_model::validate::{
    bounded_string, optional_bounded_string, validate_bounded_string,
};
use anyhow::{bail, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{json, Map, Value};

pub(super) fn reduce_model_component(
    bom: &Value,
    bom_ref: Option<&str>,
    source_artifact_ref: &str,
    source_artifact_digest: &str,
    import_time: DateTime<Utc>,
) -> Result<Value> {
    let bom_obj = bom
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("CycloneDX input must be a JSON object"))?;
    string_equals(bom_obj, "bomFormat", "CycloneDX")?;

    let components = bom_obj
        .get("components")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("CycloneDX input must contain components[]"))?;

    let candidates = components
        .iter()
        .filter(|component| {
            component
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|value| value == "machine-learning-model")
        })
        .collect::<Vec<_>>();

    if candidates.is_empty() {
        bail!("CycloneDX input contains no components[] entries with type machine-learning-model");
    }

    let selected = select_component(&candidates, bom_ref)?;
    let selected_obj = selected
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("selected CycloneDX component must be an object"))?;

    let bom_ref = bounded_string(
        selected_obj.get("bom-ref"),
        "component.bom-ref",
        MAX_BOUNDARY_STRING_CHARS,
    )?;
    let name = bounded_string(
        selected_obj.get("name"),
        "component.name",
        MAX_BOUNDARY_STRING_CHARS,
    )?;

    let mut model_component = Map::new();
    model_component.insert("bom_ref".to_string(), Value::String(bom_ref));
    model_component.insert("name".to_string(), Value::String(name));
    insert_optional_string(
        &mut model_component,
        selected_obj,
        "version",
        "component.version",
    )?;
    insert_optional_string(
        &mut model_component,
        selected_obj,
        "publisher",
        "component.publisher",
    )?;
    insert_optional_string(&mut model_component, selected_obj, "purl", "component.purl")?;

    let dataset_refs = dataset_refs(selected_obj)?;
    if !dataset_refs.is_empty() {
        model_component.insert("dataset_refs".to_string(), strings_value(dataset_refs));
    }
    let model_card_refs = model_card_refs(selected_obj)?;
    if !model_card_refs.is_empty() {
        model_component.insert(
            "model_card_refs".to_string(),
            strings_value(model_card_refs),
        );
    }

    Ok(json!({
        "schema": RECEIPT_SCHEMA,
        "source_system": SOURCE_SYSTEM,
        "source_surface": SOURCE_SURFACE,
        "source_artifact_ref": source_artifact_ref,
        "source_artifact_digest": source_artifact_digest,
        "reducer_version": REDUCER_VERSION,
        "imported_at": import_time.to_rfc3339_opts(SecondsFormat::Secs, true),
        "model_component": Value::Object(model_component),
    }))
}

fn select_component<'a>(candidates: &'a [&'a Value], bom_ref: Option<&str>) -> Result<&'a Value> {
    if let Some(bom_ref) = bom_ref {
        let matches = candidates
            .iter()
            .copied()
            .filter(|component| {
                component
                    .get("bom-ref")
                    .and_then(Value::as_str)
                    .is_some_and(|actual| actual == bom_ref)
            })
            .collect::<Vec<_>>();
        return match matches.as_slice() {
            [selected] => Ok(*selected),
            [] => bail!(
                "--bom-ref {bom_ref:?} did not match a components[] machine-learning-model entry"
            ),
            _ => bail!("--bom-ref {bom_ref:?} matched multiple model components"),
        };
    }

    match candidates {
        [selected] => Ok(*selected),
        _ => bail!(
            "CycloneDX input contains multiple machine-learning-model components; pass --bom-ref to select one"
        ),
    }
}

fn dataset_refs(component: &Map<String, Value>) -> Result<Vec<String>> {
    let mut refs = Vec::new();
    let Some(datasets) = component
        .get("modelCard")
        .and_then(|model_card| model_card.get("modelParameters"))
        .and_then(|model_parameters| model_parameters.get("datasets"))
        .and_then(Value::as_array)
    else {
        return Ok(refs);
    };

    for (index, dataset) in datasets.iter().enumerate() {
        let Some(reference) = dataset.get("ref").and_then(Value::as_str) else {
            continue;
        };
        refs.push(validate_bounded_string(
            reference,
            &format!("modelCard.modelParameters.datasets[{index}].ref"),
            MAX_BOUNDARY_STRING_CHARS,
        )?);
        if refs.len() > MAX_REF_COUNT {
            bail!("modelCard.modelParameters.datasets has more than {MAX_REF_COUNT} refs");
        }
    }

    Ok(refs)
}

fn model_card_refs(component: &Map<String, Value>) -> Result<Vec<String>> {
    let mut refs = Vec::new();
    if let Some(reference) = component
        .get("modelCard")
        .and_then(|model_card| model_card.get("bom-ref"))
        .and_then(Value::as_str)
    {
        refs.push(validate_bounded_string(
            reference,
            "modelCard.bom-ref",
            MAX_BOUNDARY_STRING_CHARS,
        )?);
    }
    Ok(refs)
}

fn insert_optional_string(
    output: &mut Map<String, Value>,
    component: &Map<String, Value>,
    key: &str,
    field_name: &str,
) -> Result<()> {
    if let Some(value) =
        optional_bounded_string(component.get(key), field_name, MAX_BOUNDARY_STRING_CHARS)?
    {
        output.insert(key.to_string(), Value::String(value));
    }
    Ok(())
}

fn strings_value(values: Vec<String>) -> Value {
    Value::Array(values.into_iter().map(Value::String).collect())
}

fn string_equals(record: &Map<String, Value>, key: &str, expected: &str) -> Result<()> {
    match record.get(key).and_then(Value::as_str) {
        Some(actual) if actual == expected => Ok(()),
        Some(actual) => bail!("{key} must be {expected:?}, got {actual:?}"),
        None => bail!("missing string {key}"),
    }
}
