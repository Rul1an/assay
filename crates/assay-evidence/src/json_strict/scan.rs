//! JSON string scanning with unicode/surrogate decoding.

use std::str::CharIndices;

use super::{
    check_next, BoundedJsonError, BudgetDimension, BudgetHit, BudgetLimit, StrictJsonError,
    MAX_STRING_LENGTH,
};

/// Parse a JSON string using the same decoder for legacy and bounded validation.
pub(crate) fn parse_json_string<'a>(
    chars: &mut std::iter::Peekable<CharIndices<'a>>,
) -> Result<String, StrictJsonError> {
    match parse_json_string_bounded(chars, None, &mut |_| Ok(())) {
        Ok(value) => Ok(value),
        Err(BoundedJsonError::Strict(error)) => Err(error),
        Err(BoundedJsonError::Budget(_)) => unreachable!("legacy scan has no budget"),
    }
}

pub(crate) fn parse_json_string_bounded<'a>(
    chars: &mut std::iter::Peekable<CharIndices<'a>>,
    decoded_limit: Option<(BudgetLimit, BudgetDimension)>,
    on_raw_end: &mut dyn FnMut(usize) -> Result<(), BudgetHit>,
) -> Result<String, BoundedJsonError> {
    fn take<'a>(
        chars: &mut std::iter::Peekable<CharIndices<'a>>,
        on_raw_end: &mut dyn FnMut(usize) -> Result<(), BudgetHit>,
    ) -> Result<Option<(usize, char)>, BudgetHit> {
        let next = chars.peek().copied();
        if let Some((pos, c)) = next {
            on_raw_end(pos + c.len_utf8())?;
            chars.next();
        }
        Ok(next)
    }

    match take(chars, on_raw_end)? {
        Some((_, '"')) => {}
        Some((_, c)) => return Err(parse_error(&format!("expected '\"', got '{}'", c))),
        None => return Err(parse_error("unexpected end of input")),
    }

    let mut result = String::new();
    let mut prev_high_surrogate: Option<(usize, u32)> = None;
    let mut char_count = 0usize;
    let mut decoded_bytes = 0u64;

    fn push(
        result: &mut String,
        c: char,
        char_count: &mut usize,
        decoded_bytes: &mut u64,
        decoded_limit: Option<(BudgetLimit, BudgetDimension)>,
    ) -> Result<(), BoundedJsonError> {
        *char_count += 1;
        if let Some((limit, dimension)) = decoded_limit {
            *decoded_bytes = check_next(*decoded_bytes, c.len_utf8() as u64, limit, dimension)?;
        }
        if *char_count > MAX_STRING_LENGTH {
            return Err(StrictJsonError::StringTooLong {
                length: *char_count,
            }
            .into());
        }
        result.push(c);
        Ok(())
    }

    loop {
        match take(chars, on_raw_end)? {
            Some((_, '"')) => {
                if let Some((pos, _)) = prev_high_surrogate {
                    return Err(StrictJsonError::LoneSurrogate {
                        position: pos,
                        codepoint: "unpaired high surrogate at end of string".to_string(),
                    }
                    .into());
                }
                return Ok(result);
            }
            Some((pos, '\\')) => match take(chars, on_raw_end)? {
                Some((_, 'u')) => {
                    let (codepoint, hex) = parse_unicode_escape(chars, pos, on_raw_end)?;
                    if (0xD800..=0xDBFF).contains(&codepoint) {
                        if let Some((high_pos, _)) = prev_high_surrogate {
                            return Err(StrictJsonError::LoneSurrogate {
                                position: high_pos,
                                codepoint: "consecutive high surrogates".to_string(),
                            }
                            .into());
                        }
                        prev_high_surrogate = Some((pos, codepoint));
                    } else if (0xDC00..=0xDFFF).contains(&codepoint) {
                        let Some((_, high)) = prev_high_surrogate else {
                            return Err(StrictJsonError::LoneSurrogate {
                                position: pos,
                                codepoint: format!("\\u{hex}"),
                            }
                            .into());
                        };
                        let combined = 0x10000 + ((high - 0xD800) << 10) + (codepoint - 0xDC00);
                        let c = char::from_u32(combined).ok_or({
                            BoundedJsonError::Strict(StrictJsonError::InvalidUnicodeEscape {
                                position: pos,
                            })
                        })?;
                        push(
                            &mut result,
                            c,
                            &mut char_count,
                            &mut decoded_bytes,
                            decoded_limit,
                        )?;
                        prev_high_surrogate = None;
                    } else {
                        if let Some((high_pos, _)) = prev_high_surrogate {
                            return Err(StrictJsonError::LoneSurrogate {
                                position: high_pos,
                                codepoint: "high surrogate not followed by low".to_string(),
                            }
                            .into());
                        }
                        let c = char::from_u32(codepoint).ok_or({
                            BoundedJsonError::Strict(StrictJsonError::InvalidUnicodeEscape {
                                position: pos,
                            })
                        })?;
                        push(
                            &mut result,
                            c,
                            &mut char_count,
                            &mut decoded_bytes,
                            decoded_limit,
                        )?;
                    }
                }
                Some((_, c)) => {
                    if let Some((high_pos, _)) = prev_high_surrogate {
                        return Err(StrictJsonError::LoneSurrogate {
                            position: high_pos,
                            codepoint: "high surrogate not followed by low".to_string(),
                        }
                        .into());
                    }
                    let decoded = match c {
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        '\\' => '\\',
                        '/' => '/',
                        '"' => '"',
                        'b' => '\x08',
                        'f' => '\x0c',
                        _ => return Err(parse_error(&format!("invalid escape sequence \\{c}"))),
                    };
                    push(
                        &mut result,
                        decoded,
                        &mut char_count,
                        &mut decoded_bytes,
                        decoded_limit,
                    )?;
                }
                None => return Err(parse_error("unexpected end of escape")),
            },
            Some((_, c)) => {
                if let Some((high_pos, _)) = prev_high_surrogate {
                    return Err(StrictJsonError::LoneSurrogate {
                        position: high_pos,
                        codepoint: "high surrogate not followed by low".to_string(),
                    }
                    .into());
                }
                push(
                    &mut result,
                    c,
                    &mut char_count,
                    &mut decoded_bytes,
                    decoded_limit,
                )?;
            }
            None => return Err(parse_error("unterminated string")),
        }
    }
}

