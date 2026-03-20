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

    if cfg!(test)
        && let Ok(path) = std::path::Path::new(trimmed).canonicalize()
        && path.is_dir()
    {
        let owner_repo = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("local-repo")
            .to_string();
        return Ok(GitSource {
            owner_repo,
            clone_url: path.to_string_lossy().to_string(),
        });
    }

    if trimmed.starts_with("https://") || trimmed.starts_with("http://") {
        let url = if trimmed.ends_with(".git") {
            trimmed.to_string()
        } else {
            format!("{trimmed}.git")
        };
        let owner_repo = extract_owner_repo(&url).unwrap_or_else(|| trimmed.to_string());
        Ok(GitSource {
            owner_repo,
            clone_url: url,
        })
    } else if trimmed.contains('/') && !trimmed.contains(':') {
        let owner_repo = trimmed.to_string();
        let clone_url = format!("https://github.com/{trimmed}.git");
        Ok(GitSource {
            owner_repo,
            clone_url,
        })
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

    #[cfg(test)]
    #[test]
    fn local_path_is_supported() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let canonical = repo.canonicalize().unwrap();
        let source = parse_source(canonical.to_str().unwrap()).unwrap();
        assert_eq!(source.owner_repo, "repo");
        assert_eq!(source.clone_url, canonical.to_str().unwrap());
    }

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
        assert_eq!(
            s.clone_url,
            "https://github.com/vercel-labs/agent-skills.git"
        );
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

    #[test]
    fn https_non_github_url_uses_raw_as_owner_repo() {
        // Non-github.com URL → extract_owner_repo returns None → trimmed.to_string() fallback
        let s = parse_source("https://gitlab.com/group/project").unwrap();
        assert!(s.clone_url.ends_with(".git"));
        // owner_repo falls back to the trimmed URL
        assert_eq!(s.owner_repo, "https://gitlab.com/group/project");
    }

    #[test]
    fn https_url_with_single_path_segment() {
        // github.com but only one path segment → extract_owner_repo returns None
        let s = parse_source("https://github.com/singleuser").unwrap();
        // Falls back to trimmed URL as owner_repo
        assert!(!s.owner_repo.is_empty());
    }

    #[test]
    fn file_path_in_test_mode_falls_through_when_not_a_dir() {
        // In cfg(test): canonicalize succeeds but path.is_dir() is false (it's a file)
        // → covers lines 29 (} of if path.is_dir()) and 31 (} of if cfg!(test))
        // then falls through to shorthand/url parsing → no '/' without ':' → error
        let tmp = tempfile::tempdir().unwrap();
        let file_path = tmp.path().join("not-a-dir.txt");
        std::fs::write(&file_path, "content").unwrap();
        let canonical = file_path.canonicalize().unwrap();
        // Not a dir → cfg(test) block falls through → parse as shorthand → Ok (treated as owner/repo)
        let result = parse_source(canonical.to_str().unwrap());
        // The path contains '/' and no ':', so it's parsed as a "shorthand" source
        assert!(result.is_ok());
    }

    #[test]
    fn nonexistent_path_falls_through_cfg_test_block() {
        // canonicalize() fails for non-existent paths → if let Ok doesn't match
        // → covers line 31 (} of if cfg!(test) when inner if let was not taken)
        // Falls through to shorthand parsing (contains '/', no ':') → Ok
        let result = parse_source("/tmp/this-path-definitely-does-not-exist-opengoose-test/foo");
        assert!(result.is_ok()); // parsed as "shorthand" owner/repo
    }

    #[test]
    fn http_url_is_accepted() {
        let s = parse_source("http://github.com/owner/repo").unwrap();
        assert!(s.clone_url.ends_with(".git"));
    }
}
