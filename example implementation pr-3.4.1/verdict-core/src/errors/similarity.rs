//! String similarity functions for closest match hints.
//!
//! Used to find the closest matching trace entry when a prompt doesn't match exactly.

use super::diagnostic::{ClosestMatch, DiffPosition};

/// Compute the Levenshtein distance between two strings.
pub fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();

    let a_len = a_chars.len();
    let b_len = b_chars.len();

    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }

    let mut matrix = vec![vec![0usize; b_len + 1]; a_len + 1];

    for i in 0..=a_len {
        matrix[i][0] = i;
    }
    for j in 0..=b_len {
        matrix[0][j] = j;
    }

    for i in 1..=a_len {
        for j in 1..=b_len {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };

            matrix[i][j] = (matrix[i - 1][j] + 1)
                .min(matrix[i][j - 1] + 1)
                .min(matrix[i - 1][j - 1] + cost);
        }
    }

    matrix[a_len][b_len]
}

/// Compute normalized similarity score (0.0 - 1.0) between two strings.
///
/// Returns 1.0 for identical strings, 0.0 for completely different strings.
pub fn similarity_score(a: &str, b: &str) -> f64 {
    if a == b {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }

    let distance = levenshtein_distance(a, b);
    let max_len = a.len().max(b.len());

    1.0 - (distance as f64 / max_len as f64)
}

/// Find the closest matching string from a list of candidates.
///
/// Returns `None` if candidates is empty or all similarities are below `min_threshold`.
pub fn find_closest_match(
    needle: &str,
    candidates: &[String],
    min_threshold: f64,
) -> Option<ClosestMatch> {
    if candidates.is_empty() {
        return None;
    }

    let mut best_match: Option<(usize, f64)> = None;

    for (i, candidate) in candidates.iter().enumerate() {
        let score = similarity_score(needle, candidate);
        if score >= min_threshold {
            if let Some((_, best_score)) = best_match {
                if score > best_score {
                    best_match = Some((i, score));
                }
            } else {
                best_match = Some((i, score));
            }
        }
    }

    best_match.map(|(idx, score)| {
        let candidate = &candidates[idx];
        let diff_positions = find_diff_positions(needle, candidate);

        ClosestMatch {
            prompt: candidate.clone(),
            similarity: score,
            diff_positions,
        }
    })
}

/// Find positions where two strings differ.
///
/// Returns a list of diff positions highlighting the differences.
pub fn find_diff_positions(expected: &str, found: &str) -> Vec<DiffPosition> {
    let mut diffs = Vec::new();

    let expected_chars: Vec<char> = expected.chars().collect();
    let found_chars: Vec<char> = found.chars().collect();

    let mut i = 0;
    let mut j = 0;

    while i < expected_chars.len() || j < found_chars.len() {
        // Find next difference
        while i < expected_chars.len()
            && j < found_chars.len()
            && expected_chars[i] == found_chars[j]
        {
            i += 1;
            j += 1;
        }

        if i >= expected_chars.len() && j >= found_chars.len() {
            break;
        }

        let diff_start_expected = i;
        let diff_start_found = j;

        // Find extent of difference - use word boundaries
        let expected_word_end = find_word_end(&expected_chars, i);
        let found_word_end = find_word_end(&found_chars, j);

        let expected_diff: String = expected_chars[i..expected_word_end].iter().collect();
        let found_diff: String = found_chars[j..found_word_end].iter().collect();

        if !expected_diff.is_empty() || !found_diff.is_empty() {
            diffs.push(DiffPosition {
                start: diff_start_expected,
                end: expected_word_end,
                expected: expected_diff,
                found: found_diff,
            });
        }

        i = expected_word_end;
        j = found_word_end;

        // Limit to 3 diffs for readability
        if diffs.len() >= 3 {
            break;
        }
    }

    diffs
}

/// Find the end of the current word (next whitespace or punctuation).
fn find_word_end(chars: &[char], start: usize) -> usize {
    let mut end = start;
    while end < chars.len() && !chars[end].is_whitespace() && !is_punctuation(chars[end]) {
        end += 1;
    }
    // If we didn't move, advance by at least one character
    if end == start && end < chars.len() {
        end += 1;
    }
    end
}