fn parse_unicode_escape<'a>(
    chars: &mut std::iter::Peekable<CharIndices<'a>>,
    start_pos: usize,
    on_raw_end: &mut dyn FnMut(usize) -> Result<(), BudgetHit>,
) -> Result<(u32, String), BoundedJsonError> {
    let mut hex = String::with_capacity(4);
    for _ in 0..4 {
        match chars.peek().copied() {
            Some((pos, c)) => {
                on_raw_end(pos + c.len_utf8())?;
                chars.next();
                if !c.is_ascii_hexdigit() {
                    return Err(StrictJsonError::InvalidUnicodeEscape {
                        position: start_pos,
                    }
                    .into());
                }
                hex.push(c);
            }
            _ => {
                return Err(StrictJsonError::InvalidUnicodeEscape {
                    position: start_pos,
                }
                .into())
            }
        }
    }
    let codepoint = u32::from_str_radix(&hex, 16).map_err(|_| {
        BoundedJsonError::Strict(StrictJsonError::InvalidUnicodeEscape {
            position: start_pos,
        })
    })?;
    Ok((codepoint, hex))
}

fn parse_error(message: &str) -> BoundedJsonError {
    BoundedJsonError::Strict(StrictJsonError::ParseError(
        serde_json::from_str::<()>(message).unwrap_err(),
    ))
}
