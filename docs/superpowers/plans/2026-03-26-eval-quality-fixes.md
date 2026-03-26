# 코드 품질 평가 개선 — 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 코드 품질 평가에서 도출된 7개 개선 항목 해결 — evolver 크레이트 추출, ARCHITECTURE.md 동기화, 통합 테스트, doc-tests, 런타임 graceful degradation

**Architecture:** evolver 모듈(2,490 LOC)을 `opengoose-evolver` 크레이트로 추출하고, `AgentConfig`/`create_agent`를 `opengoose-rig`로 이동하여 순환 의존 없이 분리. Board→Worker 통합 테스트를 `opengoose-rig/tests/`에 추가. 핵심 공개 API에 doc-test 추가. Runtime의 Worker 실패를 graceful하게 처리.

**Tech Stack:** Rust, sea-orm, tokio, goose, ratatui

---

### Task 1: `AgentConfig` + `create_agent`를 `opengoose-rig`로 이동

evolver 추출의 전제 조건. 현재 `crates/opengoose/src/runtime.rs`에 있는 범용 Agent 생성 유틸을 rig 크레이트로 이동.

**Files:**
- Create: `crates/opengoose-rig/src/agent_factory.rs`
- Modify: `crates/opengoose-rig/src/lib.rs`
- Modify: `crates/opengoose-rig/Cargo.toml`
- Modify: `crates/opengoose/src/runtime.rs`

- [ ] **Step 1: `crates/opengoose-rig/src/agent_factory.rs` 생성**

`runtime.rs:50-112`의 `AgentConfig` struct와 `create_agent` 함수를 복사. `crate::` 참조를 제거하고 독립 모듈로 만든다.

```rust
// Agent creation utilities — shared by Operator, Worker, Evolver.

use anyhow::{Context, Result};
use goose::agents::Agent;
use goose::model::ModelConfig;
use goose::session::session_manager::SessionType;
use tracing::info;

pub struct AgentConfig {
    pub session_id: String,
    pub system_prompt: Option<String>,
}

/// Create a Goose Agent with the given config.
/// Reads GOOSE_PROVIDER and GOOSE_MODEL from the environment.
pub async fn create_agent(config: AgentConfig) -> Result<Agent> {
    let provider_name = std::env::var("GOOSE_PROVIDER").unwrap_or_else(|_| "anthropic".to_string());

    let agent = Agent::new();

    let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
    let session = agent
        .config
        .session_manager
        .create_session(
            cwd,
            config.session_id.clone(),
            SessionType::User,
            goose::config::goose_mode::GooseMode::Auto,
        )
        .await
        .context("failed to create session")?;

    let provider = match std::env::var("GOOSE_MODEL") {
        Ok(model_name) => {
            info!(
                provider = %provider_name,
                model = %model_name,
                session = %config.session_id,
                "creating agent"
            );
            let model_config = ModelConfig::new(&model_name)
                .context("invalid model config")?
                .with_canonical_limits(&provider_name);
            goose::providers::create(&provider_name, model_config, vec![]).await
        }
        Err(_) => {
            info!(
                provider = %provider_name,
                model = "default",
                session = %config.session_id,
                "creating agent"
            );
            goose::providers::create_with_default_model(&provider_name, vec![]).await
        }
    }
    .context("failed to create provider")?;

    agent
        .update_provider(provider, &session.id)
        .await
        .context("failed to set provider")?;

    if let Some(prompt) = config.system_prompt {
        agent
            .extend_system_prompt(config.session_id.clone(), prompt)
            .await;
    }

    Ok(agent)
}
```

- [ ] **Step 2: `opengoose-rig/src/lib.rs`에 모듈 등록**

```rust
pub mod agent_factory;
```

- [ ] **Step 3: `opengoose-rig/Cargo.toml`에 goose 의존성 확인**

goose는 이미 의존하고 있으므로 추가 불필요. 확인만.

- [ ] **Step 4: `opengoose/src/runtime.rs` 업데이트**

