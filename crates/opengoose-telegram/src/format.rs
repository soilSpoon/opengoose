/// Convert basic markdown to Telegram-compatible HTML.
///
/// Handles the most common patterns: bold, italic, code, links.
/// Follows goose's telegram_format.rs pattern.
pub fn markdown_to_telegram_html(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        match chars[i] {
            // Escape HTML special chars
            '&' => {
                result.push_str("&amp;");
                i += 1;
            }
            '<' => {
                result.push_str("&lt;");
                i += 1;
            }
            '>' => {
                result.push_str("&gt;");
                i += 1;
            }
            // Code blocks: ```...```
            '`' if i + 2 < len && chars[i + 1] == '`' && chars[i + 2] == '`' => {
                i += 3;
                // Skip optional language tag on same line
                while i < len && chars[i] != '\n' {
                    i += 1;
                }
                if i < len {
                    i += 1; // skip newline
                }
                let start = i;
                while i + 2 < len && !(chars[i] == '`' && chars[i + 1] == '`' && chars[i + 2] == '`') {
                    i += 1;
                }
                let code: String = chars[start..i].iter().collect();
                result.push_str("<pre>");
                result.push_str(&escape_html(&code));
                result.push_str("</pre>");
                if i + 2 < len {
                    i += 3; // skip closing ```
                }
            }
            // Inline code: `...`
            '`' => {
                i += 1;
                let start = i;
                while i < len && chars[i] != '`' {
                    i += 1;
                }
                let code: String = chars[start..i].iter().collect();
                result.push_str("<code>");
                result.push_str(&escape_html(&code));
                result.push_str("</code>");
                if i < len {
                    i += 1;
                }
            }
            // Bold: **...**
            '*' if i + 1 < len && chars[i + 1] == '*' => {
                i += 2;
                let start = i;
                while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '*') {
                    i += 1;
                }
                let content: String = chars[start..i].iter().collect();
                result.push_str("<b>");
                result.push_str(&content);
                result.push_str("</b>");
                if i + 1 < len {
                    i += 2;
                }
            }
            // Italic: *...*
            '*' => {
                i += 1;
                let start = i;
                while i < len && chars[i] != '*' {
                    i += 1;
                }
                let content: String = chars[start..i].iter().collect();
                result.push_str("<i>");
                result.push_str(&content);
                result.push_str("</i>");
                if i < len {
                    i += 1;
                }
            }
            c => {
                result.push(c);
                i += 1;
            }
        }
    }

    result
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_text() {
        assert_eq!(markdown_to_telegram_html("hello world"), "hello world");
    }

    #[test]
    fn test_bold() {
        assert_eq!(
            markdown_to_telegram_html("**bold**"),
            "<b>bold</b>"
        );
    }

    #[test]
    fn test_italic() {
        assert_eq!(
            markdown_to_telegram_html("*italic*"),
            "<i>italic</i>"
        );
    }

    #[test]
    fn test_inline_code() {
        assert_eq!(
            markdown_to_telegram_html("`code`"),
            "<code>code</code>"
        );
    }

    #[test]
    fn test_code_block() {
        assert_eq!(
            markdown_to_telegram_html("```rust\nfn main() {}\n```"),
            "<pre>fn main() {}\n</pre>"
        );
    }

    #[test]
    fn test_html_escaping() {
        assert_eq!(
            markdown_to_telegram_html("<script>&"),
            "&lt;script&gt;&amp;"
        );
    }

    #[test]
    fn test_code_html_escaping() {
        assert_eq!(
            markdown_to_telegram_html("`a < b & c > d`"),
            "<code>a &lt; b &amp; c &gt; d</code>"
        );
    }
}
