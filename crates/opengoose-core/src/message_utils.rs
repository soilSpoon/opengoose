/// Split a text message into chunks that respect a maximum byte length.
///
/// Prefers splitting at newline boundaries for readability. Falls back to
/// splitting at the nearest UTF-8 character boundary when no newline is found.
pub fn split_message(text: &str, max_len: usize) -> Vec<&str> {
    if text.len() <= max_len {
        return vec![text];
    }
    let mut chunks = Vec::new();
    let mut remaining = text;
    while !remaining.is_empty() {
        if remaining.len() <= max_len {
            chunks.push(remaining);
            break;
        }
        let mut boundary = max_len;
        while !remaining.is_char_boundary(boundary) {
            boundary -= 1;
        }
        let split_at = remaining[..boundary].rfind('\n').unwrap_or(boundary);
        chunks.push(&remaining[..split_at]);
        remaining = remaining[split_at..].trim_start_matches('\n');
    }
    chunks
}

/// Truncate text to fit within `max_len` bytes at a valid UTF-8 boundary.
///
/// Used during streaming updates where partial content must fit within
/// platform message size limits.
pub fn truncate_for_display(text: &str, max_len: usize) -> &str {
    if text.len() <= max_len {
        return text;
    }
    let mut boundary = max_len;
    while !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    &text[..boundary]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_short_message() {
        assert_eq!(split_message("hello", 2000), vec!["hello"]);
    }

    #[test]
    fn test_split_exact_boundary() {
        let msg = "a".repeat(2000);
        let chunks = split_message(&msg, 2000);
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn test_split_at_newline() {
        let mut msg = "a".repeat(1900);
        msg.push('\n');
        msg.push_str(&"b".repeat(600));
        let chunks = split_message(&msg, 2000);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), 1900);
    }

    #[test]
    fn test_split_no_newline() {
        let msg = "a".repeat(2500);
        let chunks = split_message(&msg, 2000);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), 2000);
        assert_eq!(chunks[1].len(), 500);
    }

    #[test]
    fn test_split_utf8_safety() {
        let mut msg = "a".repeat(1999);
        msg.push('\u{1F600}'); // 4-byte emoji
        msg.push_str(&"b".repeat(100));
        let chunks = split_message(&msg, 2000);
        assert!(chunks.len() >= 2);
        for chunk in &chunks {
            assert!(chunk.is_ascii() || chunk.len() > 0);
        }
    }

    #[test]
    fn test_split_empty() {
        assert_eq!(split_message("", 2000), vec![""]);
    }

    #[test]
    fn test_truncate_short() {
        assert_eq!(truncate_for_display("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_long() {
        assert_eq!(truncate_for_display("hello world", 5), "hello");
    }

    #[test]
    fn test_truncate_utf8() {
        let text = "aaa\u{1F600}bbb"; // 3 + 4 + 3 = 10 bytes
        let result = truncate_for_display(text, 5);
        assert_eq!(result, "aaa"); // can't fit the emoji
    }
}
