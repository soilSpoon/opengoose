/// Parsed git source.
pub struct GitSource {
    /// Normalized "owner/repo"
    pub owner_repo: String,
    /// Full git clone URL
    pub clone_url: String,
}

/// Parse a source string into a GitSource.
///
/// Accepts:
/// - "owner/repo" → https://github.com/owner/repo.git
/// - "https://github.com/owner/repo" → as-is (append .git if needed)
pub fn parse_source(input: &str) -> anyhow::Result<GitSource> {
    let trimmed = input.trim().trim_end_matches('/');

    if trimmed.starts_with("https://") || trimmed.starts_with("http://") {
        let url = if trimmed.ends_with(".git") {
            trimmed.to_string()
        } else {
            format!("{trimmed}.git")
        };
        let owner_repo = extract_owner_repo(&url)
            .unwrap_or_else(|| trimmed.to_string());
        Ok(GitSource { owner_repo, clone_url: url })
    } else if trimmed.contains('/') && !trimmed.contains(':') {
        let owner_repo = trimmed.to_string();
        let clone_url = format!("https://github.com/{trimmed}.git");
        Ok(GitSource { owner_repo, clone_url })
    } else {
        anyhow::bail!("Invalid source: {input}. Use owner/repo or a full HTTPS URL.")
    }
}

fn extract_owner_repo(url: &str) -> Option<String> {
    let url = url.trim_end_matches(".git");
    if let Some(rest) = url.strip_prefix("https://github.com/") {
        let parts: Vec<&str> = rest.splitn(3, '/').collect();
        if parts.len() >= 2 {
            return Some(format!("{}/{}", parts[0], parts[1]));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shorthand() {
        let s = parse_source("anthropics/skills").unwrap();
        assert_eq!(s.owner_repo, "anthropics/skills");
        assert_eq!(s.clone_url, "https://github.com/anthropics/skills.git");
    }

    #[test]
    fn full_url() {
        let s = parse_source("https://github.com/vercel-labs/agent-skills").unwrap();
        assert_eq!(s.owner_repo, "vercel-labs/agent-skills");
        assert_eq!(s.clone_url, "https://github.com/vercel-labs/agent-skills.git");
    }

    #[test]
    fn full_url_with_git_suffix() {
        let s = parse_source("https://github.com/anthropics/skills.git").unwrap();
        assert_eq!(s.owner_repo, "anthropics/skills");
        assert_eq!(s.clone_url, "https://github.com/anthropics/skills.git");
    }

    #[test]
    fn trailing_slash_stripped() {
        let s = parse_source("anthropics/skills/").unwrap();
        assert_eq!(s.owner_repo, "anthropics/skills");
    }

    #[test]
    fn invalid_source() {
        assert!(parse_source("just-a-word").is_err());
    }
}
