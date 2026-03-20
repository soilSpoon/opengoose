# Worker Pull Loop 통합 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Board를 실제 오케스트레이터로 만들기 — Worker pull loop를 스폰하고, TUI/headless 모드를 Operator/Worker 추상화로 전환.

**Architecture:** main.rs에서 raw Agent 사용을 제거하고 Operator(채팅) + Worker(풀 루프)로 교체. Agent 생성 로직을 `create_base_agent()`로 추출하여 중복 제거. TUI는 `Arc<Operator>`를 받고, headless 모드는 Board notify로 Worker 완료를 대기.

**Tech Stack:** Rust, tokio, Goose Agent, SeaORM (SQLite), ratatui

**Spec:** `docs/superpowers/specs/2026-03-20-worker-pull-loop-design.md`

---

### Task 1: Worker 실패 시 abandon + Result<bool> 반환

가장 독립적이고 단순한 변경. rig.rs만 수정.

**Files:**
- Modify: `crates/opengoose-rig/src/rig.rs:142-191` (Worker impl)

- [ ] **Step 1: `try_claim_and_execute` 시그니처를 `Result<bool>`로 변경하고 에러 핸들링 추가**

```rust
/// Board에서 가장 높은 우선순위 작업을 가져가서 실행.
/// Ok(true) = 작업 수행 완료, Ok(false) = 대기할 작업 없음.
async fn try_claim_and_execute(&self) -> anyhow::Result<bool> {
    let board_arc = self.board.as_ref().expect("Worker must have a board");
    let ready = board_arc.ready().await?;

    let Some(item) = ready.first() else {
        return Ok(false);
    };

    let item = board_arc.claim(item.id, &self.id).await?;
    info!(rig = %self.id, item_id = item.id, title = %item.title, "claimed work item");

    // Strategy가 세션 ID를 결정 (TaskMode: "task-{id}")
    let input = WorkInput::task(
        format!("Work item #{}: {}\n\n{}", item.id, item.title, item.description),
        item.id,
    );

    let result = self.process(input).await;
    if let Err(e) = &result {
        warn!(rig = %self.id, item_id = item.id, error = %e, "execution failed, abandoning");
        board_arc.abandon(item.id).await.ok();
    } else {
        board_arc.submit(item.id, &self.id).await?;
        info!(rig = %self.id, item_id = item.id, "submitted work item");
    }

    result.map(|()| true)
}
```

- [ ] **Step 2: 빌드 확인**

Run: `cargo check -p opengoose-rig`
Expected: success (내부 호출자가 rig.rs 안에만 있으므로)

- [ ] **Step 3: 커밋**

```bash
git add crates/opengoose-rig/src/rig.rs
git commit -m "fix: abandon work item on Worker process failure, return Result<bool>"
```

---

### Task 2: register-before-check 패턴으로 Worker::run() 개선

Task 1에 의존 (try_claim_and_execute가 Result<bool> 반환).

**Files:**
- Modify: `crates/opengoose-rig/src/rig.rs:142-164` (Worker::run)

- [ ] **Step 1: Worker::run()을 register-before-check 패턴으로 재작성**

```rust
pub async fn run(&self) {
    let Some(board) = &self.board else {
        warn!(rig = %self.id, "worker has no board, exiting");
        return;
    };
    info!(rig = %self.id, "worker started, waiting for work");

    loop {
        // 1. 관심 먼저 등록 — 이 시점 이후의 모든 알림을 캡처
        let notified = board.notify_handle().notified();

        // 2. 준비된 항목 확인 + 실행
        match self.try_claim_and_execute().await {
            Ok(true) => continue,  // 작업 발견, 즉시 추가 확인
            Ok(false) => {}        // 작업 없음, 대기로 이동
            Err(e) => warn!(rig = %self.id, error = %e, "execution failed"),
        }

        // 3. 작업 없음 — 알림 대기 (손실 불가능)
        tokio::select! {
            _ = notified => {}
            _ = self.cancel.cancelled() => {
                info!(rig = %self.id, "worker cancelled");
                break;
            }
        }
    }
}
```

