// WorkMode — Strategy 패턴으로 Operator(대화)와 Worker(작업)의 차이를 캡슐화.
//
// 공유: Rig<M>.process() → Agent.reply()
// 차이: 세션 관리 (영속 vs 작업당)

use goose::agents::SessionConfig;

/// Agent에게 전달되는 작업 단위.
pub struct WorkInput {
    pub text: String,
    pub work_id: Option<i64>,
    /// 미리 생성된 Goose 세션 ID. 설정 시 WorkMode.session_for()보다 우선.
    pub session_id: Option<String>,
}

impl WorkInput {
    pub fn chat(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            work_id: None,
            session_id: None,
        }
    }

    pub fn task(text: impl Into<String>, work_id: i64) -> Self {
        Self {
            text: text.into(),
            work_id: Some(work_id),
            session_id: None,
        }
    }

    /// 미리 생성된 세션 ID를 설정.
    pub fn with_session_id(mut self, id: String) -> Self {
        self.session_id = Some(id);
        self
    }
}

/// Strategy: 세션 관리 방식을 캡슐화.
///
/// ChatMode: 영속 세션 → prompt cache 보장.
/// TaskMode: 작업당 세션 → 대화 캐시 오염 방지.
pub trait WorkMode: Send + Sync {
    /// 이 입력에 사용할 Goose 세션 ID.
    fn session_for(&self, input: &WorkInput) -> String;

    /// SessionConfig 생성 (편의 메서드).
    fn session_config(&self, input: &WorkInput) -> SessionConfig {
        SessionConfig {
            id: self.session_for(input),
            schedule_id: None,
            max_turns: None,
            retry_config: None,
        }
    }
}

/// Operator용: 영속 세션. 같은 session_id를 반복 사용 → prompt cache 100% hit.
pub struct ChatMode {
    session_id: String,
}

impl ChatMode {
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
        }
    }
}

impl WorkMode for ChatMode {
    fn session_for(&self, _input: &WorkInput) -> String {
        self.session_id.clone()
    }
}

/// Worker용: 작업당 새 세션. 대화 캐시를 오염시키지 않음.
pub struct TaskMode;

impl WorkMode for TaskMode {
    fn session_for(&self, input: &WorkInput) -> String {
        // 미리 생성된 세션 ID가 있으면 우선 사용
        if let Some(id) = &input.session_id {
            return id.clone();
        }
        match input.work_id {
            Some(id) => format!("task-{id}"),
            None => format!(
                "task-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis()
            ),
        }
    }
}

/// Evolver용: stamp 분석당 세션. 대화 캐시 오염 방지.
pub struct EvolveMode;

impl WorkMode for EvolveMode {
    fn session_for(&self, input: &WorkInput) -> String {
        if let Some(id) = &input.session_id {
            return id.clone();
        }
        match input.work_id {
            Some(id) => format!("evolve-{id}"),
            None => format!(
                "evolve-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis()
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_mode_returns_same_session() {
        let mode = ChatMode::new("session-abc");
        let a = mode.session_for(&WorkInput::chat("hello"));
        let b = mode.session_for(&WorkInput::chat("world"));
        assert_eq!(a, b);
        assert_eq!(a, "session-abc");
    }

    #[test]
    fn task_mode_returns_unique_sessions() {
        let mode = TaskMode;
        let a = mode.session_for(&WorkInput::task("fix auth", 1));
        let b = mode.session_for(&WorkInput::task("add tests", 2));
        assert_eq!(a, "task-1");
        assert_eq!(b, "task-2");
        assert_ne!(a, b);
    }

    #[test]
    fn evolve_mode_returns_stamp_based_session() {
        let mode = EvolveMode;
        let a = mode.session_for(&WorkInput::task("analyze stamp", 5));
        assert_eq!(a, "evolve-5");
    }

    #[test]
    fn session_config_uses_session_for() {
        let mode = ChatMode::new("my-session");
        let config = mode.session_config(&WorkInput::chat("hi"));
        assert_eq!(config.id, "my-session");
    }

    #[test]
    fn task_mode_uses_presupplied_session_id() {
        let mode = TaskMode;
        let input = WorkInput::task("do something", 42).with_session_id("pre-session-xyz".into());
        let id = mode.session_for(&input);
        assert_eq!(id, "pre-session-xyz");
    }

    #[test]
    fn task_mode_generates_timestamp_session_when_no_work_id() {
        let mode = TaskMode;
        let input = WorkInput::chat("no work_id here");
        let id = mode.session_for(&input);
        assert!(
            id.starts_with("task-"),
            "expected 'task-' prefix, got: {id}"
        );
    }

    #[test]
    fn evolve_mode_generates_timestamp_session_when_no_work_id() {
        let mode = EvolveMode;
        let input = WorkInput::chat("no work_id");
        let id = mode.session_for(&input);
        assert!(
            id.starts_with("evolve-"),
            "expected 'evolve-' prefix, got: {id}"
        );
    }

    #[test]
    fn work_input_with_session_id_sets_field() {
        let input = WorkInput::chat("hello").with_session_id("my-sid".into());
        assert_eq!(input.session_id.as_deref(), Some("my-sid"));
    }

    #[test]
    fn evolve_mode_uses_presupplied_session_id() {
        let mode = EvolveMode;
        let input = WorkInput::chat("x").with_session_id("pre-evolve".into());
        assert_eq!(mode.session_for(&input), "pre-evolve");
    }
}