`AgentConfig`와 `create_agent` 함수 본문을 삭제하고 re-export으로 교체:

```rust
pub use opengoose_rig::agent_factory::{AgentConfig, create_agent};
```

`create_operator_agent`와 `create_worker_agent`는 바이너리 크레이트 고유이므로 그대로 유지 (시스템 프롬프트가 다름).

- [ ] **Step 5: 빌드 확인**

Run: `cargo check 2>&1 | tail -5`
Expected: `Finished` 성공

- [ ] **Step 6: 테스트 실행**

Run: `cargo nextest run 2>&1 | tail -5`
Expected: 전체 통과

- [ ] **Step 7: 커밋**

```bash
git add crates/opengoose-rig/src/agent_factory.rs crates/opengoose-rig/src/lib.rs crates/opengoose/src/runtime.rs
git commit -m "refactor: move AgentConfig + create_agent to opengoose-rig"
```

---

### Task 2: `opengoose-evolver` 크레이트 생성

evolver 모듈을 새 크레이트로 추출.

**Files:**
- Create: `crates/opengoose-evolver/Cargo.toml`
- Create: `crates/opengoose-evolver/src/lib.rs`
- Create: `crates/opengoose-evolver/src/loop_driver.rs`
- Create: `crates/opengoose-evolver/src/pipeline.rs`
- Create: `crates/opengoose-evolver/src/sweep.rs`
- Modify: `Cargo.toml` (workspace members)
- Modify: `crates/opengoose/Cargo.toml` (add opengoose-evolver dep)
- Delete: `crates/opengoose/src/evolver/` (entire directory)
- Modify: `crates/opengoose/src/runtime.rs`

- [ ] **Step 1: `crates/opengoose-evolver/Cargo.toml` 생성**

```toml
[package]
name = "opengoose-evolver"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
opengoose-board = { path = "../opengoose-board" }
opengoose-rig = { path = "../opengoose-rig" }
opengoose-skills = { path = "../opengoose-skills" }
goose = { workspace = true }
async-trait = { workspace = true }
anyhow = { workspace = true }
tokio = { workspace = true }
tracing = { workspace = true }
futures = "0.3"
serde_json = { workspace = true }
chrono = { workspace = true }
```

- [ ] **Step 2: workspace `Cargo.toml` 업데이트**

`members` 배열에 `"crates/opengoose-evolver"` 추가.
`[workspace.dependencies]`에 `opengoose-evolver = { path = "crates/opengoose-evolver" }` 추가.

- [ ] **Step 3: evolver 소스 파일 이동**

`crates/opengoose/src/evolver/` 4개 파일을 `crates/opengoose-evolver/src/`로 이동.
`mod.rs` → `lib.rs`로 리네임.

내부 참조 수정:
- `crate::runtime::{AgentConfig, create_agent}` → `opengoose_rig::agent_factory::{AgentConfig, create_agent}`
- `crate::skills::{evolve, load}` → `opengoose_skills` 직접 import로 교체
- `crate::skills::test_env_lock` → 새 크레이트 내부 `test_env_lock` 정의
- `super::` 참조 → `crate::` 참조로 변경

`lib.rs` 변경:

