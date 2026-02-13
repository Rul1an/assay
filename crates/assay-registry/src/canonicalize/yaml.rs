//! YAML parsing and normalization for canonical pack format.

use serde_json::Value as JsonValue;

use super::errors::{
    CanonicalizeError, CanonicalizeResult, MAX_DEPTH, MAX_KEYS_PER_MAPPING, MAX_SAFE_INTEGER,
    MAX_STRING_LENGTH, MAX_TOTAL_SIZE, MIN_SAFE_INTEGER,
};

/// Parse YAML with strict validation per SPEC §6.1.
///
/// Validates:
/// - No anchors/aliases
/// - No tags
/// - No multi-document
/// - No duplicate keys
/// - No floats
/// - Integers within safe range
/// - DoS limits (depth, keys, string length, total size)
pub fn parse_yaml_strict(content: &str) -> CanonicalizeResult<JsonValue> {
    // Pre-check: input size
    if content.len() > MAX_TOTAL_SIZE {
        return Err(CanonicalizeError::InputTooLarge {
            size: content.len(),
        });
    }

    // Pre-scan for forbidden patterns
    pre_scan_yaml(content)?;

    // Parse YAML to intermediate value
    let yaml_value: serde_yaml::Value =
        serde_yaml::from_str(content).map_err(|e| CanonicalizeError::ParseError {
            message: e.to_string(),
        })?;

    // Convert to JSON and validate
    let json_value = yaml_to_json(&yaml_value, 0)?;

    Ok(json_value)
}

/// Pre-scan YAML for forbidden patterns.
///
/// This is a fast check before full parsing to reject obviously invalid input.
/// Uses a line-based approach to avoid false positives from string content.
fn pre_scan_yaml(content: &str) -> CanonicalizeResult<()> {
    // Track indentation levels and keys for duplicate detection
    // Key: (indent_level, key_name) -> seen
    let mut key_stack: Vec<(usize, std::collections::HashSet<String>)> =
        vec![(0, std::collections::HashSet::new())];

    for (line_num, line) in content.lines().enumerate() {
        // Skip empty lines and comments
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Calculate indentation (number of leading spaces)
        let indent = line.len() - line.trim_start().len();

        // Check for multi-document separator at line start
        if trimmed == "---" || trimmed.starts_with("--- ") || trimmed == "..." {
            return Err(CanonicalizeError::MultiDocumentFound);
        }

        // Check for anchors: &name at start of value (line-based, more conservative)
        // Pattern: key: &anchor or just &anchor as value
        if let Some(colon_pos) = trimmed.find(':') {
            let value_part = trimmed[colon_pos + 1..].trim_start();
            if value_part.starts_with('&') && value_part.len() > 1 {
                let next_char = value_part.chars().nth(1).unwrap_or(' ');
                if next_char.is_alphanumeric() || next_char == '_' {
                    return Err(CanonicalizeError::AnchorFound {
                        position: format!("line {}", line_num + 1),
                    });
                }
            }
            // Check for aliases: *name as value
            if value_part.starts_with('*') && value_part.len() > 1 {
                let next_char = value_part.chars().nth(1).unwrap_or(' ');
                if next_char.is_alphanumeric() || next_char == '_' {
                    return Err(CanonicalizeError::AliasFound {
                        position: format!("line {}", line_num + 1),
                    });
                }
            }
        }

        // Check for tags: !! or !<
        if trimmed.contains("!!") || trimmed.contains("!<") {
            // Make sure it's not inside a quoted string
            if !is_inside_quotes(trimmed, "!!") && !is_inside_quotes(trimmed, "!<") {
                let tag_start = trimmed.find("!!").or_else(|| trimmed.find("!<")).unwrap();
                let tag_end = trimmed[tag_start..]
                    .find(|c: char| c.is_whitespace() || c == ':')
                    .map(|p| tag_start + p)
                    .unwrap_or(trimmed.len().min(tag_start + 20));
                return Err(CanonicalizeError::TagFound {
                    tag: trimmed[tag_start..tag_end].to_string(),
                });
            }
        }

        // Duplicate key detection: extract key from mapping lines
        // A mapping line looks like: key: value or key:
        // List items (starting with -) create a new scope for each item
        let is_list_item = trimmed.starts_with('-');

        // For list items like "- key: value", extract key from the part after "-"
        let key_source = if is_list_item {
            trimmed
                .strip_prefix('-')
                .map(|s| s.trim_start())
                .unwrap_or("")
        } else {
            trimmed
        };

        // Handle scope changes for list items
        // Each list item gets its own fresh scope for keys, even at indent 0
        // This ensures `- a: 1\n- a: 2` is valid (different items) while
        // `- a: 1\n  a: 2` is invalid (duplicate in same item)
        if is_list_item {
            // Pop all scopes at or deeper than current indent (leaving parent scopes)
            while key_stack.len() > 1
                && key_stack.last().map(|(i, _)| *i >= indent).unwrap_or(false)
            {
                key_stack.pop();
            }

            // Always push a fresh scope for this list item
            // Use indent+1 as "effective indent" so each list item at same level
            // still gets its own scope (the +1 is just a marker, not real indent)
            key_stack.push((indent + 1, std::collections::HashSet::new()));
        }

        // Extract and check keys
        if let Some(key) = extract_yaml_key(key_source) {
            if !is_list_item {
                // Normal key: pop scopes that are strictly deeper than current indent
                while key_stack.len() > 1
                    && key_stack.last().map(|(i, _)| *i > indent).unwrap_or(false)
                {
                    key_stack.pop();
                }
                // If we're at a new indent level, push a new scope
                if key_stack.last().map(|(i, _)| *i < indent).unwrap_or(true) {
                    key_stack.push((indent, std::collections::HashSet::new()));
                }
            }

            // Check for duplicate at current level
            if let Some((_, keys)) = key_stack.last_mut() {
                if !keys.insert(key.clone()) {
                    return Err(CanonicalizeError::DuplicateKey { key });
                }
            }
        }
    }

    Ok(())
}

