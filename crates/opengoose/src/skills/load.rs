// Skills Load — .opengoose/skills/ 스캔 + 카탈로그 생성
//
// 작업 시작 시 스킬 카탈로그를 빌드하여 시스템 프롬프트에 주입.
// 에이전트가 관련 스킬을 참조할 수 있도록 name + description 요약.

use std::path::{Path, PathBuf};

/// 로드된 스킬 요약.
#[derive(Debug, Clone)]
pub struct LoadedSkill {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
    pub content: String,
}

/// .opengoose/skills/ 에서 스킬 스캔 + 로드.
pub fn load_skills() -> Vec<LoadedSkill> {
    let home = dirs::home_dir().unwrap_or_else(|| ".".into());
    let skills_dir = home.join(".opengoose/skills");
    load_skills_from(&skills_dir)
}

/// 지정 디렉토리에서 스킬 로드.
pub fn load_skills_from(skills_dir: &Path) -> Vec<LoadedSkill> {
    if !skills_dir.is_dir() {
        return vec![];
    }

    let entries = match std::fs::read_dir(skills_dir) {
        Ok(e) => e,
        Err(_) => return vec![],
    };

    let mut skills = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let skill_md = path.join("SKILL.md");
        if !skill_md.is_file() {
            continue;
        }
        if let Ok(content) = std::fs::read_to_string(&skill_md) {
            if let Some((name, desc)) = parse_skill_header(&content) {
                skills.push(LoadedSkill {
                    name,
                    description: desc,
                    path: path.clone(),
                    content,
                });
            }
        }
    }

    skills
}

/// 카탈로그 문자열 생성 (시스템 프롬프트 주입용).
pub fn build_catalog(skills: &[LoadedSkill]) -> String {
    if skills.is_empty() {
        return String::new();
    }

    let mut catalog = String::from(
        "# Available Skills (auto-generated from past experience)\n\
         \n\
         These skills were learned from previous tasks. Reference them when relevant.\n\n",
    );

    for skill in skills {
        catalog.push_str(&format!("## {}\n", skill.name));
        catalog.push_str(&format!("{}\n\n", skill.description));

        // 본문에서 핵심 규칙 추출 (frontmatter 이후)
        if let Some(body) = extract_body(&skill.content) {
            let trimmed = body.trim();
            if !trimmed.is_empty() {
                // 최대 500자까지만 포함
                let summary = if trimmed.len() > 500 {
                    &trimmed[..500]
                } else {
                    trimmed
                };
                catalog.push_str(summary);
                catalog.push_str("\n\n");
            }
        }
    }

    catalog
}

fn parse_skill_header(content: &str) -> Option<(String, String)> {
    let content = content.trim_start();
    if !content.starts_with("---") {
        return None;
    }
    let rest = &content[3..];
    let end = rest.find("\n---")?;
    let frontmatter = &rest[..end];

    let mut name = None;
    let mut description = None;
    for line in frontmatter.lines() {
        if let Some(val) = line.strip_prefix("name:") {
            name = Some(val.trim().trim_matches('"').to_string());
        }
        if let Some(val) = line.strip_prefix("description:") {
            description = Some(val.trim().trim_matches('"').to_string());
        }
    }

    Some((name?, description?))
}

fn extract_body(content: &str) -> Option<&str> {
    let content = content.trim_start();
    if !content.starts_with("---") {
        return Some(content);
    }
    let rest = &content[3..];
    let end = rest.find("\n---")?;
    Some(&rest[end + 4..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_header_extracts_name_and_desc() {
        let content = "---\nname: test-skill\ndescription: A test skill\n---\n# Body\n";
        let (name, desc) = parse_skill_header(content).unwrap();
        assert_eq!(name, "test-skill");
        assert_eq!(desc, "A test skill");
    }

    #[test]
    fn extract_body_returns_content_after_frontmatter() {
        let content = "---\nname: x\n---\n# Body here\nMore content";
        let body = extract_body(content).unwrap();
        assert!(body.contains("Body here"));
    }

    #[test]
    fn build_catalog_formats_skills() {
        let skills = vec![LoadedSkill {
            name: "always-test".into(),
            description: "Always write tests".into(),
            path: PathBuf::from("/tmp/always-test"),
            content: "---\nname: always-test\ndescription: Always write tests\n---\n\nWrite unit tests for every function.\n".into(),
        }];
        let catalog = build_catalog(&skills);
        assert!(catalog.contains("always-test"));
        assert!(catalog.contains("Write unit tests"));
    }

    #[test]
    fn load_skills_from_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let skills = load_skills_from(tmp.path());
        assert!(skills.is_empty());
    }

    #[test]
    fn load_skills_from_populated_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("my-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: my-skill\ndescription: Do the thing\n---\n\nDetails here.\n",
        )
        .unwrap();

        let skills = load_skills_from(tmp.path());
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "my-skill");
    }
}
