// Skill Evolution — Stamp 기반 스킬 자동 추출
//
// 트리거: stamp의 아무 dimension에서 score < 0.3
// 입력: work_item 정보 + stamp(dimension, score, comment) + 대화 로그
// 출력: SKILL.md → .opengoose/skills/{name}/
// 연결: metadata.json에 stamp_id 기록

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// 스킬과 stamp의 연결 정보.
#[derive(Debug, Serialize, Deserialize)]
pub struct SkillMetadata {
    pub generated_from: GeneratedFrom,
    pub generated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GeneratedFrom {
    pub stamp_id: i64,
    pub work_item_id: i64,
    pub dimension: String,
    pub score: f32,
}

/// 낮은 score stamp에서 스킬 자동 추출 시도.
///
/// 반환: Ok(Some(skill_name)) — 스킬 생성됨
///       Ok(None) — 이미 유사 스킬 존재하여 스킵
///       Err — 파일 I/O 에러
pub fn try_evolve_skill(
    skills_dir: &Path,
    stamp_id: i64,
    work_item_id: i64,
    target_rig: &str,
    dimension: &str,
    score: f32,
    comment: Option<&str>,
) -> anyhow::Result<Option<String>> {
    // 스킬 이름 결정: dimension + work_item_id 기반
    let skill_name = generate_skill_name(dimension, work_item_id, comment);

    // 이미 존재하면 스킵
    let skill_path = skills_dir.join(&skill_name);
    if skill_path.exists() {
        return Ok(None);
    }

    // 대화 로그에서 컨텍스트 추출 시도
    let session_id = format!("task-{work_item_id}");
    let log_context = opengoose_rig::conversation_log::read_log(&session_id)
        .map(|content| summarize_log(&content))
        .unwrap_or_default();

    // SKILL.md 생성
    let lesson = build_lesson(dimension, score, comment, target_rig, &log_context);
    let skill_md = format!(
        "---\nname: {skill_name}\ndescription: {desc}\n---\n\n{lesson}\n",
        desc = build_description(dimension, comment),
    );

    // 디렉토리 생성 + 파일 저장
    std::fs::create_dir_all(&skill_path)?;
    std::fs::write(skill_path.join("SKILL.md"), &skill_md)?;

    // metadata.json 저장
    let metadata = SkillMetadata {
        generated_from: GeneratedFrom {
            stamp_id,
            work_item_id,
            dimension: dimension.to_string(),
            score,
        },
        generated_at: Utc::now().to_rfc3339(),
    };
    let metadata_json = serde_json::to_string_pretty(&metadata)?;
    std::fs::write(skill_path.join("metadata.json"), &metadata_json)?;

    Ok(Some(skill_name))
}

/// 스킬 이름 생성. comment가 있으면 그걸 기반으로, 없으면 dimension + id.
fn generate_skill_name(dimension: &str, work_item_id: i64, comment: Option<&str>) -> String {
    let base = if let Some(c) = comment {
        // 코멘트에서 키워드 추출 (공백→하이픈, 소문자, 20자 제한)
        let sanitized: String = c
            .chars()
            .take(30)
            .map(|ch| if ch.is_alphanumeric() { ch.to_ascii_lowercase() } else { '-' })
            .collect();
        let trimmed = sanitized.trim_matches('-').to_string();
        if trimmed.is_empty() {
            format!("{}-{}", dimension.to_lowercase(), work_item_id)
        } else {
            trimmed
        }
    } else {
        format!("{}-{}", dimension.to_lowercase(), work_item_id)
    };

    // 중복 하이픈 제거
    let mut result = String::new();
    let mut prev_hyphen = false;
    for ch in base.chars() {
        if ch == '-' {
            if !prev_hyphen {
                result.push(ch);
            }
            prev_hyphen = true;
        } else {
            result.push(ch);
            prev_hyphen = false;
        }
    }
    result
}

fn build_description(dimension: &str, comment: Option<&str>) -> String {
    match comment {
        Some(c) => format!("Lesson from low {dimension} score: {c}"),
        None => format!("Lesson from low {dimension} score"),
    }
}

fn build_lesson(
    dimension: &str,
    score: f32,
    comment: Option<&str>,
    target_rig: &str,
    log_context: &str,
) -> String {
    let mut lesson = format!(
        "# Lesson: {dimension} (score: {score:.1})\n\n\
         This skill was auto-generated because rig `{target_rig}` received a low score.\n\n"
    );

    if let Some(c) = comment {
        lesson.push_str(&format!("## Reviewer feedback\n\n{c}\n\n"));
    }

    lesson.push_str(&format!(
        "## What to do differently\n\n\
         When working on tasks related to {dimension}:\n"
    ));

    // dimension별 기본 가이드라인
    match dimension {
        "Quality" => {
            lesson.push_str(
                "- Write tests for all new code\n\
                 - Follow existing code conventions\n\
                 - Handle edge cases explicitly\n",
            );
        }
        "Reliability" => {
            lesson.push_str(
                "- Add error handling for all failure paths\n\
                 - Test with invalid inputs\n\
                 - Consider timeout and retry behavior\n",
            );
        }
        "Helpfulness" => {
            lesson.push_str(
                "- Provide clear commit messages\n\
                 - Document non-obvious decisions\n\
                 - Ask clarifying questions when requirements are ambiguous\n",
            );
        }
        _ => {
            lesson.push_str(&format!(
                "- Pay extra attention to {dimension} in future tasks\n"
            ));
        }
    }

    if !log_context.is_empty() {
        lesson.push_str(&format!("\n## Context from conversation\n\n{log_context}\n"));
    }

    lesson
}

/// 대화 로그에서 핵심 요약 추출 (최대 500자).
fn summarize_log(content: &str) -> String {
    // 마지막 몇 줄의 에러/경고 메시지를 추출
    let lines: Vec<&str> = content.lines().collect();
    let relevant: Vec<&str> = lines
        .iter()
        .rev()
        .take(20)
        .filter(|line| {
            line.contains("error") || line.contains("fail") || line.contains("warn")
                || line.contains("Error") || line.contains("Fail")
        })
        .copied()
        .collect();

    if relevant.is_empty() {
        // 마지막 5줄 반환
        lines
            .iter()
            .rev()
            .take(5)
            .rev()
            .copied()
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        relevant.into_iter().rev().collect::<Vec<_>>().join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_name_from_comment() {
        let name = generate_skill_name("Quality", 1, Some("테스트 없음"));
        assert!(!name.is_empty());
        assert!(!name.contains(' '));
    }

    #[test]
    fn skill_name_without_comment() {
        let name = generate_skill_name("Quality", 42, None);
        assert_eq!(name, "quality-42");
    }

    #[test]
    fn skill_name_no_double_hyphens() {
        let name = generate_skill_name("Quality", 1, Some("no   tests   written"));
        assert!(!name.contains("--"));
    }

    #[test]
    fn build_description_with_comment() {
        let desc = build_description("Quality", Some("no tests"));
        assert!(desc.contains("Quality"));
        assert!(desc.contains("no tests"));
    }

    #[test]
    fn try_evolve_creates_files() {
        let tmp = tempfile::tempdir().unwrap();
        let skills_dir = tmp.path();

        let result =
            try_evolve_skill(skills_dir, 1, 10, "worker-1", "Quality", 0.1, Some("no tests"));
        assert!(result.is_ok());
        let name = result.unwrap().unwrap();

        // SKILL.md 존재 확인
        let skill_md = skills_dir.join(&name).join("SKILL.md");
        assert!(skill_md.exists());

        // metadata.json 존재 확인
        let meta_path = skills_dir.join(&name).join("metadata.json");
        assert!(meta_path.exists());

        let meta: SkillMetadata =
            serde_json::from_str(&std::fs::read_to_string(&meta_path).unwrap()).unwrap();
        assert_eq!(meta.generated_from.stamp_id, 1);
        assert_eq!(meta.generated_from.work_item_id, 10);
    }

    #[test]
    fn try_evolve_skips_existing() {
        let tmp = tempfile::tempdir().unwrap();
        let skills_dir = tmp.path();

        // 첫 번째: 생성
        let r1 =
            try_evolve_skill(skills_dir, 1, 10, "w", "Quality", 0.1, Some("no tests"));
        assert!(r1.unwrap().is_some());

        // 두 번째: 스킵 (같은 comment → 같은 name)
        let r2 =
            try_evolve_skill(skills_dir, 2, 10, "w", "Quality", 0.1, Some("no tests"));
        assert!(r2.unwrap().is_none());
    }

    #[test]
    fn summarize_log_extracts_errors() {
        let log = "line1\nline2 error happened\nline3\nline4 failed\nline5\n";
        let summary = summarize_log(log);
        assert!(summary.contains("error") || summary.contains("failed"));
    }

    #[test]
    fn metadata_roundtrip() {
        let meta = SkillMetadata {
            generated_from: GeneratedFrom {
                stamp_id: 5,
                work_item_id: 12,
                dimension: "Quality".into(),
                score: 0.2,
            },
            generated_at: "2026-03-19T10:00:00Z".into(),
        };
        let json = serde_json::to_string(&meta).unwrap();
        let parsed: SkillMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.generated_from.stamp_id, 5);
    }
}