/// Check if a pattern appears inside quotes in a line.
fn is_inside_quotes(line: &str, pattern: &str) -> bool {
    if let Some(pos) = line.find(pattern) {
        let before = &line[..pos];
        // Count unescaped quotes before the pattern
        let double_quotes = before.matches('"').count() - before.matches("\\\"").count();
        let single_quotes = before.matches('\'').count() - before.matches("\\'").count();
        // If odd number of quotes, we're inside a string
        double_quotes % 2 == 1 || single_quotes % 2 == 1
    } else {
        false
    }
}

/// Extract a YAML mapping key from a line.
/// Returns None for non-mapping lines (arrays, scalars, etc.)
fn extract_yaml_key(line: &str) -> Option<String> {
    let trimmed = line.trim();

    // Skip array items
    if trimmed.starts_with('-') {
        return None;
    }

    // Skip block scalar indicators
    if trimmed == "|" || trimmed == ">" || trimmed == "|-" || trimmed == ">-" {
        return None;
    }

    // Find the colon that separates key from value
    // Handle quoted keys: "key": value or 'key': value
    if let Some(after_dquote) = trimmed.strip_prefix('"') {
        // Double-quoted key
        if let Some(end_quote) = after_dquote.find('"') {
            let key = &after_dquote[..end_quote];
            // Check there's a colon after the closing quote
            let after_key = &after_dquote[end_quote + 1..];
            if after_key.trim_start().starts_with(':') {
                return Some(key.to_string());
            }
        }
        return None;
    }

    if let Some(after_squote) = trimmed.strip_prefix('\'') {
        // Single-quoted key
        if let Some(end_quote) = after_squote.find('\'') {
            let key = &after_squote[..end_quote];
            let after_key = &after_squote[end_quote + 1..];
            if after_key.trim_start().starts_with(':') {
                return Some(key.to_string());
            }
        }
        return None;
    }

    // Unquoted key: find the first colon not inside brackets
    let mut depth: usize = 0;
    for (i, c) in trimmed.char_indices() {
        match c {
            '[' | '{' => depth += 1,
            ']' | '}' => depth = depth.saturating_sub(1),
            ':' if depth == 0 => {
                // Found the key separator
                let key = trimmed[..i].trim();
                if !key.is_empty() && !key.contains(' ') {
                    return Some(key.to_string());
                }
                return None;
            }
            _ => {}
        }
    }

    None
}

/// Convert YAML value to JSON value with validation.
fn yaml_to_json(yaml: &serde_yaml::Value, depth: usize) -> CanonicalizeResult<JsonValue> {
    // Check depth limit
    if depth > MAX_DEPTH {
        return Err(CanonicalizeError::MaxDepthExceeded { depth });
    }

    match yaml {
        serde_yaml::Value::Null => Ok(JsonValue::Null),

        serde_yaml::Value::Bool(b) => Ok(JsonValue::Bool(*b)),

        serde_yaml::Value::Number(n) => {
            // Check for float
            if n.is_f64() {
                return Err(CanonicalizeError::FloatNotAllowed {
                    value: n.to_string(),
                });
            }

            // Check integer range
            if let Some(i) = n.as_i64() {
                if !(MIN_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(&i) {
                    return Err(CanonicalizeError::IntegerOutOfRange { value: i });
                }
                Ok(JsonValue::Number(serde_json::Number::from(i)))
            } else if let Some(u) = n.as_u64() {
                if u > MAX_SAFE_INTEGER as u64 {
                    return Err(CanonicalizeError::IntegerOutOfRange { value: u as i64 });
                }
                Ok(JsonValue::Number(serde_json::Number::from(u)))
            } else {
                Err(CanonicalizeError::FloatNotAllowed {
                    value: n.to_string(),
                })
            }
        }

        serde_yaml::Value::String(s) => {
            // Check string length
            if s.len() > MAX_STRING_LENGTH {
                return Err(CanonicalizeError::StringTooLong { length: s.len() });
            }
            Ok(JsonValue::String(s.clone()))
        }

        serde_yaml::Value::Sequence(seq) => {
            let items: CanonicalizeResult<Vec<JsonValue>> = seq
                .iter()
                .map(|item| yaml_to_json(item, depth + 1))
                .collect();
            Ok(JsonValue::Array(items?))
        }

        serde_yaml::Value::Mapping(map) => {
            // Check key count
            if map.len() > MAX_KEYS_PER_MAPPING {
                return Err(CanonicalizeError::MaxKeysExceeded { count: map.len() });
            }

            let mut json_map = serde_json::Map::new();
            let mut seen_keys = std::collections::HashSet::new();

            for (key, value) in map {
                // Keys must be strings
                let key_str = match key {
                    serde_yaml::Value::String(s) => s.clone(),
                    _ => {
                        return Err(CanonicalizeError::ParseError {
                            message: format!("non-string key: {:?}", key),
                        })
                    }
                };

                // Check for duplicate keys
                if !seen_keys.insert(key_str.clone()) {
                    return Err(CanonicalizeError::DuplicateKey { key: key_str });
                }

                let json_value = yaml_to_json(value, depth + 1)?;
                json_map.insert(key_str, json_value);
            }

            Ok(JsonValue::Object(json_map))
        }

        // Tagged values are not allowed
        serde_yaml::Value::Tagged(tagged) => Err(CanonicalizeError::TagFound {
            tag: format!("{:?}", tagged.tag),
        }),
    }
}
