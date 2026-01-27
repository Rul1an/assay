use assay_evidence::sanitize::{sanitize_terminal, sanitize_terminal_with_limit};

#[test]
fn test_osc8_hyperlink() {
    let input = "\x1b]8;;https://evil.com\x07Click me\x1b]8;;\x07";
    let result = sanitize_terminal(input);
    assert_eq!(result, "Click me");
    assert!(!result.contains("evil.com"));
}

#[test]
fn test_osc52_clipboard() {
    let input = "\x1b]52;c;SGVsbG8gV29ybGQ=\x07visible text";
    let result = sanitize_terminal(input);
    assert_eq!(result, "visible text");
    assert!(!result.contains("SGVsbG8"));
}

#[test]
fn test_bel_stripped() {
    let result = sanitize_terminal("before\x07after");
    assert_eq!(result, "beforeafter");
}

#[test]
fn test_csi_color_codes() {
    let result = sanitize_terminal("\x1b[31mred\x1b[0m normal");
    assert_eq!(result, "red normal");
}

#[test]
fn test_control_chars_replaced() {
    // NUL, SOH, STX replaced with replacement char
    let result = sanitize_terminal("a\x00b\x01c\x02d");
    assert_eq!(result, "a\u{FFFD}b\u{FFFD}c\u{FFFD}d");
}

#[test]
fn test_newline_tab_preserved() {
    let result = sanitize_terminal("line1\nline2\tcol");
    assert_eq!(result, "line1\nline2\tcol");
}

#[test]
fn test_length_cap_default() {
    let long_input = "x".repeat(300);
    let result = sanitize_terminal(&long_input);
    assert!(result.chars().count() <= 200);
    assert!(result.ends_with("..."));
}

#[test]
fn test_length_cap_custom() {
    let result = sanitize_terminal_with_limit("hello world", 8);
    assert_eq!(result, "hello...");
}

#[test]
fn test_malicious_subject() {
    // Combine multiple attack vectors
    let input =
        "\x1b]52;c;AAAA\x07\x1b]8;;https://evil.com\x07\x1b[31m\x00payload\x1b[0m\x1b]8;;\x07";
    let result = sanitize_terminal(input);
    // Should contain only "payload" with the NUL replaced
    assert!(!result.contains("evil.com"));
    assert!(!result.contains("AAAA"));
    assert!(result.contains("\u{FFFD}payload"));
}

#[test]
fn test_empty_string() {
    assert_eq!(sanitize_terminal(""), "");
}

#[test]
fn test_normal_text_unchanged() {
    let input = "This is a perfectly normal subject string with no escapes.";
    assert_eq!(sanitize_terminal(input), input);
}

#[test]
fn test_unicode_preserved() {
    let input = "Hello 世界 🌍";
    assert_eq!(sanitize_terminal(input), input);
}

#[test]
fn test_dcs_sequence_stripped() {
    // DCS: ESC P ... ST (ESC \)
    let input = "\x1bPsome;data\x1b\\visible";
    let result = sanitize_terminal(input);
    assert_eq!(result, "visible");
}