핵심 변경: `notify_handle().notified()`를 `try_claim_and_execute()` **전에** 등록. 이렇게 하면 실행 중 Board에 새 작업이 post되더라도 알림을 놓치지 않음.

- [ ] **Step 2: 빌드 확인**

Run: `cargo check -p opengoose-rig`
Expected: success

- [ ] **Step 3: 기존 테스트 통과 확인**

Run: `cargo test`
Expected: 10/10 pass

- [ ] **Step 4: 커밋**

```bash
git add crates/opengoose-rig/src/rig.rs
git commit -m "fix: register-before-check pattern in Worker::run() to prevent notify loss"
```

---

### Task 3: Agent 생성 로직 추출 — create_base_agent

main.rs에서 `create_agent()`를 분해.

**Files:**
- Modify: `crates/opengoose/src/main.rs:331-384` (create_agent → create_base_agent + create_operator_agent + create_worker_agent)

- [ ] **Step 1: `create_base_agent()` 추출**

`create_agent()`의 공통 부분을 추출. 시스템 프롬프트 확장은 포함하지 않음.

```rust
/// 공통 Agent 생성. session_name으로 세션 구분.
async fn create_base_agent(session_name: &str) -> Result<(Agent, String)> {
    let provider_name =
        std::env::var("GOOSE_PROVIDER").unwrap_or_else(|_| "anthropic".to_string());

    let agent = Agent::new();

    let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
    let session = agent
        .config
        .session_manager
        .create_session(cwd, session_name.into(), SessionType::User)
        .await
        .context("failed to create session")?;

    let provider = match std::env::var("GOOSE_MODEL") {
        Ok(model_name) => {
            info!(provider = %provider_name, model = %model_name, session = %session_name, "creating agent");
            let model_config = ModelConfig::new(&model_name)
                .context("invalid model config")?
                .with_canonical_limits(&provider_name);
            goose::providers::create(&provider_name, model_config, vec![]).await
        }
        Err(_) => {
            info!(provider = %provider_name, model = "default", session = %session_name, "creating agent");
            goose::providers::create_with_default_model(&provider_name, vec![]).await
        }
    }
    .context("failed to create provider")?;

    agent
        .update_provider(provider, &session.id)
        .await
        .context("failed to set provider")?;

    Ok((agent, session.id))
}
```

- [ ] **Step 2: `create_operator_agent()` 래퍼 작성**

```rust
/// Operator Agent 생성. Board 상태 조회 + 태스크 포스팅 프롬프트 포함.
/// 참고: Worker가 Board 태스크를 자동 처리하므로, Operator는 claim/submit 하지 않음.
async fn create_operator_agent() -> Result<(Agent, String)> {
    let (agent, session_id) = create_base_agent("opengoose").await?;
    agent
        .extend_system_prompt(
            "opengoose".to_string(),
            "You are an OpenGoose Operator rig — you handle interactive conversation.\n\
             A separate Worker rig automatically claims and executes Board tasks.\n\n\
             Available commands (run via shell):\n\
             - opengoose board status    — show board state (open/claimed/done)\n\
             - opengoose board ready     — list claimable work items\n\
             - opengoose board create \"TITLE\" — post a new task\n\
             \n\
             When the user posts a task via /task, it goes to the Board and the Worker picks it up automatically.\n\
             You do NOT need to claim or submit tasks yourself."
                .to_string(),
        )
        .await;
    Ok((agent, session_id))
}
```

- [ ] **Step 3: `create_worker_agent()` 래퍼 작성**

```rust
/// Worker Agent 생성. 태스크 자율 실행 프롬프트 포함.
async fn create_worker_agent() -> Result<(Agent, String)> {
    let (agent, session_id) = create_base_agent("worker").await?;
    agent
        .extend_system_prompt(
            "worker".to_string(),
            "You are an OpenGoose Worker rig. You receive tasks from the Board and execute them autonomously.\n\
             Focus on completing the task. Use available tools. Do not ask clarifying questions — make reasonable assumptions and proceed."
                .to_string(),
        )
        .await;
    Ok((agent, session_id))
}
```

