//! Strict JSON parsing with duplicate key rejection.
//!
//! Standard JSON parsers (including serde_json) accept duplicate keys with
//! "last key wins" semantics. This is a security risk for mandate evidence
//! where different parsers in the pipeline could interpret the same JSON
//! differently.
//!
//! # Security Rationale
//!
//! ```text
//! Attacker crafts: {"mandate_id": "legit", "mandate_id": "evil"}
//! Parser A sees:   mandate_id = "legit" (first wins)
//! Parser B sees:   mandate_id = "evil"  (last wins)
//! ```
//!
//! By rejecting duplicates at ingest, we ensure all downstream code sees
//! the same semantics.
//!
//! # Normative Behavior (SPEC-Mandate-v1)
//!
//! Object member names MUST be compared after JSON string unescaping:
//!
//! - Unicode escapes (`\uXXXX`) are decoded to actual characters
//! - Surrogate pairs are combined into Unicode scalars (U+10000+)
//! - Standard escapes (`\n`, `\t`, `\/`, `\\`, `\"`) are decoded
//!
//! This ensures `"a"` and `"\u0061"` are correctly detected as duplicate keys.
//!
//! # DoS Protection
//!
//! This validator enforces limits to prevent resource exhaustion:
//! - Max nesting depth: 64 levels
//! - Max keys per object: 10,000
//! - Max string length: 1MB
//!
//! # Usage
//!
//! ```
//! use assay_evidence::json_strict::from_str_strict;
//! use serde::Deserialize;
//!
//! #[derive(Deserialize)]
//! struct Data { key: String }
//!
//! // Rejects: {"key": "a", "key": "b"}
//! let result = from_str_strict::<Data>(r#"{"key": "a", "key": "b"}"#);
//! assert!(result.is_err());
//!
//! // Also rejects: {"a": 1, "\u0061": 2} (same key after decoding)
//! let result = from_str_strict::<serde_json::Value>(r#"{"a": 1, "\u0061": 2}"#);
//! assert!(result.is_err());
//! ```

use serde::de::DeserializeOwned;
use std::collections::HashSet;
use thiserror::Error;

// DoS protection limits
const MAX_NESTING_DEPTH: usize = 64;
const MAX_KEYS_PER_OBJECT: usize = 10_000;
const MAX_STRING_LENGTH: usize = 1_048_576; // 1MB

/// Error returned when strict JSON parsing fails.
#[derive(Debug, Error)]
pub enum StrictJsonError {
    #[error("Duplicate key '{key}' at path '{path}'")]
    DuplicateKey { key: String, path: String },

    #[error("Invalid unicode escape sequence at position {position}")]
    InvalidUnicodeEscape { position: usize },

    #[error("Lone surrogate at position {position}: {codepoint}")]
    LoneSurrogate { position: usize, codepoint: String },

    #[error("JSON parse error: {0}")]
    ParseError(#[from] serde_json::Error),

    #[error("Security limit exceeded: nesting depth {depth} exceeds maximum {MAX_NESTING_DEPTH}")]
    NestingTooDeep { depth: usize },

    #[error(
        "Security limit exceeded: {count} keys in object exceeds maximum {MAX_KEYS_PER_OBJECT}"
    )]
    TooManyKeys { count: usize },

    #[error("Security limit exceeded: string length {length} exceeds maximum {MAX_STRING_LENGTH}")]
    StringTooLong { length: usize },
}

/// Parse JSON with strict duplicate key rejection.
///
/// Scans the JSON for duplicate keys at any nesting level before deserializing.
/// This ensures semantic consistency across different JSON parsers.
pub fn from_str_strict<T: DeserializeOwned>(s: &str) -> Result<T, StrictJsonError> {
    // Phase 1: Scan for duplicate keys and invalid unicode
    validate_json_strict(s)?;

    // Phase 2: Deserialize (now safe)
    Ok(serde_json::from_str(s)?)
}

/// Validate JSON string for security issues without deserializing.
///
/// Checks:
/// - Duplicate keys at any nesting level
/// - Lone surrogates in unicode escapes
pub fn validate_json_strict(s: &str) -> Result<(), StrictJsonError> {
    let mut validator = JsonValidator::new(s);
    validator.validate()
}

