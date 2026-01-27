/// Sanitize a string for safe terminal rendering.
///
/// Strips:
/// - ANSI ESC sequences (CSI `\x1b[...`, OSC `\x1b]...`, SS2/SS3)
/// - BEL character (`\x07`)
/// - OSC8 hyperlinks
/// - OSC52 clipboard sequences
/// - Control characters (0x00-0x1F except \n, \t) replaced with `\u{FFFD}`
///
/// Applies length cap (truncates with "..." if exceeded).
pub fn sanitize_terminal(input: &str) -> String {
    sanitize_terminal_with_limit(input, 200)
}

/// Sanitize with a custom length cap.
pub fn sanitize_terminal_with_limit(input: &str, max_chars: usize) -> String {
    let stripped = strip_escape_sequences(input);
    let cleaned = replace_control_chars(&stripped);

    if cleaned.chars().count() > max_chars {
        let truncated: String = cleaned.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{}...", truncated)
    } else {
        cleaned
    }
}

/// Strip all ANSI/terminal escape sequences from a string.
fn strip_escape_sequences(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == 0x1b {
            // ESC sequence
            i += 1;
            if i >= bytes.len() {
                break;
            }
            match bytes[i] {
                b'[' => {
                    // CSI sequence: ESC [ ... (ends at 0x40-0x7E)
                    i += 1;
                    while i < bytes.len() && !(0x40..=0x7E).contains(&bytes[i]) {
                        i += 1;
                    }
                    if i < bytes.len() {
                        i += 1; // skip final byte
                    }
                }
                b']' => {
                    // OSC sequence: ESC ] ... (ends at BEL or ST)
                    i += 1;
                    while i < bytes.len() {
                        if bytes[i] == 0x07 {
                            // BEL terminator
                            i += 1;
                            break;
                        }
                        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'\\' {
                            // ST terminator (ESC \)
                            i += 2;
                            break;
                        }
                        i += 1;
                    }
                }
                b'N' | b'O' => {
                    // SS2/SS3: skip next byte
                    i += 1;
                    if i < bytes.len() {
                        i += 1;
                    }
                }
                b'P' => {
                    // DCS: ESC P ... ST
                    i += 1;
                    while i < bytes.len() {
                        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'\\' {
                            i += 2;
                            break;
                        }
                        i += 1;
                    }
                }
                _ => {
                    // Unknown ESC sequence, skip one more byte
                    i += 1;
                }
            }
        } else if bytes[i] == 0x07 {
            // BEL — skip
            i += 1;
        } else {
            result.push(bytes[i]);
            i += 1;
        }
    }

    String::from_utf8_lossy(&result).to_string()
}

/// Replace control characters (0x00-0x1F except \n and \t) with U+FFFD.
fn replace_control_chars(input: &str) -> String {
    input
        .chars()
        .map(|c| {
            if c == '\n' || c == '\t' {
                c
            } else if c.is_control() && (c as u32) < 0x20 {
                '\u{FFFD}'
            } else {
                c
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_text_unchanged() {
        assert_eq!(sanitize_terminal("hello world"), "hello world");
    }

    #[test]
    fn test_csi_stripped() {
        // Bold text: ESC[1m ... ESC[0m
        assert_eq!(sanitize_terminal("\x1b[1mhello\x1b[0m"), "hello");
    }

    #[test]
    fn test_osc8_hyperlink_stripped() {
        // OSC 8 hyperlink: ESC]8;;url BEL text ESC]8;; BEL
        let input = "\x1b]8;;https://example.com\x07click here\x1b]8;;\x07";
        assert_eq!(sanitize_terminal(input), "click here");
    }

    #[test]
    fn test_osc52_clipboard_stripped() {
        // OSC 52: ESC]52;c;base64data BEL
        let input = "\x1b]52;c;SGVsbG8=\x07visible";
        assert_eq!(sanitize_terminal(input), "visible");
    }

    #[test]
    fn test_bel_stripped() {
        assert_eq!(sanitize_terminal("hello\x07world"), "helloworld");
    }

    #[test]
    fn test_control_chars_replaced() {
        assert_eq!(sanitize_terminal("hello\x00world"), "hello\u{FFFD}world");
        assert_eq!(sanitize_terminal("hello\x01world"), "hello\u{FFFD}world");
    }

    #[test]
    fn test_newline_tab_preserved() {
        assert_eq!(sanitize_terminal("hello\tworld\n"), "hello\tworld\n");
    }

    #[test]
    fn test_length_cap() {
        let long_input = "a".repeat(300);
        let result = sanitize_terminal(&long_input);
        assert!(result.len() <= 200);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_custom_limit() {
        let result = sanitize_terminal_with_limit("abcdefghij", 5);
        assert_eq!(result, "ab...");
    }

    #[test]
    fn test_ss2_ss3_stripped() {
        // SS2: ESC N + byte
        assert_eq!(sanitize_terminal("\x1bNAhello"), "hello");
        // SS3: ESC O + byte
        assert_eq!(sanitize_terminal("\x1bOBhello"), "hello");
    }

    #[test]
    fn test_st_terminated_osc() {
        // OSC terminated by ST (ESC \)
        let input = "\x1b]0;title\x1b\\visible";
        assert_eq!(sanitize_terminal(input), "visible");
    }
}