```rust
// Evolver — stamp_notify listener with lazy Agent init.
// Queries unprocessed low stamps, creates work items, analyzes with LLM.

mod loop_driver;
mod pipeline;
mod sweep;

use async_trait::async_trait;
use futures::StreamExt;
use goose::agents::{Agent, AgentEvent, SessionConfig};
use goose::conversation::message::Message;
use opengoose_rig::work_mode::evolve_session_id;

pub use loop_driver::run;

pub(crate) const EVOLVER_SYSTEM_PROMPT: &str = "You are a skill analyst for OpenGoose.\n\
     Analyze failed tasks and extract concrete, actionable lessons as SKILL.md files.\n\n\
     Rules:\n\
     - description MUST start with 'Use when...' (triggering conditions only)\n\
     - description must NOT summarize the skill's workflow\n\
     - Every lesson must be specific to THIS failure, not generic advice\n\
     - Include a 'Common Mistakes' table with specific rationalizations\n\
     - Include a 'Red Flags' list for self-checking\n\
     - If the lesson is something any competent agent already knows, output SKIP\n\
     - If an existing skill covers the same lesson, output UPDATE:{skill-name}\n\n\
     Output format: raw SKILL.md content with YAML frontmatter, OR 'SKIP', OR 'UPDATE:{name}'.";

pub(crate) const LOW_STAMP_THRESHOLD: f32 = 0.3;
const FALLBACK_SWEEP_SECS: u64 = 300;

#[async_trait]
pub(crate) trait AgentCaller: Send + Sync {
    async fn call(&self, prompt: &str, work_id: i64) -> anyhow::Result<String>;
}

struct RealAgentCaller<'a> {
    agent: &'a Agent,
}

#[async_trait]
impl AgentCaller for RealAgentCaller<'_> {
    async fn call(&self, prompt: &str, work_id: i64) -> anyhow::Result<String> {
        let message = Message::user().with_text(prompt);
        let session_config = SessionConfig {
            id: evolve_session_id(work_id),
            schedule_id: None,
            max_turns: None,
            retry_config: None,
        };

        let stream = self.agent.reply(message, session_config, None).await?;
        tokio::pin!(stream);

        let mut response_text = String::new();
        while let Some(event) = stream.next().await {
            match event {
                Ok(AgentEvent::Message(msg)) => {
                    use goose::conversation::message::MessageContent;
                    for content in &msg.content {
                        if let MessageContent::Text(t) = content {
                            response_text.push_str(&t.text);
                        }
                    }
                }
                Err(e) => return Err(e),
                _ => {}
            }
        }

        Ok(response_text)
    }
}

#[cfg(test)]
pub(crate) fn test_env_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    &LOCK
}
```

- [ ] **Step 4: `loop_driver.rs` 내부 참조 수정**

```rust
// 변경 전
use crate::runtime::{AgentConfig, create_agent};
// 변경 후
use opengoose_rig::agent_factory::{AgentConfig, create_agent};
```

`super::` 참조는 `crate::`로:
```rust
// 변경 전
use super::{EVOLVER_SYSTEM_PROMPT, FALLBACK_SWEEP_SECS, LOW_STAMP_THRESHOLD, RealAgentCaller};
// 변경 후
use crate::{EVOLVER_SYSTEM_PROMPT, FALLBACK_SWEEP_SECS, LOW_STAMP_THRESHOLD, RealAgentCaller};
```

- [ ] **Step 5: `pipeline.rs` 내부 참조 수정**

```rust
// 변경 전
use crate::skills::{evolve, load};
// 변경 후 (직접 import)
use opengoose_skills::evolution::parser::{EvolveAction, parse_evolve_response};
use opengoose_skills::evolution::prompts::{build_evolve_prompt, build_update_prompt, UpdatePromptParams, summarize_for_prompt};
use opengoose_skills::evolution::validator::validate_skill_output;
use opengoose_skills::evolution::writer::{WriteSkillParams, update_existing_skill, write_skill_to_rig_scope};
use opengoose_skills::loader::{LoadedSkill, load_skills};
use opengoose_skills::metadata::{is_effective, read_metadata};
```

`read_conversation_log` 함수를 이 크레이트 내에 정의:
```rust
fn read_conversation_log(work_item_id: i64) -> String {
    let session_id = format!("task-{work_item_id}");
    opengoose_rig::conversation_log::read_log(&session_id)
        .map(|content| summarize_for_prompt(&content, 4000))
        .unwrap_or_default()
}
```

테스트 내 `crate::skills::test_env_lock` → `crate::test_env_lock`.
테스트 내 `crate::skills::evolve::SkillMetadata` → `opengoose_skills::metadata::SkillMetadata`.

- [ ] **Step 6: `sweep.rs` 내부 참조 수정**

pipeline.rs와 동일한 패턴으로 `crate::skills::{evolve, load}` → 직접 import.
`crate::skills::test_env_lock` → `crate::test_env_lock`.