fn is_punctuation(c: char) -> bool {
    matches!(c, '.' | ',' | '!' | '?' | ';' | ':' | '"' | '\'' | '(' | ')' | '[' | ']' | '{' | '}')
}

/// Find multiple closest matches, useful for suggesting alternatives.
pub fn find_closest_matches(
    needle: &str,
    candidates: &[String],
    min_threshold: f64,
    max_results: usize,
) -> Vec<ClosestMatch> {
    if candidates.is_empty() {
        return Vec::new();
    }

    let mut scored: Vec<(usize, f64)> = candidates
        .iter()
        .enumerate()
        .map(|(i, c)| (i, similarity_score(needle, c)))
        .filter(|(_, score)| *score >= min_threshold)
        .collect();

    // Sort by score descending
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    scored
        .into_iter()
        .take(max_results)
        .map(|(idx, score)| {
            let candidate = &candidates[idx];
            ClosestMatch {
                prompt: candidate.clone(),
                similarity: score,
                diff_positions: find_diff_positions(needle, candidate),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein_identical() {
        assert_eq!(levenshtein_distance("hello", "hello"), 0);
    }

    #[test]
    fn test_levenshtein_single_char() {
        assert_eq!(levenshtein_distance("hello", "hallo"), 1);
        assert_eq!(levenshtein_distance("capital", "capitol"), 1);
    }

    #[test]
    fn test_levenshtein_insertion() {
        assert_eq!(levenshtein_distance("hello", "helllo"), 1);
    }

    #[test]
    fn test_levenshtein_deletion() {
        assert_eq!(levenshtein_distance("hello", "helo"), 1);
    }

    #[test]
    fn test_levenshtein_empty() {
        assert_eq!(levenshtein_distance("", "hello"), 5);
        assert_eq!(levenshtein_distance("hello", ""), 5);
        assert_eq!(levenshtein_distance("", ""), 0);
    }

    #[test]
    fn test_similarity_identical() {
        assert_eq!(similarity_score("hello", "hello"), 1.0);
    }

    #[test]
    fn test_similarity_similar() {
        let score = similarity_score(
            "What is the capital of France?",
            "What is the capitol of France?",
        );
        assert!(score > 0.9);
        assert!(score < 1.0);
    }

    #[test]
    fn test_similarity_different() {
        let score = similarity_score("hello", "world");
        assert!(score < 0.5);
    }

    #[test]
    fn test_find_closest_match() {
        let candidates = vec![
            "What is the capitol of France?".to_string(),
            "What is the capital of Germany?".to_string(),
            "Hello world".to_string(),
        ];

        let result = find_closest_match(
            "What is the capital of France?",
            &candidates,
            0.5,
        );

        assert!(result.is_some());
        let closest = result.unwrap();
        assert!(closest.similarity > 0.9);
        assert!(closest.prompt.contains("capitol") || closest.prompt.contains("capital"));
    }

    #[test]
    fn test_find_closest_match_no_match() {
        let candidates = vec!["completely different".to_string()];

        let result = find_closest_match("hello world", &candidates, 0.9);
        assert!(result.is_none());
    }

    #[test]
    fn test_find_diff_positions() {
        let diffs = find_diff_positions(
            "What is the capital of France?",
            "What is the capitol of France?",
        );

        assert!(!diffs.is_empty());
        // Should find the capital/capitol diff
        let has_capital_diff = diffs
            .iter()
            .any(|d| d.expected == "capital" && d.found == "capitol");
        assert!(has_capital_diff);
    }

    #[test]
    fn test_find_closest_matches_multiple() {
        let candidates = vec![
            "What is the capital of France?".to_string(),
            "What is the capital of Germany?".to_string(),
            "What is the capital of Italy?".to_string(),
            "Hello world".to_string(),
        ];

        let results = find_closest_matches(
            "What is the capital of Spain?",
            &candidates,
            0.7,
            3,
        );

        assert_eq!(results.len(), 3);
        // All should be the "capital of X" variants
        for r in &results {
            assert!(r.prompt.contains("capital"));
        }
    }
}
