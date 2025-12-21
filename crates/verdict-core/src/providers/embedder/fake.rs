use super::Embedder;
use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Clone)]
pub struct FakeEmbedder {
    pub model: String,
    calls: Arc<AtomicUsize>,
    vec: Vec<f32>,
}

impl FakeEmbedder {
    pub fn new(model: &str, vec: Vec<f32>) -> Self {
        Self {
            model: model.to_string(),
            calls: Arc::new(AtomicUsize::new(0)),
            vec,
        }
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Embedder for FakeEmbedder {
    async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.vec.clone())
    }

    fn name(&self) -> &'static str {
        "fake"
    }

    fn model_id(&self) -> String {
        self.model.clone()
    }
}
