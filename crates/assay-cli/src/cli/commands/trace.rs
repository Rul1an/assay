use super::super::args::{TraceArgs, TraceSub};
use super::exit_codes;

use assay_core::trace;
mod import_mcp;

pub async fn cmd_trace(args: TraceArgs, legacy_mode: bool) -> anyhow::Result<i32> {
    match args.cmd {
        TraceSub::Ingest { input, output } => {
            trace::ingest::ingest_file(&input, &output)?;
            eprintln!(
                "Ingested trace from {} to {}",
                input.display(),
                output.display()
            );
            Ok(exit_codes::OK)
        }
        TraceSub::IngestOtel {
            input,
            db,
            suite: _suite,
            out_trace,
        } => {
            use assay_core::storage::Store;
            use assay_core::trace::otel_ingest::{convert_spans_to_episodes, OtelSpan};
            use std::io::BufRead;

            let file = std::fs::File::open(&input)
                .map_err(|e| anyhow::anyhow!("failed to open input file: {}", e))?;
            let reader = std::io::BufReader::new(file);

            let mut spans = Vec::new();
            for line in reader.lines() {
                let line = line?; // propagate error
                if line.trim().is_empty() {
                    continue;
                }
                let span: OtelSpan = serde_json::from_str(&line)
                    .map_err(|e| anyhow::anyhow!("failed to parse OTel span: {}", e))?;
                spans.push(span);
            }

            let events = convert_spans_to_episodes(spans);
            let count = events.len();

            let store = Store::open(&db)?;
            store.init_schema()?; // ensure tables exist

            store.insert_batch(&events, None, None)?;

            eprintln!(
                "Ingested {} OTel spans as {} V2 events into {}",
                count,
                events.len(),
                db.display()
            );

            if let Some(out_path) = out_trace {
                let f = std::fs::File::create(&out_path)
                    .map_err(|e| anyhow::anyhow!("failed to create output trace file: {}", e))?;
                let mut writer = std::io::BufWriter::new(f);
                for event in &events {
                    use std::io::Write;
                    let json = serde_json::to_string(event)?;
                    writeln!(writer, "{}", json)?;
                }
                eprintln!("Wrote trace replay file to {}", out_path.display());
            }

            Ok(exit_codes::OK)
        }
        TraceSub::Verify { trace, config } => {
            let cfg = assay_core::config::load_config(&config, legacy_mode)
                .map_err(|e| anyhow::anyhow!("failed to load config: {}", e))?;

            trace::verify::verify_coverage(&trace, &cfg)?;
            Ok(exit_codes::OK)
        }
        TraceSub::PrecomputeEmbeddings {
            trace,
            config,
            embedder,
            model,
            output,
        } => {
            let cfg = assay_core::config::load_config(&config, legacy_mode)
                .map_err(|e| anyhow::anyhow!("failed to load config: {}", e))?;

            // Build embedder (simplified version of build_runner logic)
            use assay_core::providers::embedder::{
                fake::FakeEmbedder, openai::OpenAIEmbedder, Embedder,
            };
            use std::sync::Arc;

            let embedder_client: Arc<dyn Embedder> = match embedder.as_str() {
                "openai" => {
                    let key = std::env::var("OPENAI_API_KEY")
                        .map_err(|_| anyhow::anyhow!("OPENAI_API_KEY required for precompute"))?;
                    Arc::new(OpenAIEmbedder::new(model.clone(), key))
                }
                "fake" => Arc::new(FakeEmbedder::new(&model, vec![0.1; 1536])), // Mock vector
                _ => anyhow::bail!("unknown embedder: {}", embedder),
            };

            let out_path = output.unwrap_or_else(|| trace.clone()); // Default overwrite logic? No, let's play safe.
                                                                    // If output is None, maybe we should warn?
                                                                    // The user args say output is Option.
                                                                    // Let's assume overwrite if not provided, as per "enrichment" philosophy.
                                                                    // Wait, I can't overwrite input while reading it easily without slurp.
                                                                    // `precompute_embeddings` takes input path and output path.
                                                                    // If output is None, we should likely write to a temp file and rename.

            let final_output = out_path.clone();
            let effective_output = if final_output == trace {
                // If rewriting in place, write to temp first
                let mut temp = trace.clone();
                temp.set_extension("tmp.jsonl");
                temp
            } else {
                final_output.clone()
            };

            trace::precompute::precompute_embeddings(
                &trace,
                &effective_output,
                embedder_client,
                &model,
                &cfg,
            )
            .await?;

            if effective_output != final_output {
                std::fs::rename(&effective_output, &final_output)?;
            }

            println!(
                "Precomputed embeddings for {} -> {}",
                trace.display(),
                final_output.display()
            );
            Ok(exit_codes::OK)
        }
        TraceSub::PrecomputeJudge {
            trace,
            config,
            judge,
            judge_model,
            output,
        } => {
            let cfg = assay_core::config::load_config(&config, legacy_mode)
                .map_err(|e| anyhow::anyhow!("failed to load config: {}", e))?;

            // Build judge service
            // This is complex, requires JudgeRuntimeConfig etc.
            // We need to implement a lightweight builder or reuse existing one.
            // Reusing `assay_core::judge::JudgeService::new` requires `JudgeStore` and `LlmClient`.
            // We can use a NullStore or MemoryStore? Precompute usually implies we don't care about caching results IN the DB,
            // but we might want to USE the validation logic.
            // Actually, we are WRITING to the trace JSONL. We probably don't need the sqlite store enabled?
            // `JudgeService` requires a `JudgeStore` trait object? It takes `JudgeCache` struct which takes `Store`.
            // This dependency chain `JudgeService -> JudgeCache -> Store -> Sqlite` makes lightweight usage hard.
            // Maybe we should bypass `JudgeService` and call `LlmClient` directly?
            // BUT `JudgeService` encapsulates the prompt logic (`templates::render_judge_prompt`). We NEED that.

            // So we need a Store. We can open an in-memory SQLite store?
            use assay_core::storage::Store;
            let store = Store::memory()?;
            // We need to init schema for it to work?
            store.init_schema()?;
            let judge_store = assay_core::storage::judge_cache::JudgeCache::new(store);

            let model = judge_model
                .clone()
                .unwrap_or_else(|| "gpt-4o-mini".to_string());

            use assay_core::providers::llm::{fake::FakeClient, openai::OpenAIClient, LlmClient};
            use std::sync::Arc;

            let client: Option<Arc<dyn LlmClient>> = match judge.as_str() {
                "openai" => {
                    let key = std::env::var("OPENAI_API_KEY")
                        .map_err(|_| anyhow::anyhow!("OPENAI_API_KEY required"))?;
                    Some(Arc::new(OpenAIClient::new(model.clone(), key, 0.0, 1000)))
                }
                "fake" => Some(Arc::new(FakeClient::new(model.clone()))),
                _ => anyhow::bail!("unknown judge provider: {}", judge),
            };

            let config = assay_core::judge::JudgeRuntimeConfig {
                enabled: true,
                provider: judge.clone(),
                model: Some(model),
                samples: 1, // Precompute usually deterministic 1 pass?
                temperature: 0.0,
                max_tokens: 1000,
                refresh: true,
            };

            let service = assay_core::judge::JudgeService::new(config, judge_store, client);

            let out_path = output.unwrap_or_else(|| trace.clone());
            let final_output = out_path.clone();
            let effective_output = if final_output == trace {
                let mut temp = trace.clone();
                temp.set_extension("tmp.jsonl");
                temp
            } else {
                final_output.clone()
            };

            trace::precompute::precompute_judge(&trace, &effective_output, &service, &cfg).await?;

            if effective_output != final_output {
                std::fs::rename(&effective_output, &final_output)?;
            }

            println!(
                "Precomputed judge scores for {} -> {}",
                trace.display(),
                final_output.display()
            );
            Ok(exit_codes::OK)
        }

        TraceSub::ImportMcp {
            input,
            out_trace,
            format,
            episode_id,
            test_id,
            prompt,
        } => {
            let format_enum = match format.as_str() {
                "inspector" => assay_core::mcp::McpInputFormat::Inspector,
                "jsonrpc" => assay_core::mcp::McpInputFormat::JsonRpc,
                other => anyhow::bail!("unknown format: {}", other),
            };

            import_mcp::run(import_mcp::ImportMcpArgs {
                input,
                out_trace,
                format: format_enum,
                episode_id,
                test_id,
                prompt,
            })?;
            Ok(exit_codes::OK)
        }
    }
}
