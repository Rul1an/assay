/// Return whether JSON structure exceeds `max_depth` before full deserialization.
///
/// JSONL treats each physical line as a separate document. Framed inputs and embedded SSE payloads
/// are single documents and must retain their structural depth across newlines.
pub(crate) fn exceeds_limit(bytes: &[u8], max_depth: usize, reset_at_newline: bool) -> bool {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for &byte in bytes {
        if byte == b'\n' {
            // Raw newlines cannot occur inside JSON strings. Recover here so malformed JSONL on one
            // row cannot hide structural depth on later rows from the resource guard.
            in_string = false;
            escaped = false;
            if reset_at_newline {
                depth = 0;
            }
            continue;
        }
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' | b'[' => {
                depth += 1;
                if depth > max_depth {
                    return true;
                }
            }
            b'}' | b']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    false
}