- [ ] **Step 4: 기존 `create_agent()`를 삭제하고 호출부 갱신**

main.rs의 `main()` 함수에서:
- `Commands::Run` 분기: `create_agent()` → `create_operator_agent()` (이 Task에서는 아직 headless 변경 안 함)
- `None` (TUI) 분기: `create_agent()` → `create_operator_agent()`

```rust
Some(Commands::Run { task }) => {
    // ... board, web, evolver 스폰 ...
    let (agent, session_id) = create_operator_agent().await?;
    run_headless(&board, &agent, &session_id, &task).await
}
None => {
    // ... board, web, evolver 스폰 ...
    let (agent, session_id) = create_operator_agent().await?;
    let agent = Arc::new(agent);
    tui::run_tui(board, agent, session_id).await
}
```

- [ ] **Step 5: 빌드 확인**

Run: `cargo check -p opengoose`
Expected: success

- [ ] **Step 6: 커밋**

```bash
git add crates/opengoose/src/main.rs
git commit -m "refactor: extract create_base_agent, create_operator_agent, create_worker_agent"
```

---

### Task 4: Worker 스폰 — main.rs에서 Worker pull loop 시작

Task 3에 의존 (create_worker_agent 존재).

**Files:**
- Modify: `crates/opengoose/src/main.rs:128-174` (main 함수)

- [ ] **Step 1: TUI 분기에 Worker 스폰 추가**

`None` 분기 (TUI 모드)에서:

```rust
None => {
    let board = Arc::new(Board::connect(&db_url()).await?);
    web::spawn_server(Arc::clone(&board), cli.port).await?;
    // Spawn Evolver
    let stamp_notify = board.stamp_notify_handle();
    tokio::spawn(evolver::run(Arc::clone(&board), stamp_notify));
    // Spawn Worker
    let (worker_agent, _) = create_worker_agent().await?;
    let worker = Arc::new(opengoose_rig::rig::Worker::new(
        RigId::new("worker"),
        Arc::clone(&board),
        worker_agent,
        opengoose_rig::work_mode::TaskMode,
    ));
    let worker_handle = Arc::clone(&worker);
    tokio::spawn(async move { worker_handle.run().await });
    // Operator (chat)
    let (agent, session_id) = create_operator_agent().await?;
    let agent = Arc::new(agent);
    let result = tui::run_tui(board, agent, session_id).await;
    worker.cancel();
    result
}
```

- [ ] **Step 2: Run 분기에도 Worker 스폰 추가**

```rust
Some(Commands::Run { task }) => {
    let board = Arc::new(Board::connect(&db_url()).await?);
    web::spawn_server(Arc::clone(&board), cli.port).await?;
    // Spawn Evolver
    let stamp_notify = board.stamp_notify_handle();
    tokio::spawn(evolver::run(Arc::clone(&board), stamp_notify));
    // Spawn Worker
    let (worker_agent, _) = create_worker_agent().await?;
    let worker = Arc::new(opengoose_rig::rig::Worker::new(
        RigId::new("worker"),
        Arc::clone(&board),
        worker_agent,
        opengoose_rig::work_mode::TaskMode,
    ));
    let worker_handle = Arc::clone(&worker);
    tokio::spawn(async move { worker_handle.run().await });
    // Headless
    let result = run_headless(&board, &task).await;
    worker.cancel();
    result
}
```

**주의:** `run_headless`의 시그니처를 임시로 변경 — agent/session_id 파라미터 제거는 Task 6에서 수행. 지금은 컴파일 에러가 나므로 기존 시그니처를 유지하고 dummy agent를 전달하거나, 아래 방식으로 처리:

실제로는 Task 6 (headless 변경)과 묶어서 진행하는 것이 자연스러움. 따라서 이 step에서는 **TUI 분기만** Worker 스폰을 추가하고, Run 분기는 Task 6에서 함께 변경.

수정:
```rust
None => {
    let board = Arc::new(Board::connect(&db_url()).await?);
    web::spawn_server(Arc::clone(&board), cli.port).await?;
    let stamp_notify = board.stamp_notify_handle();
    tokio::spawn(evolver::run(Arc::clone(&board), stamp_notify));
    // Spawn Worker
    let (worker_agent, _) = create_worker_agent().await?;
    let worker = Arc::new(opengoose_rig::rig::Worker::new(
        RigId::new("worker"),
        Arc::clone(&board),
        worker_agent,
        opengoose_rig::work_mode::TaskMode,
    ));
    let worker_handle = Arc::clone(&worker);
    tokio::spawn(async move { worker_handle.run().await });
    // Operator
    let (agent, session_id) = create_operator_agent().await?;
    let agent = Arc::new(agent);
    let result = tui::run_tui(board, agent, session_id).await;
    worker.cancel();
    result
}
```

- [ ] **Step 3: import 추가**

main.rs 상단에 추가:
```rust
use opengoose_rig::rig::Worker;
use opengoose_rig::work_mode::TaskMode;
```

실제 사용은 fully-qualified path로 하므로 import가 필요 없을 수도 있음. 빌드로 확인.

- [ ] **Step 4: 빌드 확인**

Run: `cargo check -p opengoose`
Expected: success

- [ ] **Step 5: 기존 테스트 통과 확인**

Run: `cargo test`
Expected: 10/10 pass

- [ ] **Step 6: 커밋**

```bash
git add crates/opengoose/src/main.rs
git commit -m "feat: spawn Worker pull loop alongside Evolver in TUI mode"
```

---

### Task 5: TUI — Arc<Operator> 수용 + /task 핸들러 간소화

Task 3-4에 의존.

**Files:**
- Modify: `crates/opengoose-rig/src/rig.rs:117-134` (Operator impl — chat_streaming 추가)
- Modify: `crates/opengoose/src/tui/event.rs` (전체 — Agent → Operator)

- [ ] **Step 1: Operator에 `chat_streaming()` 메서드 추가**

rig.rs의 Operator impl 블록에 추가:

```rust
impl Operator {
    // ... 기존 without_board, chat ...

    /// 스트리밍 채팅 — TUI에서 토큰 단위 표시용.
    /// Agent.reply() 스트림을 직접 반환하여 호출자가 이벤트를 소비.
    pub async fn chat_streaming(
        &self,
        input: &str,
    ) -> anyhow::Result<impl futures::Stream<Item = Result<AgentEvent, anyhow::Error>>> {
        let session_config = self.mode.session_config(&WorkInput::chat(input));
        let session_id = session_config.id.clone();
        let message = Message::user().with_text(input);

        // 사용자 입력 로깅
        conversation_log::append_entry(&session_id, "user", input);

        self.agent
            .reply(message, session_config, Some(self.cancel.clone()))
            .await
    }
}
```

- [ ] **Step 2: 빌드 확인**

Run: `cargo check -p opengoose-rig`
Expected: success. Agent.reply()의 반환 타입과 일치하는지 확인.

- [ ] **Step 3: TUI event.rs — `run_tui` 시그니처 변경**

`Arc<Agent>` → `Arc<Operator>` 교체. `session_id` 파라미터 제거 (Operator 내부에 있음).

```rust
pub async fn run_tui(board: Arc<Board>, operator: Arc<Operator>) -> Result<()> {
```

import 변경:
```rust
// 삭제: use goose::agents::{Agent, AgentEvent, SessionConfig};
// 추가:
use goose::agents::AgentEvent;
use goose::conversation::message::MessageContent;
use opengoose_rig::rig::Operator;
```

- [ ] **Step 4: `handle_key` → agent/session_id 파라미터를 operator로 교체**