/// Internal validator state machine.
struct JsonValidator<'a> {
    chars: std::iter::Peekable<std::str::CharIndices<'a>>,
    /// Stack of (path, keys_at_this_level)
    object_stack: Vec<(String, HashSet<String>)>,
    current_path: String,
    /// Current nesting depth (objects + arrays)
    depth: usize,
    _phantom: std::marker::PhantomData<&'a str>,
}

impl<'a> JsonValidator<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            chars: input.char_indices().peekable(),
            object_stack: Vec::new(),
            current_path: String::new(),
            depth: 0,
            _phantom: std::marker::PhantomData,
        }
    }

    fn validate(&mut self) -> Result<(), StrictJsonError> {
        self.skip_whitespace();
        self.validate_value()?;
        Ok(())
    }

    fn validate_value(&mut self) -> Result<(), StrictJsonError> {
        self.skip_whitespace();

        match self.peek_char() {
            Some('{') => self.validate_object(),
            Some('[') => self.validate_array(),
            Some('"') => self.validate_string().map(|_| ()),
            Some(c) if c == '-' || c.is_ascii_digit() => self.validate_number(),
            Some('t') | Some('f') => self.validate_bool(),
            Some('n') => self.validate_null(),
            Some(c) => Err(StrictJsonError::ParseError(
                serde_json::from_str::<()>(&format!("unexpected char '{}'", c)).unwrap_err(),
            )),
            None => Ok(()),
        }
    }

    fn validate_object(&mut self) -> Result<(), StrictJsonError> {
        self.expect_char('{')?;
        self.skip_whitespace();

        // DoS: Check nesting depth
        self.depth += 1;
        if self.depth > MAX_NESTING_DEPTH {
            return Err(StrictJsonError::NestingTooDeep { depth: self.depth });
        }

        // Push new scope
        let path = self.current_path.clone();
        self.object_stack.push((path, HashSet::new()));

        if self.peek_char() == Some('}') {
            self.next_char();
            self.object_stack.pop();
            self.depth -= 1;
            return Ok(());
        }

        loop {
            self.skip_whitespace();

            // Parse key
            let key = self.validate_string()?;

            // Check for duplicate and DoS: key count
            if let Some((path, keys)) = self.object_stack.last_mut() {
                if keys.len() >= MAX_KEYS_PER_OBJECT {
                    return Err(StrictJsonError::TooManyKeys {
                        count: keys.len() + 1,
                    });
                }
                if !keys.insert(key.clone()) {
                    return Err(StrictJsonError::DuplicateKey {
                        key,
                        path: if path.is_empty() {
                            "/".to_string()
                        } else {
                            path.clone()
                        },
                    });
                }
            }

            self.skip_whitespace();
            self.expect_char(':')?;

            // Update path for nested validation
            let old_path = self.current_path.clone();
            self.current_path = if self.current_path.is_empty() {
                format!("/{}", key)
            } else {
                format!("{}/{}", self.current_path, key)
            };

            // Parse value
            self.validate_value()?;

            // Restore path
            self.current_path = old_path;

            self.skip_whitespace();

            match self.peek_char() {
                Some(',') => {
                    self.next_char();
                }
                Some('}') => {
                    self.next_char();
                    self.object_stack.pop();
                    self.depth -= 1;
                    return Ok(());
                }
                _ => {
                    return Err(StrictJsonError::ParseError(
                        serde_json::from_str::<()>("expected ',' or '}'").unwrap_err(),
                    ))
                }
            }
        }
    }

    fn validate_array(&mut self) -> Result<(), StrictJsonError> {
        self.expect_char('[')?;
        self.skip_whitespace();

        // DoS: Check nesting depth
        self.depth += 1;
        if self.depth > MAX_NESTING_DEPTH {
            return Err(StrictJsonError::NestingTooDeep { depth: self.depth });
        }

        if self.peek_char() == Some(']') {
            self.next_char();
            self.depth -= 1;
            return Ok(());
        }

        let mut index = 0;
        loop {
            let old_path = self.current_path.clone();
            self.current_path = format!("{}/{}", self.current_path, index);

            self.validate_value()?;

            self.current_path = old_path;
            index += 1;

            self.skip_whitespace();

            match self.peek_char() {
                Some(',') => {
                    self.next_char();
                    self.skip_whitespace();
                }
                Some(']') => {
                    self.next_char();
                    self.depth -= 1;
                    return Ok(());
                }
                _ => {
                    return Err(StrictJsonError::ParseError(
                        serde_json::from_str::<()>("expected ',' or ']'").unwrap_err(),
                    ))
                }
            }
        }
    }

    /// Validate a JSON string and return DECODED content for key comparison.
    ///
    /// CRITICAL: We decode unicode escapes (\uXXXX) to actual characters so that
    /// duplicate key detection works correctly. Otherwise `"a"` and `"\u0061"`
    /// would not be detected as duplicates.
    ///
    /// Surrogate pairs are validated AND combined into proper Unicode scalars.
    fn validate_string(&mut self) -> Result<String, StrictJsonError> {
        self.expect_char('"')?;

        let mut result = String::new();
        let mut prev_high_surrogate: Option<(usize, u32)> = None; // (position, codepoint)
        let mut char_count = 0usize;

        loop {
            // DoS: Check string length
            char_count += 1;
            if char_count > MAX_STRING_LENGTH {
                return Err(StrictJsonError::StringTooLong { length: char_count });
            }

            match self.next_char() {
                Some((_, '"')) => {
                    // Check for trailing high surrogate
                    if let Some((pos, _)) = prev_high_surrogate {
                        return Err(StrictJsonError::LoneSurrogate {
                            position: pos,
                            codepoint: "unpaired high surrogate at end of string".to_string(),
                        });
                    }
                    return Ok(result);
                }
                Some((pos, '\\')) => {
                    match self.next_char() {
                        Some((_, 'u')) => {
                            // Unicode escape
                            let (codepoint, hex_str) = self.parse_unicode_escape(pos)?;

                            // Check surrogate handling
                            if (0xD800..=0xDBFF).contains(&codepoint) {
                                // High surrogate - must be followed by low surrogate
                                if let Some((high_pos, _)) = prev_high_surrogate {
                                    return Err(StrictJsonError::LoneSurrogate {
                                        position: high_pos,
                                        codepoint: "consecutive high surrogates".to_string(),
                                    });
                                }
                                prev_high_surrogate = Some((pos, codepoint));
                            } else if (0xDC00..=0xDFFF).contains(&codepoint) {
                                // Low surrogate - must follow high surrogate
                                if let Some((_, high)) = prev_high_surrogate {
                                    // Combine into Unicode scalar
                                    let combined =
                                        0x10000 + ((high - 0xD800) << 10) + (codepoint - 0xDC00);
                                    if let Some(c) = char::from_u32(combined) {
                                        result.push(c);
                                    } else {
                                        return Err(StrictJsonError::InvalidUnicodeEscape {
                                            position: pos,
                                        });
                                    }
                                    prev_high_surrogate = None;
                                } else {
                                    return Err(StrictJsonError::LoneSurrogate {
                                        position: pos,
                                        codepoint: format!("\\u{}", hex_str),
                                    });
                                }
                            } else {
                                // Regular BMP character
                                if let Some((high_pos, _)) = prev_high_surrogate {
                                    return Err(StrictJsonError::LoneSurrogate {
                                        position: high_pos,
                                        codepoint: "high surrogate not followed by low".to_string(),
                                    });
                                }
                                // Decode to actual character
                                if let Some(c) = char::from_u32(codepoint) {
                                    result.push(c);
                                } else {
                                    return Err(StrictJsonError::InvalidUnicodeEscape {
                                        position: pos,
                                    });
                                }
                            }
                        }
                        Some((_, c)) => {
                            // Other JSON escapes - must not have pending high surrogate
                            if let Some((high_pos, _)) = prev_high_surrogate {
                                return Err(StrictJsonError::LoneSurrogate {
                                    position: high_pos,
                                    codepoint: "high surrogate not followed by low".to_string(),
                                });
                            }
                            // Decode standard JSON escapes
                            let decoded = match c {
                                'n' => '\n',
                                'r' => '\r',
                                't' => '\t',
                                '\\' => '\\',
                                '/' => '/',
                                '"' => '"',
                                'b' => '\x08', // backspace
                                'f' => '\x0C', // form feed
                                _ => {
                                    return Err(StrictJsonError::ParseError(
                                        serde_json::from_str::<()>(&format!(
                                            "invalid escape sequence \\{}",
                                            c
                                        ))
                                        .unwrap_err(),
                                    ))
                                }
                            };
                            result.push(decoded);
                        }
                        None => {
                            return Err(StrictJsonError::ParseError(
                                serde_json::from_str::<()>("unexpected end of escape").unwrap_err(),
                            ))
                        }
                    }
                }
                Some((_, c)) => {
                    // Regular character - must not have pending high surrogate
                    if let Some((high_pos, _)) = prev_high_surrogate {
                        return Err(StrictJsonError::LoneSurrogate {
                            position: high_pos,
                            codepoint: "high surrogate not followed by low".to_string(),
                        });
                    }
                    result.push(c);
                }
                None => {
                    return Err(StrictJsonError::ParseError(
                        serde_json::from_str::<()>("unterminated string").unwrap_err(),
                    ))
                }
            }
        }
    }

    fn parse_unicode_escape(&mut self, start_pos: usize) -> Result<(u32, String), StrictJsonError> {
        let mut hex = String::with_capacity(4);
        for _ in 0..4 {
            match self.next_char() {
                Some((_, c)) if c.is_ascii_hexdigit() => hex.push(c),
                _ => {
                    return Err(StrictJsonError::InvalidUnicodeEscape {
                        position: start_pos,
                    })
                }
            }
        }

        let codepoint =
            u32::from_str_radix(&hex, 16).map_err(|_| StrictJsonError::InvalidUnicodeEscape {
                position: start_pos,
            })?;

        Ok((codepoint, hex))
    }

    fn validate_number(&mut self) -> Result<(), StrictJsonError> {
        // Optional minus
        if self.peek_char() == Some('-') {
            self.next_char();
        }

        // Integer part
        match self.peek_char() {
            Some('0') => {
                self.next_char();
            }
            Some(c) if c.is_ascii_digit() => {
                while self.peek_char().is_some_and(|c| c.is_ascii_digit()) {
                    self.next_char();
                }
            }
            _ => {
                return Err(StrictJsonError::ParseError(
                    serde_json::from_str::<()>("expected digit").unwrap_err(),
                ))
            }
        }

        // Fractional part
        if self.peek_char() == Some('.') {
            self.next_char();
            if !self.peek_char().is_some_and(|c| c.is_ascii_digit()) {
                return Err(StrictJsonError::ParseError(
                    serde_json::from_str::<()>("expected digit after decimal").unwrap_err(),
                ));
            }
            while self.peek_char().is_some_and(|c| c.is_ascii_digit()) {
                self.next_char();
            }
        }

        // Exponent part
        if self.peek_char().is_some_and(|c| c == 'e' || c == 'E') {
            self.next_char();
            if self.peek_char().is_some_and(|c| c == '+' || c == '-') {
                self.next_char();
            }
            if !self.peek_char().is_some_and(|c| c.is_ascii_digit()) {
                return Err(StrictJsonError::ParseError(
                    serde_json::from_str::<()>("expected digit in exponent").unwrap_err(),
                ));
            }
            while self.peek_char().is_some_and(|c| c.is_ascii_digit()) {
                self.next_char();
            }
        }

        Ok(())
    }

    fn validate_bool(&mut self) -> Result<(), StrictJsonError> {
        if self.consume_keyword("true") || self.consume_keyword("false") {
            Ok(())
        } else {
            Err(StrictJsonError::ParseError(
                serde_json::from_str::<()>("expected bool").unwrap_err(),
            ))
        }
    }

    fn validate_null(&mut self) -> Result<(), StrictJsonError> {
        if self.consume_keyword("null") {
            Ok(())
        } else {
            Err(StrictJsonError::ParseError(
                serde_json::from_str::<()>("expected null").unwrap_err(),
            ))
        }
    }

    fn consume_keyword(&mut self, keyword: &str) -> bool {
        let remaining: String = self.chars.clone().map(|(_, c)| c).collect();
        if remaining.starts_with(keyword) {
            for _ in keyword.chars() {
                self.next_char();
            }
            true
        } else {
            false
        }
    }

    fn skip_whitespace(&mut self) {
        while self.peek_char().is_some_and(|c| c.is_ascii_whitespace()) {
            self.next_char();
        }
    }

    fn peek_char(&mut self) -> Option<char> {
        self.chars.peek().map(|(_, c)| *c)
    }

    fn next_char(&mut self) -> Option<(usize, char)> {
        self.chars.next()
    }

    fn expect_char(&mut self, expected: char) -> Result<(), StrictJsonError> {
        match self.next_char() {
            Some((_, c)) if c == expected => Ok(()),
            _ => Err(StrictJsonError::ParseError(
                serde_json::from_str::<()>(&format!("expected '{}'", expected)).unwrap_err(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct TestStruct {
        key: String,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct NestedStruct {
        outer: Inner,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct Inner {
        key: String,
    }

    // === B1: Duplicate Key Tests ===

    #[test]
    fn test_rejects_top_level_duplicate() {
        let json = r#"{"key": "first", "key": "second"}"#;
        let result = from_str_strict::<TestStruct>(json);

        assert!(matches!(
            result,
            Err(StrictJsonError::DuplicateKey { key, path })
            if key == "key" && path == "/"
        ));
    }

    #[test]
    fn test_rejects_nested_duplicate() {
        let json = r#"{"outer": {"key": "a", "key": "b"}}"#;
        let result = from_str_strict::<NestedStruct>(json);

        assert!(matches!(
            result,
            Err(StrictJsonError::DuplicateKey { key, path })
            if key == "key" && path == "/outer"
        ));
    }

    #[test]
    fn test_rejects_deeply_nested_duplicate() {
        let json = r#"{"data": {"scope": {"tools": ["a"], "tools": ["b"]}}}"#;
        let result = validate_json_strict(json);

        assert!(matches!(
            result,
            Err(StrictJsonError::DuplicateKey { key, path })
            if key == "tools" && path == "/data/scope"
        ));
    }

    #[test]
    fn test_accepts_same_key_different_objects() {
        // Same key name in different objects is fine
        let json = r#"{"a": {"key": "1"}, "b": {"key": "2"}}"#;
        let result = validate_json_strict(json);
        assert!(result.is_ok());
    }

    /// CRITICAL: Unicode escape normalization for duplicate detection.
    /// "a" and "\u0061" are the SAME key and MUST be detected as duplicate.
    #[test]
    fn test_rejects_unicode_escape_duplicate() {
        // \u0061 = 'a' - these are duplicate keys!
        let json = r#"{"a": 1, "\u0061": 2}"#;
        let result = validate_json_strict(json);

        assert!(
            matches!(&result, Err(StrictJsonError::DuplicateKey { key, .. }) if key == "a"),
            "Unicode escaped key must be detected as duplicate: {:?}",
            result
        );
    }

    #[test]
    fn test_rejects_mixed_escape_duplicate() {
        // \u0048\u0065\u006c\u006c\u006f = "Hello"
        let json = r#"{"Hello": 1, "\u0048\u0065\u006c\u006c\u006f": 2}"#;
        let result = validate_json_strict(json);

        assert!(
            matches!(&result, Err(StrictJsonError::DuplicateKey { key, .. }) if key == "Hello"),
            "Fully escaped key must be detected as duplicate: {:?}",
            result
        );
    }

    #[test]
    fn test_rejects_partial_escape_duplicate() {
        // "k\u0065y" = "key"
        let json = r#"{"key": 1, "k\u0065y": 2}"#;
        let result = validate_json_strict(json);

        assert!(
            matches!(&result, Err(StrictJsonError::DuplicateKey { key, .. }) if key == "key"),
            "Partially escaped key must be detected as duplicate: {:?}",
            result
        );
    }

    /// Non-BMP character via surrogate pair must equal direct UTF-8.
    #[test]
    fn test_rejects_surrogate_pair_duplicate() {
        // \uD83D\uDE00 = 😀 (grinning face, U+1F600)
        let json = r#"{"😀": 1, "\uD83D\uDE00": 2}"#;
        let result = validate_json_strict(json);

        assert!(
            matches!(&result, Err(StrictJsonError::DuplicateKey { key, .. }) if key == "😀"),
            "Surrogate pair key must be detected as duplicate of direct UTF-8: {:?}",
            result
        );
    }

    /// Escaped solidus (\/) must equal unescaped solidus (/).
    #[test]
    fn test_rejects_escaped_solidus_duplicate() {
        // JSON allows \/ as escape for /
        let json = r#"{"a/b": 1, "a\/b": 2}"#;
        let result = validate_json_strict(json);

        assert!(
            matches!(&result, Err(StrictJsonError::DuplicateKey { key, .. }) if key == "a/b"),
            "Escaped solidus key must be detected as duplicate: {:?}",
            result
        );
    }

    /// Escaped quotes and backslashes in keys.
    #[test]
    fn test_rejects_escaped_quote_duplicate() {
        // "a\"b" escaped vs a"b direct (but direct " in key needs escaping anyway)
        // Test: {"a\\b":1, "a\u005Cb":2} where \u005C = backslash
        let json = r#"{"a\\b": 1, "a\u005Cb": 2}"#;
        let result = validate_json_strict(json);

        assert!(
            matches!(&result, Err(StrictJsonError::DuplicateKey { key, .. }) if key == "a\\b"),
            "Escaped backslash key must be detected as duplicate: {:?}",
            result
        );
    }

    #[test]
    fn test_accepts_valid_json() {
        let json = r#"{"key": "value"}"#;
        let result: TestStruct = from_str_strict(json).unwrap();
        assert_eq!(result.key, "value");
    }

    // === B2: Lone Surrogate Tests ===

    #[test]
    fn test_rejects_lone_high_surrogate() {
        // \uD800 is a high surrogate (must be followed by low surrogate)
        let json = r#"{"key": "\uD800"}"#;
        let result = validate_json_strict(json);

        assert!(matches!(result, Err(StrictJsonError::LoneSurrogate { .. })));
    }

    #[test]
    fn test_rejects_lone_low_surrogate() {
        // \uDC00 is a low surrogate (must follow high surrogate)
        let json = r#"{"key": "\uDC00"}"#;
        let result = validate_json_strict(json);

        assert!(matches!(result, Err(StrictJsonError::LoneSurrogate { .. })));
    }

    #[test]
    fn test_accepts_valid_surrogate_pair() {
        // Valid pair: \uD83D\uDE00 = 😀
        let json = r#"{"key": "\uD83D\uDE00"}"#;
        let result = validate_json_strict(json);
        assert!(result.is_ok());
    }

    #[test]
    fn test_rejects_reversed_surrogate_pair() {
        // Low then high is invalid
        let json = r#"{"key": "\uDC00\uD800"}"#;
        let result = validate_json_strict(json);

        assert!(matches!(result, Err(StrictJsonError::LoneSurrogate { .. })));
    }

    #[test]
    fn test_accepts_non_surrogate_unicode() {
        let json = r#"{"key": "\u0041"}"#; // 'A'
        let result = validate_json_strict(json);
        assert!(result.is_ok());
    }

    // === Edge Cases ===

    #[test]
    fn test_array_with_objects() {
        let json = r#"[{"key": "a", "key": "b"}]"#;
        let result = validate_json_strict(json);

        assert!(matches!(
            result,
            Err(StrictJsonError::DuplicateKey { key, .. })
            if key == "key"
        ));
    }

    #[test]
    fn test_complex_nested_structure() {
        let json = r#"{
            "manifest": {"version": "1.0"},
            "events": [
                {"type": "test", "data": {"a": 1}},
                {"type": "test", "data": {"b": 2}}
            ]
        }"#;
        let result = validate_json_strict(json);
        assert!(result.is_ok());
    }

    #[test]
    fn test_empty_objects_and_arrays() {
        let json = r#"{"empty_obj": {}, "empty_arr": []}"#;
        let result = validate_json_strict(json);
        assert!(result.is_ok());
    }

    #[test]
    fn test_signature_duplicate_key_attack() {
        // Real attack vector: signature with duplicate key_id
        let json = r#"{"signature": {"key_id": "legit", "key_id": "evil"}}"#;
        let result = validate_json_strict(json);

        assert!(matches!(
            result,
            Err(StrictJsonError::DuplicateKey { key, path })
            if key == "key_id" && path == "/signature"
        ));
    }

    // === DoS Protection Tests ===

    #[test]
    fn test_dos_nesting_depth_limit() {
        // Create JSON with 65 levels of nesting (exceeds MAX_NESTING_DEPTH=64)
        let deep_open = "{\"a\":".repeat(65);
        let deep_close = "}".repeat(65);
        let json = format!("{}1{}", deep_open, deep_close);

        let result = validate_json_strict(&json);
        assert!(
            matches!(result, Err(StrictJsonError::NestingTooDeep { depth: 65 })),
            "Expected NestingTooDeep error, got: {:?}",
            result
        );
    }

    #[test]
    fn test_dos_nesting_at_limit_ok() {
        // 64 levels should be accepted
        let deep_open = "{\"a\":".repeat(64);
        let deep_close = "}".repeat(64);
        let json = format!("{}1{}", deep_open, deep_close);

        let result = validate_json_strict(&json);
        assert!(result.is_ok(), "64 levels of nesting should be allowed");
    }

    #[test]
    fn test_dos_array_nesting_counts() {
        // Arrays also count towards nesting depth
        let deep_open = "[".repeat(65);
        let deep_close = "]".repeat(65);
        let json = format!("{}1{}", deep_open, deep_close);

        let result = validate_json_strict(&json);
        assert!(
            matches!(result, Err(StrictJsonError::NestingTooDeep { .. })),
            "Array nesting should count towards depth limit"
        );
    }

    // === Edge Cases: Whitespace & Escapes ===

    #[test]
    fn test_crlf_in_string_accepted() {
        // CRLF (\r\n) escaped in string value is valid JSON
        let json = r#"{"key": "line1\r\nline2"}"#;
        let result = validate_json_strict(json);
        assert!(result.is_ok(), "Escaped CRLF in string should be accepted");
    }

    #[test]
    fn test_whitespace_between_tokens() {
        // Various whitespace (space, tab, newline, CR) between tokens is valid
        let json = "{ \t\n\r\"key\" \t:\r\n \"value\" \t}";
        let result = validate_json_strict(json);
        assert!(
            result.is_ok(),
            "Whitespace between tokens should be accepted"
        );
    }

    #[test]
    fn test_many_unicode_escapes_in_string() {
        // Many unicode escapes should be handled without panic
        // This creates "aaaa..." (1000 'a' chars via escapes)
        let escapes = "\\u0061".repeat(1000);
        let json = format!(r#"{{"key": "{}"}}"#, escapes);
        let result = validate_json_strict(&json);
        assert!(result.is_ok(), "Many unicode escapes should be handled");
    }

    #[test]
    fn test_string_length_limit_on_decoded_content() {
        // String with many short escapes - total char count matters
        // MAX_STRING_LENGTH is 1MB, so this should be well under
        let escapes = "\\u0061".repeat(10000); // 10k 'a' chars
        let json = format!(r#"{{"key": "{}"}}"#, escapes);
        let result = validate_json_strict(&json);
        assert!(result.is_ok(), "10k escaped chars should be under limit");
    }

    #[test]
    fn test_mixed_escapes_in_key() {
        // Key with mixed literal and escaped content
        let json = r#"{"a\tb\nc\\d\"e": "value"}"#;
        let result = validate_json_strict(json);
        assert!(result.is_ok(), "Mixed escapes in key should be accepted");
    }

    #[test]
    fn test_all_standard_escapes() {
        // All JSON standard escapes: \", \\, \/, \b, \f, \n, \r, \t
        let json = r#"{"key": "\"\\/\b\f\n\r\t"}"#;
        let result = validate_json_strict(json);
        assert!(
            result.is_ok(),
            "All standard JSON escapes should be accepted"
        );
    }
}
