use super::schema::TraceEntryV1;
use crate::judge::JudgeService;
use crate::model::{EvalConfig, Expected, TestCase};
use crate::providers::embedder::Embedder;
use anyhow::Context;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::sync::Arc;

// Enrichment adds or updates members. Copy the producer's spelling of the
// surrounding object and every untouched value, including duplicate members.
fn write_metadata_changes(
    out: &mut impl Write,
    raw: &str,
    old: &serde_json::Value,
    new: &serde_json::Value,
) -> anyhow::Result<()> {
    use serde_json::value::RawValue;
    let (Some(old), Some(new)) = (old.as_object(), new.as_object()) else {
        serde_json::to_writer(out, new)?;
        return Ok(());
    };
    let members: std::collections::HashMap<String, &RawValue> = serde_json::from_str(raw)?;
    let mut updates = Vec::new();
    let mut additions = Vec::new();
    for (key, value) in new {
        if old.get(key) == Some(value) {
            continue;
        }
        if let (Some(prev), Some(member)) = (old.get(key), members.get(key)) {
            let text = member.get();
            let offset = text.as_ptr() as usize - raw.as_ptr() as usize;
            updates.push((offset, text, prev, value));
        } else {
            additions.push((key, value));
        }
    }
    updates.sort_unstable_by_key(|update| update.0);
    let mut pos = 0;
    for (offset, text, prev, value) in updates {
        out.write_all(&raw.as_bytes()[pos..offset])?;
        write_metadata_changes(out, text, prev, value)?;
        pos = offset + text.len();
    }
    // RawValue excludes surrounding whitespace. Its last byte closes the object.
    let end = raw.len() - 1;
    out.write_all(&raw.as_bytes()[pos..end])?;
    for (i, (key, value)) in additions.into_iter().enumerate() {
        if i != 0 || !members.is_empty() {
            out.write_all(b",")?;
        }
        serde_json::to_writer(&mut *out, key)?;
        out.write_all(b":")?;
        serde_json::to_writer(&mut *out, value)?;
    }
    out.write_all(b"}")?;
    Ok(())
}

// Keep producer JSON outside the metadata slot verbatim. Parsing an unknown
// number into Value could round it or reject an exponent the producer used.
fn write_object_metadata(
    out: &mut impl Write,
    line: &str,
    old_meta: Option<&serde_json::Value>,
    meta: &serde_json::Value,
) -> anyhow::Result<()> {
    use serde_json::value::RawValue;

    #[derive(serde::Deserialize)]
    struct Metadata<'a> {
        #[serde(borrow, default, deserialize_with = "present")]
        meta: Option<&'a RawValue>,
    }
    // Distinguish an explicit JSON null from a missing member.
    fn present<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<&'de RawValue>, D::Error> {
        serde::Deserialize::deserialize(d).map(Some)
    }

    if let Some(raw) = serde_json::from_str::<Metadata<'_>>(line)?.meta {
        let borrowed = raw.get();
        let start = borrowed.as_ptr() as usize - line.as_ptr() as usize;
        let (before, rest) = line.split_at(start);
        write!(out, "{before}")?;
        if let Some(old) = old_meta {
            write_metadata_changes(out, borrowed, old, meta)?;
        } else {
            serde_json::to_writer(&mut *out, meta)?;
        }
        write!(out, "{}", &rest[borrowed.len()..])?;
    } else {
        // Both a legacy record and an episode start have required fields.
        let before = line
            .trim_end()
            .strip_suffix('}')
            .context("expected trace object")?;
        write!(out, "{before},\"meta\":")?;
        serde_json::to_writer(&mut *out, meta)?;
        out.write_all(&line.as_bytes()[before.len()..])?;
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Enrichment {
    Embeddings,
    Judge,
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "only judge metrics use these rubric slots"
)]
fn judge_rubric(expected: &Expected) -> Option<(&'static str, Option<&str>)> {
    match expected {
        Expected::Faithfulness { rubric_version, .. } => {
            Some(("faithfulness", rubric_version.as_deref()))
        }
        Expected::Relevance { rubric_version, .. } => {
            Some(("relevance", rubric_version.as_deref()))
        }
        Expected::JudgeCriteria { .. } => Some(("custom", None)),
        _ => None,
    }
}

fn has_embeddings(meta: &serde_json::Value) -> bool {
    meta.pointer("/assay/embeddings/response").is_some()
        && meta.pointer("/assay/embeddings/reference").is_some()
}