- [ ] **Step 7: 바이너리 크레이트 업데이트**

`crates/opengoose/Cargo.toml`에 `opengoose-evolver = { workspace = true }` 추가 (먼저 workspace deps에도 추가).

`crates/opengoose/src/runtime.rs`에서:
```rust
// 변경 전
use crate::evolver;
// ...
tokio::spawn(evolver::run(Arc::clone(&board), stamp_notify));

// 변경 후
tokio::spawn(opengoose_evolver::run(Arc::clone(&board), stamp_notify));
```

`crates/opengoose/src/evolver/` 디렉토리 전체 삭제.

`crates/opengoose/src/main.rs` (또는 lib.rs)에서 `mod evolver;` 제거.

- [ ] **Step 8: `crates/opengoose/src/skills/evolve.rs` 정리**

evolver가 사용하던 re-export들 중 바이너리 크레이트 내에서 더 이상 사용되지 않는 항목 제거. `read_conversation_log`은 evolver로 이동했으므로 삭제. 테스트에서만 사용하는 항목은 테스트 모듈 내부 import으로 변경.

- [ ] **Step 9: 빌드 확인**

Run: `cargo check 2>&1 | tail -5`
Expected: `Finished` 성공

- [ ] **Step 10: 테스트 실행**

Run: `cargo nextest run 2>&1 | tail -10`
Expected: 전체 통과 (evolver 테스트가 새 크레이트에서 실행됨)

- [ ] **Step 11: 커밋**

```bash
git add -A crates/opengoose-evolver/ Cargo.toml crates/opengoose/
git commit -m "refactor: extract opengoose-evolver crate (2.5k LOC)"
```

---

### Task 3: ARCHITECTURE.md 문서 동기화

**Files:**
- Modify: `docs/v0.2/ARCHITECTURE.md`

- [ ] **Step 1: 크레이트 수 수정 (18번 줄)**

```
// 변경 전
3. **4개 크레이트** — `opengoose`, `opengoose-board`, `opengoose-rig`, `opengoose-skills`.
// 변경 후
3. **6개 크레이트** — `opengoose`, `opengoose-board`, `opengoose-rig`, `opengoose-skills`, `opengoose-evolver`, `opengoose-sandbox` (실험적).
```

- [ ] **Step 2: 크레이트 구조 트리 (§3)에 evolver + sandbox 추가**

opengoose-skills 뒤에 추가:

```
│   ├── opengoose-evolver/               # Evolver — stamp 기반 스킬 자동 진화
│   │   └── src/
│   │       ├── lib.rs                   # AgentCaller trait, run() 진입점
│   │       ├── loop_driver.rs           # stamp_notify 대기 + lazy Agent init
│   │       ├── pipeline.rs              # stamp → LLM 분석 → 스킬 생성
│   │       └── sweep.rs                 # 주기적 미처리 stamp 스캔
│   │
│   └── opengoose-sandbox/               # 실험적 — microVM 샌드박스 (macOS HVF)
│       └── src/
│           ├── hypervisor/              # HVF (Apple Hypervisor.framework)
│           ├── boot.rs                  # VM 부팅 시퀀스
│           ├── machine.rs              # VM 머신 설정
│           ├── pool.rs                 # VM 풀 관리
│           ├── snapshot.rs             # CoW 스냅샷
│           ├── vm.rs                   # VM 라이프사이클
│           ├── uart.rs                # 시리얼 콘솔
│           ├── virtio.rs             # VirtIO 장치
│           └── initramfs.rs          # initramfs 빌더
```

- [ ] **Step 3: 의존성 그래프 (§3.1) 업데이트**

```
opengoose-board           (OpenGoose 의존성 없음. sea-orm, chrono, serde, tokio)
       ↑
opengoose-rig             (의존: board, goose)
       ↑
opengoose-evolver         (의존: board, rig, skills, goose)
       ↑
opengoose                 (의존: board, rig, skills, evolver — 바이너리)

opengoose-skills          (독립. board, rig, goose 의존 없음)
opengoose-sandbox         (독립. macOS 전용, HVF 의존)
```

