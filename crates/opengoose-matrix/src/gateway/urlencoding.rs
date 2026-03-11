/// Percent-encode a string for use in a URL path segment.
/// Only characters outside [A-Za-z0-9\-_.~] are encoded.
pub fn encode(s: &str) -> Encoded {
    Encoded(
        s.bytes()
            .flat_map(|b| {
                if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
                    vec![b as char]
                } else {
                    format!("%{b:02X}").chars().collect()
                }
            })
            .collect(),
    )
}

pub struct Encoded(String);

impl Encoded {
    pub fn into_owned(self) -> String {
        self.0
    }
}
