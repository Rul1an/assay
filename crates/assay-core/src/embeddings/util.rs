use sha2::{Digest, Sha256};

pub fn encode_vec_f32(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

#[allow(clippy::manual_is_multiple_of)]
pub fn decode_vec_f32(bytes: &[u8]) -> anyhow::Result<Vec<f32>> {
    if bytes.len() % 4 != 0 {
        anyhow::bail!("config error: invalid embedding blob size");
    }
    let mut v = Vec::with_capacity(bytes.len() / 4);
    for chunk in bytes.chunks_exact(4) {
        v.push(f32::from_le_bytes(chunk.try_into().unwrap()));
    }
    Ok(v)
}

pub fn sha256_hex(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    hex::encode(h.finalize())
}

pub fn embed_cache_key(model_id: &str, text: &str) -> String {
    format!("emb|{}|{}", model_id, sha256_hex(text))
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> anyhow::Result<f64> {
    let af: Vec<f64> = a.iter().map(|x| *x as f64).collect();
    let bf: Vec<f64> = b.iter().map(|x| *x as f64).collect();
    cosine_similarity_f64(&af, &bf)
}

pub fn cosine_similarity_f64(a: &[f64], b: &[f64]) -> anyhow::Result<f64> {
    if a.is_empty() || a.len() != b.len() {
        anyhow::bail!(
            "config error: embedding dims mismatch (a={}, b={})",
            a.len(),
            b.len()
        );
    }
    let mut dot = 0.0f64;
    let mut na = 0.0f64;
    let mut nb = 0.0f64;

    for i in 0..a.len() {
        let x = a[i];
        let y = b[i];
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    let denom = na.sqrt() * nb.sqrt();
    if denom == 0.0 {
        anyhow::bail!("config error: zero-norm embedding");
    }
    let s = dot / denom;
    Ok(s.clamp(-1.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_roundtrip() -> anyhow::Result<()> {
        let v = vec![0.1_f32, -0.2_f32, 3.5_f32];
        let blob = encode_vec_f32(&v);
        let out = decode_vec_f32(&blob)?;
        assert_eq!(v.len(), out.len());
        for i in 0..v.len() {
            assert!((v[i] - out[i]).abs() < 1e-6);
        }
        Ok(())
    }

    #[test]
    fn cosine_identical_is_one() -> anyhow::Result<()> {
        let a = vec![1.0_f32, 0.0, 0.0];
        let b = vec![1.0_f32, 0.0, 0.0];
        let s = cosine_similarity(&a, &b)?;
        assert!((s - 1.0).abs() < 1e-9);
        Ok(())
    }
}