- [ ] **Step 4: "하지 않는 것" 테이블 (§3.2)에 evolver + sandbox 행 추가**

| 크레이트 | 하지 않는 것 |
|----------|-------------|
| **evolver** | Board CRUD, 세션 관리, CLI/TUI, 직접 스킬 파일 I/O (opengoose-skills에 위임) |
| **sandbox** | LLM 호출, Board 접근, 네트워크, 플랫폼 추상화 (macOS HVF 전용) |

- [ ] **Step 5: "추가된 것" 테이블 (§12) 업데이트**

크레이트 수: `4개 (opengoose-skills 추가)` → `6개 (opengoose-evolver, opengoose-sandbox 추가)`

- [ ] **Step 6: 설계 결정 기록 섹션 추가 (문서 끝, §13 전)**

```markdown
## 13. 설계 결정 기록

### ADR-1: Board의 SQLite + CowStore 단일 소유

Board struct는 SQLite(영속성)와 CowStore(인메모리 브랜치/머지) 두 저장소를 소유한다.
`merge()` 메서드에서 staged clone → merge → persist → swap 4단계가 하나의 Mutex lock 안에서 실행되어 원자성을 보장한다.
persist 실패 시 swap이 안 일어남 → CowStore와 SQLite 일관성 자동 보장.
분리하면 이 원자성을 외부 호출자가 보장해야 하므로 동기화 버그 표면적이 증가한다.
재검토 시점: board.rs가 500줄을 넘거나, 저장소 백엔드를 교체할 필요가 생길 때.
```

기존 "열린 질문" 섹션 번호를 §14로 변경.

- [ ] **Step 7: 커밋**

```bash
git add docs/v0.2/ARCHITECTURE.md
git commit -m "docs: sync ARCHITECTURE.md — 6 crates, evolver + sandbox, ADR-1"
```

---

### Task 4: Board→Worker 통합 테스트

**Files:**
- Create: `crates/opengoose-rig/tests/worker_integration.rs`

- [ ] **Step 1: 테스트 파일 생성 — `post_claim_submit_lifecycle`**

```rust
//! Board → Worker integration tests.
//! Tests claim/submit/retry logic at the Board API level.
//! No LLM calls — pure state transitions.

use opengoose_board::Board;
use opengoose_board::work_item::{PostWorkItem, Priority, RigId, Status};
use std::sync::Arc;

fn post_req(title: &str) -> PostWorkItem {
    PostWorkItem {
        title: title.to_string(),
        description: format!("Description for {title}"),
        created_by: RigId::new("human"),
        priority: Priority::P1,
        tags: vec![],
    }
}

#[tokio::test]
async fn post_claim_submit_lifecycle() {
    let board = Board::in_memory().await.expect("board init should succeed");
    let worker_id = RigId::new("worker-1");
    board
        .register_rig("worker-1", "ai", None, None)
        .await
        .expect("register_rig should succeed");

    // Post
    let item = board.post(post_req("test task")).await.expect("post should succeed");
    assert_eq!(item.status, Status::Open);

    // Claim
    let claimed = board.claim(item.id, &worker_id).await.expect("claim should succeed");
    assert_eq!(claimed.status, Status::Claimed);
    assert_eq!(claimed.claimed_by.as_ref(), Some(&worker_id));

    // Submit
    board.submit(item.id, &worker_id).await.expect("submit should succeed");
    let done = board.get(item.id).await.expect("get should succeed").expect("item should exist");
    assert_eq!(done.status, Status::Done);
}
```

- [ ] **Step 2: `worker_skips_blocked_items` 테스트 추가**