```rust
async fn handle_key(
    key: KeyEvent,
    app: &mut App,
    agent_tx: &mpsc::Sender<AgentMsg>,
    board: &Arc<Board>,
    operator: &Arc<Operator>,
) -> bool {
```

`handle_input` 호출부도 동일하게 변경.

- [ ] **Step 5: `handle_input` → operator 사용**

```rust
async fn handle_input(
    app: &mut App,
    text: &str,
    agent_tx: &mpsc::Sender<AgentMsg>,
    board: &Arc<Board>,
    operator: &Arc<Operator>,
) {
    // ... /board, /quit 처리 동일 ...

    // /task 명령
    if let Some(task_title) = text.strip_prefix("/task ") {
        let task_title = task_title.trim().trim_matches('"');
        if task_title.is_empty() {
            app.push_chat(ChatLine::System("Usage: /task \"description\"".into()));
            return;
        }
        handle_task(app, task_title, board).await;
        return;
    }

    // 일반 대화 → Operator로 전송
    if app.agent_busy {
        app.push_chat(ChatLine::System("Agent is busy...".into()));
        return;
    }

    app.agent_busy = true;
    spawn_operator_reply(operator.clone(), text.to_string(), agent_tx.clone());
}
```

- [ ] **Step 6: `handle_task` 간소화 — Agent 알림 블록 제거**

Worker가 자동으로 Board를 풀링하므로 Agent에게 claim 지시가 불필요.

```rust
async fn handle_task(
    app: &mut App,
    title: &str,
    board: &Arc<Board>,
) {
    match board
        .post(PostWorkItem {
            title: title.to_string(),
            description: String::new(),
            created_by: RigId::new("operator"),
            priority: Priority::P1,
            tags: vec![],
        })
        .await
    {
        Ok(item) => {
            app.push_chat(ChatLine::System(format!(
                "● #{} \"{}\" — posted (Worker will pick it up)",
                item.id, item.title
            )));
            if let Ok(items) = board.list().await {
                app.board_items = items;
            }
            // Worker가 board.notify를 통해 자동으로 작업을 가져감
        }
        Err(e) => {
            app.push_chat(ChatLine::System(format!("Post failed: {e}")));
        }
    }
}
```

- [ ] **Step 7: `spawn_agent_reply` → `spawn_operator_reply`로 교체**

어시스턴트 응답도 conversation_log에 기록해야 함 (`chat_streaming`은 user 입력만 로깅하므로).

```rust
fn spawn_operator_reply(
    operator: Arc<Operator>,
    input: String,
    tx: mpsc::Sender<AgentMsg>,
) {
    tokio::spawn(async move {
        match operator.chat_streaming(&input).await {
            Ok(stream) => {
                tokio::pin!(stream);
                while let Some(event) = stream.next().await {
                    match event {
                        Ok(AgentEvent::Message(msg))
                            if msg.role == rmcp::model::Role::Assistant =>
                        {
                            for content in &msg.content {
                                if let MessageContent::Text(text) = content {
                                    let _ = tx.send(AgentMsg::Text(text.text.clone())).await;
                                }
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(AgentMsg::Text(format!("\n⚠ Stream error: {e}"))).await;
                            break;
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                let _ = tx.send(AgentMsg::Text(format!("Error: {e}"))).await;
            }
        }
        let _ = tx.send(AgentMsg::Done).await;
    });
}
```

참고: conversation_log 로깅은 `chat_streaming`이 user 입력을 로깅하고, 어시스턴트 응답 로깅은 `process()` 경로와 달리 스트림 소비자 측에서 해야 한다. 현재 TUI에서는 기존 `spawn_agent_reply`도 로깅하지 않았으므로 기존 동작과 동일. 추후 로깅 통합 시 `chat_streaming`이 로깅 래퍼 스트림을 반환하도록 개선 가능.

- [ ] **Step 8: main.rs TUI 분기 — run_tui 호출부 변경**

