use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClosestMatch {
    pub prompt: String,
    pub similarity: f64,
}

pub fn closest_prompt<'a>(needle: &str, hay: impl Iterator<Item = &'a String>) -> Option<ClosestMatch> {
    let mut best: Option<ClosestMatch> = None;

    // Threshold for suggestion. 0.55 is a reasonable heuristic.
    const THRESHOLD: f64 = 0.55;

    for candidate in hay {
        let sim = strsim::normalized_levenshtein(needle, candidate);
        if sim >= THRESHOLD {
            if best.as_ref().map_or(true, |b| sim > b.similarity) {
                best = Some(ClosestMatch {
                    prompt: candidate.clone(),
                    similarity: sim,
                });
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_closest_prompt_exact() {
        let candidates = vec!["foo".to_string(), "bar".to_string()];
        let hit = closest_prompt("foo", candidates.iter()).unwrap();
        assert_eq!(hit.prompt, "foo");
        assert!((hit.similarity - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_closest_prompt_typo() {
        let candidates = vec!["capitol".to_string(), "bar".to_string()];
        let hit = closest_prompt("capital", candidates.iter()).unwrap();
        assert_eq!(hit.prompt, "capitol");
        assert!(hit.similarity > 0.8);
    }

    #[test]
    fn test_closest_prompt_none() {
        let candidates = vec!["zulu".to_string(), "bar".to_string()];
        let hit = closest_prompt("alpha", candidates.iter());
        assert!(hit.is_none());
    }
}