```rust
#[tokio::test]
async fn worker_skips_blocked_items() {
    let board = Board::in_memory().await.expect("board init should succeed");

    let blocker = board.post(post_req("blocker")).await.expect("post should succeed");
    let blocked = board.post(post_req("blocked")).await.expect("post should succeed");
    board
        .add_dependency(blocker.id, blocked.id)
        .await
        .expect("add_dependency should succeed");

    let ready = board.ready().await.expect("ready should succeed");
    let ready_ids: Vec<i64> = ready.iter().map(|i| i.id).collect();

    assert!(ready_ids.contains(&blocker.id), "blocker should be ready");
    assert!(!ready_ids.contains(&blocked.id), "blocked item should NOT be ready");
}
```

- [ ] **Step 3: `worker_retries_then_stuck` 테스트 추가**

```rust
#[tokio::test]
async fn claim_then_mark_stuck() {
    let board = Board::in_memory().await.expect("board init should succeed");
    let worker_id = RigId::new("worker-1");
    board
        .register_rig("worker-1", "ai", None, None)
        .await
        .expect("register_rig should succeed");

    let item = board.post(post_req("failing task")).await.expect("post should succeed");
    board.claim(item.id, &worker_id).await.expect("claim should succeed");

    // Simulate bounded retry exhaustion → mark stuck
    board
        .mark_stuck(item.id, &worker_id)
        .await
        .expect("mark_stuck should succeed");

    let stuck = board.get(item.id).await.expect("get should succeed").expect("item should exist");
    assert_eq!(stuck.status, Status::Stuck);
}
```

- [ ] **Step 4: `concurrent_workers_no_double_claim` 테스트 추가**

```rust
#[tokio::test]
async fn concurrent_workers_no_double_claim() {
    let board = Arc::new(Board::in_memory().await.expect("board init should succeed"));
    let item = board.post(post_req("contested task")).await.expect("post should succeed");

    for i in 0..2 {
        board
            .register_rig(&format!("w-{i}"), "ai", None, None)
            .await
            .expect("register_rig should succeed");
    }

    let board1 = Arc::clone(&board);
    let board2 = Arc::clone(&board);

    let h1 = tokio::spawn(async move {
        board1.claim(item.id, &RigId::new("w-0")).await
    });
    let h2 = tokio::spawn(async move {
        board2.claim(item.id, &RigId::new("w-1")).await
    });

    let (r1, r2) = tokio::join!(h1, h2);
    let r1 = r1.expect("task should not panic");
    let r2 = r2.expect("task should not panic");

    // Exactly one should succeed, the other should fail with AlreadyClaimed
    let successes = [r1.is_ok(), r2.is_ok()];
    assert_eq!(
        successes.iter().filter(|&&s| s).count(),
        1,
        "exactly one worker should claim successfully"
    );
}
```

- [ ] **Step 5: 테스트 실행**

Run: `cargo nextest run -p opengoose-rig --test worker_integration 2>&1 | tail -10`
Expected: 4 tests passed

- [ ] **Step 6: 커밋**

```bash
git add crates/opengoose-rig/tests/worker_integration.rs
git commit -m "test: add Board→Worker integration tests (4 scenarios)"
```

---

### Task 5: Doc-tests — opengoose-board

**Files:**
- Modify: `crates/opengoose-board/src/board.rs`
- Modify: `crates/opengoose-board/src/work_item.rs`
- Modify: `crates/opengoose-board/src/beads.rs`

- [ ] **Step 1: `Board::in_memory` doc-test**

`board.rs`의 `pub async fn in_memory()` 위에:

```rust
/// Create an in-memory Board for testing.
///
/// # Examples
///
/// ```
/// # tokio::runtime::Runtime::new().unwrap().block_on(async {
/// let board = opengoose_board::Board::in_memory().await.unwrap();
/// let item = board.post(opengoose_board::work_item::PostWorkItem {
///     title: "Test".into(),
///     description: String::new(),
///     created_by: opengoose_board::work_item::RigId::new("human"),
///     priority: opengoose_board::work_item::Priority::P1,
///     tags: vec![],
/// }).await.unwrap();
/// assert_eq!(item.title, "Test");
/// # });
/// ```
```

- [ ] **Step 2: `RigId::new` + `RigId::try_new` doc-test**

`work_item.rs`의 `RigId::new` 위에:

```rust
/// Create a RigId without validation (panics on empty string via convention).
///
/// # Examples
///
/// ```
/// let id = opengoose_board::work_item::RigId::new("worker-1");
/// assert_eq!(id.to_string(), "worker-1");
/// ```
```

`RigId::try_new` 위에:

```rust
/// Create a RigId with validation.
///
/// # Examples
///
/// ```
/// use opengoose_board::work_item::RigId;
///
/// assert!(RigId::try_new("valid-id").is_ok());
/// assert!(RigId::try_new("").is_err());
/// assert!(RigId::try_new("has/slash").is_err());
/// assert!(RigId::try_new("has..dots").is_err());
/// ```
```

- [ ] **Step 3: `Status::precedence` doc-test**

`work_item.rs`의 `Status::precedence` 위에:

```rust
/// Merge precedence: higher value wins.
///
/// # Examples
///
/// ```
/// use opengoose_board::work_item::Status;
///
/// assert!(Status::Done.precedence() > Status::Open.precedence());
/// assert!(Status::Claimed.precedence() > Status::Open.precedence());
/// ```
```

- [ ] **Step 4: `filter_ready` doc-test**

`beads.rs`의 `filter_ready` 위에:

```rust
/// Filter open, unblocked items sorted by priority.
///
/// # Examples
///
/// ```
/// use opengoose_board::beads::filter_ready;
/// use opengoose_board::work_item::*;
/// use std::collections::HashSet;
///
/// let items = vec![WorkItem {
///     id: 1,
///     title: "task".into(),
///     description: String::new(),
///     created_by: RigId::new("human"),
///     created_at: chrono::Utc::now(),
///     status: Status::Open,
///     priority: Priority::P1,
///     tags: vec![],
///     claimed_by: None,
///     updated_at: chrono::Utc::now(),
/// }];
/// let ready = filter_ready(items.into_iter(), &HashSet::new());
/// assert_eq!(ready.len(), 1);
/// ```
```

- [ ] **Step 5: `prime_summary` doc-test**

`beads.rs`의 `prime_summary` 위에:

```rust
/// Build a compact summary of board state for agent context injection.
///
/// # Examples
///
/// ```
/// use opengoose_board::beads::prime_summary;
/// use opengoose_board::work_item::*;
///
/// let summary = prime_summary(&[], &RigId::new("worker"));
/// assert!(summary.contains("0 open"));
/// ```
```

- [ ] **Step 6: doc-test 실행**

Run: `cargo test --doc -p opengoose-board 2>&1 | tail -10`
Expected: doc-tests 통과

- [ ] **Step 7: 커밋**

```bash
git add crates/opengoose-board/src/board.rs crates/opengoose-board/src/work_item.rs crates/opengoose-board/src/beads.rs
git commit -m "docs: add doc-tests to opengoose-board core APIs"
```

---

### Task 6: Doc-tests — opengoose-rig + opengoose-skills

**Files:**
- Modify: `crates/opengoose-rig/src/work_mode.rs`
- Modify: `crates/opengoose-skills/src/catalog.rs`
- Modify: `crates/opengoose-skills/src/metadata.rs`

- [ ] **Step 1: `WorkMode` trait doc-test**

`work_mode.rs`의 `WorkMode` trait 위에 (실행 불가 — goose Agent 필요):

```rust
/// Strategy pattern for Rig session management.
///
/// # Examples
///
/// ```no_run
/// use opengoose_rig::work_mode::{TaskMode, WorkMode, WorkInput};
///
/// let input = WorkInput::task("implement feature X", 42);
/// let config = TaskMode.session_config(&input);
/// assert!(config.id.contains("task-42"));
/// ```
```

- [ ] **Step 2: skills 크레이트 doc-test**

실제 공개 API를 확인하여 `SkillMetadata`, `SkillCatalog` 등에 적절한 doc-test 추가. 실행 가능한 것만 (파일시스템 의존이 있으면 `no_run`).

- [ ] **Step 3: doc-test 실행**

Run: `cargo test --doc -p opengoose-rig -p opengoose-skills 2>&1 | tail -10`
Expected: doc-tests 통과

- [ ] **Step 4: 커밋**

```bash
git add crates/opengoose-rig/ crates/opengoose-skills/
git commit -m "docs: add doc-tests to opengoose-rig and opengoose-skills"
```

---

### Task 7: 런타임 Graceful Degradation

**Files:**
- Modify: `crates/opengoose/src/runtime.rs`
- Modify: `crates/opengoose/src/cli/commands.rs`

- [ ] **Step 1: `Runtime.worker`를 `Option`으로 변경**

`runtime.rs`:

```rust
pub struct Runtime {
    pub board: Arc<Board>,
    pub worker: Option<Arc<opengoose_rig::rig::Worker>>,
}
```

- [ ] **Step 2: `init_runtime`에서 Worker 실패를 graceful하게 처리**

```rust
pub async fn init_runtime(port: u16) -> Result<Runtime> {
    let board = Arc::new(Board::connect(&crate::db_url()).await?);
    web::spawn_server(Arc::clone(&board), port).await?;

    let stamp_notify = board.stamp_notify_handle();
    tokio::spawn(opengoose_evolver::run(Arc::clone(&board), stamp_notify));

    // Worker — graceful degradation on failure
    let worker = match create_worker_agent().await {
        Ok((worker_agent, _)) => {
            let worker = Arc::new(opengoose_rig::rig::Worker::new(
                RigId::new("worker"),
                Arc::clone(&board),
                worker_agent,
                opengoose_rig::work_mode::TaskMode,
                vec![
                    Arc::new(ContextHydrator {
                        skill_catalog: String::new(),
                    }),
                    Arc::new(ValidationGate),
                ],
            ));
            let worker_handle = Arc::clone(&worker);
            tokio::spawn(async move { worker_handle.run().await });
            Some(worker)
        }
        Err(e) => {
            tracing::warn!(error = %e, "worker agent creation failed, running without worker");
            None
        }
    };

    Ok(Runtime { board, worker })
}
```

- [ ] **Step 3: `commands.rs`에서 `Option<Worker>` 처리**

```rust
// 변경 전 (2곳)
rt.worker.cancel();

