//! JSON string scanning with unicode/surrogate decoding.
//!
//! Ensures keys are compared after JSON string unescaping so that
//! `"a"` and `"\u0061"` are correctly detected as duplicates.

use std::str::CharIndices;

use super::errors::{StrictJsonError, MAX_STRING_LENGTH};

/// Parse a JSON string from the iterator, consuming from the opening `"` to the closing `"`.
/// Returns the decoded content (with unicode escapes and surrogate pairs resolved).
pub(crate) fn parse_json_string<'a>(
    chars: &mut std::iter::Peekable<CharIndices<'a>>,
) -> Result<String, StrictJsonError> {
    // Consume opening quote
    match chars.next() {
        Some((_, '"')) => {}
        Some((_, c)) => {
            return Err(StrictJsonError::ParseError(
                serde_json::from_str::<()>(&format!("expected '\"', got '{}'", c)).unwrap_err(),
            ))
        }
        None => {
            return Err(StrictJsonError::ParseError(
                serde_json::from_str::<()>("unexpected end of input").unwrap_err(),
            ))
        }
    }

    let mut result = String::new();
    let mut prev_high_surrogate: Option<(usize, u32)> = None;
    let mut char_count = 0usize;

    // Increment decoded-char count and push; fail if over limit. Only call when adding output.
    #[inline]
    fn push_with_limit(
        result: &mut String,
        c: char,
        char_count: &mut usize,
    ) -> Result<(), StrictJsonError> {
        *char_count += 1;
        if *char_count > MAX_STRING_LENGTH {
            return Err(StrictJsonError::StringTooLong {
                length: *char_count,
            });
        }
        result.push(c);
        Ok(())
    }

    loop {
        match chars.next() {
            Some((_, '"')) => {
                if let Some((pos, _)) = prev_high_surrogate {
                    return Err(StrictJsonError::LoneSurrogate {
                        position: pos,
                        codepoint: "unpaired high surrogate at end of string".to_string(),
                    });
                }
                return Ok(result);
            }
            Some((pos, '\\')) => match chars.next() {
                Some((_, 'u')) => {
                    let (codepoint, hex_str) = parse_unicode_escape(chars, pos)?;

                    if (0xD800..=0xDBFF).contains(&codepoint) {
                        if let Some((high_pos, _)) = prev_high_surrogate {
                            return Err(StrictJsonError::LoneSurrogate {
                                position: high_pos,
                                codepoint: "consecutive high surrogates".to_string(),
                            });
                        }
                        prev_high_surrogate = Some((pos, codepoint));
                    } else if (0xDC00..=0xDFFF).contains(&codepoint) {
                        if let Some((_, high)) = prev_high_surrogate {
                            let combined = 0x10000 + ((high - 0xD800) << 10) + (codepoint - 0xDC00);
                            if let Some(c) = char::from_u32(combined) {
                                push_with_limit(&mut result, c, &mut char_count)?;
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
                        if let Some((high_pos, _)) = prev_high_surrogate {
                            return Err(StrictJsonError::LoneSurrogate {
                                position: high_pos,
                                codepoint: "high surrogate not followed by low".to_string(),
                            });
                        }
                        if let Some(c) = char::from_u32(codepoint) {
                            push_with_limit(&mut result, c, &mut char_count)?;
                        } else {
                            return Err(StrictJsonError::InvalidUnicodeEscape { position: pos });
                        }
                    }
                }
                Some((_, c)) => {
                    if let Some((high_pos, _)) = prev_high_surrogate {
                        return Err(StrictJsonError::LoneSurrogate {
                            position: high_pos,
                            codepoint: "high surrogate not followed by low".to_string(),
                        });
                    }
                    let decoded = match c {
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        '\\' => '\\',
                        '/' => '/',
                        '"' => '"',
                        'b' => '\x08',
                        'f' => '\x0C',
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
                    push_with_limit(&mut result, decoded, &mut char_count)?;
                }
                None => {
                    return Err(StrictJsonError::ParseError(
                        serde_json::from_str::<()>("unexpected end of escape").unwrap_err(),
                    ))
                }
            },
            Some((_, c)) => {
                if let Some((high_pos, _)) = prev_high_surrogate {
                    return Err(StrictJsonError::LoneSurrogate {
                        position: high_pos,
                        codepoint: "high surrogate not followed by low".to_string(),
                    });
                }
                push_with_limit(&mut result, c, &mut char_count)?;
            }
            None => {
                return Err(StrictJsonError::ParseError(
                    serde_json::from_str::<()>("unterminated string").unwrap_err(),
                ))
            }
        }
    }
}

fn parse_unicode_escape<'a>(
    chars: &mut std::iter::Peekable<CharIndices<'a>>,
    start_pos: usize,
) -> Result<(u32, String), StrictJsonError> {
    let mut hex = String::with_capacity(4);
    for _ in 0..4 {
        match chars.next() {
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
