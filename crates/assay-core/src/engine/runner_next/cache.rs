use super::super::Runner;

pub(crate) async fn embed_text_impl(
    runner: &Runner,
    model_id: &str,
    embedder: &dyn crate::providers::embedder::Embedder,
    text: &str,
) -> anyhow::Result<(Vec<f32>, &'static str)> {
    use crate::embeddings::util::embed_cache_key;

    let key = embed_cache_key(model_id, text);

    if !runner.refresh_embeddings {
        if let Some((_m, vec)) = runner.store.get_embedding(&key)? {
            return Ok((vec, "cache"));
        }
    }

    let vec = embedder.embed(text).await?;
    runner.store.put_embedding(&key, model_id, &vec)?;
    Ok((vec, "live"))
}