```rust
None => {
    // ... board, web, evolver, worker 스폰 ...
    let (agent, session_id) = create_operator_agent().await?;
    let operator = Arc::new(opengoose_rig::rig::Operator::without_board(
        RigId::new("operator"),
        agent,
        &session_id,
    ));
    let result = tui::run_tui(board, operator).await;
    worker.cancel();
    result
}
```

- [ ] **Step 9: run_tui 내부 — select 루프의 handle_key 호출부 갱신**

```rust
// run_tui 내부 loop
Some(Ok(Event::Key(key))) => {
    if handle_key(key, &mut app, &agent_tx, &board, &operator).await {
        break;
    }
}
```

기존 `agent`, `session_id` 변수 참조 모두 제거.

- [ ] **Step 10: 사용하지 않는 import 정리**

event.rs에서 `SessionConfig`, `Message` 등 더 이상 사용하지 않는 import 제거.

- [ ] **Step 11: 빌드 확인**

Run: `cargo check -p opengoose`
Expected: success

- [ ] **Step 12: 기존 테스트 통과 확인**

Run: `cargo test`
Expected: 10/10 pass

- [ ] **Step 13: 커밋**

```bash
git add crates/opengoose-rig/src/rig.rs crates/opengoose/src/tui/event.rs crates/opengoose/src/main.rs
git commit -m "feat: TUI uses Operator with streaming, /task delegates to Worker via Board"
```

---

### Task 6: Headless 모드 — Board notify 기반 완료 대기

Task 4에 의존 (Worker가 스폰됨).

**Files:**
- Modify: `crates/opengoose/src/main.rs:386-458` (run_headless, run_agent_streaming, print_message)

- [ ] **Step 1: `run_headless` 재작성 — Worker 완료를 notify로 대기**

`run_agent_streaming`을 더 이상 사용하지 않음. Board에 포스팅 후 Worker가 처리할 때까지 notify로 대기.

```rust
async fn run_headless(board: &Board, task: &str) -> Result<()> {
    let rig_id = RigId::new("headless");
    let item = board
        .post(PostWorkItem {
            title: task.to_string(),
            description: String::new(),
            created_by: rig_id,
            priority: Priority::P1,
            tags: vec![],
        })
        .await?;

    println!("Posted #{}: \"{}\" — waiting for Worker...", item.id, item.title);

    let timeout = tokio::time::sleep(std::time::Duration::from_secs(600));
    tokio::pin!(timeout);

    loop {
        let notified = board.notify_handle().notified();

        match board.get(item.id).await? {
            Some(wi) if wi.status == Status::Done => {
                println!("✓ #{} completed", item.id);
                break;
            }
            Some(wi) if wi.status == Status::Abandoned => {
                anyhow::bail!("작업 항목 #{}이 포기되었습니다", item.id);
            }
            Some(_) => {}
            None => anyhow::bail!("작업 항목 #{}이 삭제되었습니다", item.id),
        }

        tokio::select! {
            _ = notified => {}
            _ = &mut timeout => anyhow::bail!("작업 항목 #{} 대기 시간 초과 (10분)", item.id),
            _ = tokio::signal::ctrl_c() => {
                eprintln!("\n중단되었습니다.");
                return Ok(());
            }
        }
    }

    Ok(())
}
```

- [ ] **Step 2: main.rs Run 분기 — Worker 스폰 + 새 run_headless 호출**

```rust
Some(Commands::Run { task }) => {
    let board = Arc::new(Board::connect(&db_url()).await?);
    web::spawn_server(Arc::clone(&board), cli.port).await?;
    let stamp_notify = board.stamp_notify_handle();
    tokio::spawn(evolver::run(Arc::clone(&board), stamp_notify));
    // Spawn Worker
    let (worker_agent, _) = create_worker_agent().await?;
    let worker = Arc::new(opengoose_rig::rig::Worker::new(
        RigId::new("worker"),
        Arc::clone(&board),
        worker_agent,
        opengoose_rig::work_mode::TaskMode,
    ));
    let worker_handle = Arc::clone(&worker);
    tokio::spawn(async move { worker_handle.run().await });
    // Headless — Worker가 처리할 때까지 대기
    let result = run_headless(&board, &task).await;
    worker.cancel();
    result
}
```