// 변경 후
if let Some(ref worker) = rt.worker {
    worker.cancel();
}
```

- [ ] **Step 4: 빌드 확인**

Run: `cargo check 2>&1 | tail -5`
Expected: `Finished` 성공

- [ ] **Step 5: 테스트 실행**

Run: `cargo nextest run 2>&1 | tail -5`
Expected: 전체 통과

- [ ] **Step 6: 커밋**

```bash
git add crates/opengoose/src/runtime.rs crates/opengoose/src/cli/commands.rs
git commit -m "fix: graceful degradation when worker agent creation fails"
```

---

### Task 8: 최종 검증

- [ ] **Step 1: 전체 빌드**

Run: `cargo check 2>&1 | tail -5`
Expected: `Finished` 성공

- [ ] **Step 2: Clippy**

Run: `cargo clippy --all-targets 2>&1 | tail -10`
Expected: 경고 0

- [ ] **Step 3: 전체 테스트**

Run: `cargo nextest run 2>&1 | tail -10`
Expected: 전체 통과

- [ ] **Step 4: Doc-tests**

Run: `cargo test --doc 2>&1 | tail -10`
Expected: doc-tests 통과

- [ ] **Step 5: LOC 확인**

Run: `for crate in opengoose opengoose-board opengoose-rig opengoose-skills opengoose-evolver opengoose-sandbox; do echo "=== $crate ==="; find crates/$crate/src -name '*.rs' | xargs wc -l 2>/dev/null | tail -1; done`
Expected: opengoose ~8.9k (was 11.4k), opengoose-evolver ~2.5k

- [ ] **Step 6: 최종 커밋 (필요시)**

변경 사항이 있으면 커밋.