impl Enrichment {
    // One metadata slot can store only one reference or one rubric version.
    fn slot(self, expected: &Expected) -> Option<(&'static str, &str, Option<&serde_json::Value>)> {
        match self {
            Self::Embeddings => {
                if let Expected::SemanticSimilarityTo {
                    semantic_similarity_to,
                    ..
                } = expected
                {
                    Some(("embeddings", semantic_similarity_to, None))
                } else {
                    None
                }
            }
            Self::Judge => {
                let (slot, version) = judge_rubric(expected)?;
                let criteria = if let Expected::JudgeCriteria { judge_criteria } = expected {
                    Some(judge_criteria)
                } else {
                    None
                };
                Some((slot, version.unwrap_or("v1"), criteria))
            }
        }
    }
}

const MAX_EPISODE_BYTES: u64 = 16 * 1024 * 1024;

struct Replacement {
    bytes: Vec<u8>,
    limit: usize,
}

impl Write for Replacement {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if buf.len() > self.limit - self.bytes.len() {
            return Err(std::io::Error::other(format!(
                "episode output exceeds {MAX_EPISODE_BYTES} bytes"
            )));
        }
        self.bytes.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

async fn enrich_episodes(
    source: &str,
    input: &File,
    output: &Path,
    config: &EvalConfig,
    kind: Enrichment,
    enrich: impl AsyncFn(&TestCase, &str, &str, &mut serde_json::Value) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    use crate::providers::trace::precompute_episodes;
    use std::collections::BTreeMap;
    let episodes = precompute_episodes(source)?;
    let mut requests: std::collections::HashMap<_, _> = episodes
        .values()
        .map(|(prompt, _)| (prompt.as_str(), BTreeMap::new()))
        .collect();
    for tc in &config.tests {
        let Some(slots) = requests.get_mut(tc.input.prompt.as_str()) else {
            continue;
        };
        let Some((slot, version, criteria)) = kind.slot(&tc.expected) else {
            continue;
        };
        let request = (version, criteria);
        if let Some((previous, _)) = slots.get(slot) {
            anyhow::ensure!(
                *previous == request,
                "ambiguous precompute {slot} requests for test {}",
                tc.id
            );
        } else {
            slots.insert(slot, (request, tc));
        }
    }

    let mut changes = Vec::new();
    let mut offset = 0;
    let mut output_size = source.len();
    for (index, line) in source.split_inclusive('\n').enumerate() {
        let start_offset = offset;
        offset += line.len();
        let Some((prompt, response)) = episodes.get(&index) else {
            continue;
        };
        let slots = &requests[prompt.as_str()];
        if slots.is_empty() {
            continue;
        }
        let start: super::schema::EpisodeStart = serde_json::from_str(line)?;
        let old_meta = start.meta;
        let mut meta = old_meta.clone();
        if meta.is_null() {
            meta = serde_json::json!({});
        }
        anyhow::ensure!(
            meta.is_object(),
            "episode metadata on line {} is not an object",
            index + 1
        );
        for key in [
            "/assay",
            match kind {
                Enrichment::Judge => "/assay/judge",
                Enrichment::Embeddings => "/assay/embeddings",
            },
        ] {
            anyhow::ensure!(
                meta.pointer(key).is_none_or(|value| value.is_object()),
                "episode metadata {key} on line {} is not an object",
                index + 1
            );
        }
        for (_, tc) in slots.values() {
            if matches!(kind, Enrichment::Embeddings) && !has_embeddings(&meta) {
                eprintln!("Embedding entry {}...", tc.id);
            }
            enrich(tc, prompt, response, &mut meta).await?;
        }
        if old_meta != meta {
            let unchanged = output_size - line.len();
            let mut replacement = Replacement {
                bytes: Vec::new(),
                limit: MAX_EPISODE_BYTES as usize - unchanged,
            };
            write_object_metadata(&mut replacement, line, Some(&old_meta), &meta)?;
            output_size = unchanged + replacement.bytes.len();
            changes.push((start_offset, offset, replacement.bytes));
        }
    }
    let mut out = assay_common::file_output::open_preserving_inputs(output, &[input])
        .context("failed to create output trace file")?;
    let bytes = source.as_bytes();
    let mut written = 0;
    for (start, end, replacement) in changes {
        out.write_all(&bytes[written..start])?;
        out.write_all(&replacement)?;
        written = end;
    }
    out.write_all(&bytes[written..])?;
    Ok(())
}

async fn enrich_rows(
    input: &Path,
    output: &Path,
    config: &EvalConfig,
    kind: Enrichment,
    enrich: impl AsyncFn(&TestCase, &str, &str, &mut serde_json::Value) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let file = File::open(input).context("failed to open input trace file")?;
    assay_common::file_output::precheck_preserving_inputs(output, &[&file])
        .context("failed to create output trace file")?;
    let mut reader = BufReader::new(&file);
    let mut prefix = Vec::new();
    let mut probe = (&mut reader).take(MAX_EPISODE_BYTES + 1);
    loop {
        let start = prefix.len();
        if probe.read_until(b'\n', &mut prefix)? == 0 || prefix.len() as u64 > MAX_EPISODE_BYTES {
            break;
        }
        let first = &prefix[start..];
        if first.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        #[derive(serde::Deserialize)]
        struct Header {
            r#type: String,
        }
        // A legacy struct historically accepts any value in its type field.
        if serde_json::from_slice::<Header>(first).is_ok_and(|h| h.r#type == "episode_start")
            && serde_json::from_slice::<TraceEntryV1>(first).is_err()
        {
            probe.read_to_end(&mut prefix)?;
            anyhow::ensure!(
                prefix.len() as u64 <= MAX_EPISODE_BYTES,
                "episode stream exceeds {MAX_EPISODE_BYTES} bytes"
            );
            return enrich_episodes(
                std::str::from_utf8(&prefix)?,
                &file,
                output,
                config,
                kind,
                enrich,
            )
            .await;
        }
        break;
    }
    // Return the bytes used to select the reader, including an incomplete large
    // legacy line. This also works for inputs that cannot seek.
    let rows = std::io::Cursor::new(prefix).chain(reader);
    let open_output = || {
        assay_common::file_output::open_preserving_inputs(output, &[&file])
            .context("failed to create output trace file")
    };
    let mut destination = None;
    for (i, line) in rows.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let mut entry: TraceEntryV1 = serde_json::from_str(&line)
            .with_context(|| format!("failed to parse trace entry at line {}", i + 1))?;
        // A full, valid legacy row establishes the format. Until then the
        // destination may still belong to a rejected or oversized episode.
        let out = match &mut destination {
            Some(out) => out,
            None => destination.insert(open_output()?),
        };
        if matches!(kind, Enrichment::Embeddings) && !has_embeddings(&entry.meta) {
            eprintln!("Embedding entry {}...", entry.request_id);
        }
        if let Some(tc) = config.tests.iter().find(|tc| tc.id == entry.request_id) {
            enrich(tc, &entry.prompt, &entry.response, &mut entry.meta).await?;
        }
        // Positional legacy records have always been emitted as objects.
        if line.trim_start().starts_with('[') {
            serde_json::to_writer(&mut *out, &entry)?;
            writeln!(out)?;
        } else {
            write_object_metadata(out, &line, None, &entry.meta)?;
            writeln!(out)?;
        }
    }
    if destination.is_none() {
        open_output()?;
    }
    Ok(())
}

pub async fn precompute_embeddings(
    input: &Path,
    output: &Path,
    embedder: Arc<dyn Embedder>,
    model: &str,
    config: &EvalConfig,
) -> anyhow::Result<()> {
    enrich_rows(
        input,
        output,
        config,
        Enrichment::Embeddings,
        async |tc, _, response, meta| {
            let Expected::SemanticSimilarityTo {
                semantic_similarity_to,
                ..
            } = &tc.expected
            else {
                return Ok(());
            };
            if has_embeddings(meta) {
                return Ok(());
            }
            let resp_vec = embedder.embed(response).await?;
            let ref_vec = embedder.embed(semantic_similarity_to).await?;
            if !meta.is_object() {
                *meta = serde_json::json!({});
            }
            if !meta.get("assay").is_some_and(|v| v.is_object()) {
                meta["assay"] = serde_json::json!({});
            }
            meta["assay"]["embeddings"] = serde_json::json!({
                "model":model,"response":resp_vec,"reference":ref_vec,
                "source_response":"precomputed","source_reference":"precomputed"
            });
            Ok(())
        },
    )
    .await
}

pub async fn precompute_judge(
    input: &Path,
    output: &Path,
    judge: &JudgeService,
    config: &EvalConfig,
) -> anyhow::Result<()> {
    enrich_rows(
        input,
        output,
        config,
        Enrichment::Judge,
        async |tc, prompt, response, meta| {
            let Some((rubric_id, rubric_version)) = judge_rubric(&tc.expected) else {
                return Ok(());
            };
            if meta.pointer(&format!("/assay/judge/{rubric_id}")).is_some() {
                return Ok(());
            }
            eprintln!("Judging entry {} ({rubric_id}) ...", tc.id);
            let input = crate::model::TestInput {
                prompt: prompt.to_owned(),
                context: None,
            };
            judge
                .evaluate(
                    &tc.id,
                    rubric_id,
                    &input,
                    response,
                    rubric_version,
                    meta,
                    None,
                )
                .await
        },
    )
    .await
}