- [ ] **Step 3: 사용하지 않는 `run_agent_streaming`, `print_message` 함수 삭제**

`run_agent_streaming`과 `print_message`는 더 이상 호출되지 않으므로 삭제.

사용하지 않는 import 정리: `SessionConfig`, `Message`, `AgentEvent` 등이 main.rs에서 더 이상 필요한지 확인. `StreamExt`도 main.rs에서 제거 가능.

- [ ] **Step 4: 빌드 확인**

Run: `cargo check -p opengoose`
Expected: success

- [ ] **Step 5: 기존 테스트 통과 확인**

Run: `cargo test`
Expected: 10/10 pass

- [ ] **Step 6: 커밋**

```bash
git add crates/opengoose/src/main.rs
git commit -m "feat: headless mode waits for Worker via Board notify instead of running Agent directly"
```

---

### Task 7: main.rs 정리 — 중복 스폰 로직 추출

Task 4-6 이후 TUI/Run 분기에 board+web+evolver+worker 스폰 코드가 중복됨. 헬퍼로 추출.

**Files:**
- Modify: `crates/opengoose/src/main.rs`

- [ ] **Step 1: 공통 초기화 함수 추출**

```rust
struct Runtime {
    board: Arc<Board>,
    worker: Arc<opengoose_rig::rig::Worker>,
}

async fn init_runtime(port: u16) -> Result<Runtime> {
    let board = Arc::new(Board::connect(&db_url()).await?);
    web::spawn_server(Arc::clone(&board), port).await?;

    // Evolver
    let stamp_notify = board.stamp_notify_handle();
    tokio::spawn(evolver::run(Arc::clone(&board), stamp_notify));

    // Worker
    let (worker_agent, _) = create_worker_agent().await?;
    let worker = Arc::new(opengoose_rig::rig::Worker::new(
        RigId::new("worker"),
        Arc::clone(&board),
        worker_agent,
        opengoose_rig::work_mode::TaskMode,
    ));
    let worker_handle = Arc::clone(&worker);
    tokio::spawn(async move { worker_handle.run().await });

    Ok(Runtime { board, worker })
}
```

- [ ] **Step 2: main() 분기를 init_runtime 사용으로 간소화**

```rust
Some(Commands::Run { task }) => {
    let rt = init_runtime(cli.port).await?;
    let result = run_headless(&rt.board, &task).await;
    rt.worker.cancel();
    result
}
None => {
    let rt = init_runtime(cli.port).await?;
    let (agent, session_id) = create_operator_agent().await?;
    let operator = Arc::new(opengoose_rig::rig::Operator::without_board(
        RigId::new("operator"),
        agent,
        &session_id,
    ));
    let result = tui::run_tui(rt.board, operator).await;
    rt.worker.cancel();
    result
}
```

- [ ] **Step 3: 빌드 + 테스트 확인**

Run: `cargo check -p opengoose && cargo test`
Expected: success, 10/10 pass

- [ ] **Step 4: 커밋**

```bash
git add crates/opengoose/src/main.rs
git commit -m "refactor: extract init_runtime to deduplicate board+evolver+worker setup"
```

---

### Task 8: 최종 검증 — 전체 빌드 + 테스트

**Files:** (없음 — 검증만)

- [ ] **Step 1: cargo clippy**

Run: `cargo clippy --all-targets 2>&1`
Expected: no errors (warnings 허용)

- [ ] **Step 2: 전체 테스트**

Run: `cargo test`
Expected: 10/10 pass

- [ ] **Step 3: 수동 스모크 테스트 안내 출력**

다음 수동 검증이 필요함:
1. `opengoose` → TUI → `/task "echo hello"` → Board 상태 claimed → done
2. `opengoose run "echo hello"` → Worker 실행 후 종료
