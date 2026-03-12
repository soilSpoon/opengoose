/// Sanitize a string for use as a filename or database key.
///
/// Replaces any character that isn't ASCII alphanumeric, `-`, or `_` with `_`.
/// Shared across crates to avoid duplicating sanitization logic.
pub fn sanitize_name(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}
