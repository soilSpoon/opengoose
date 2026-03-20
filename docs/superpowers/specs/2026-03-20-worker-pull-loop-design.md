# Worker Pull Loop 통합

## 배경

OpenGoose v0.2는 Worker 리그가 Board에서 자율적으로 작업을 가져오는 풀(pull) 아키텍처를 사용한다. `Rig<TaskMode>` (Worker)와 `Worker::run()` 풀 루프가 구현되어 있지만 실제로 스폰된 적이 없다. 메인 바이너리는 Rig 추상화를 완전히 우회하고 raw Goose `Agent`를 직접 사용하고 있다.

이 스펙은 기존 조각들을 연결하여 Board를 실제 오케스트레이터로 만드는 것이 목표다.

## 결정사항

1. **채팅/태스크 분리**: Operator가 대화를 직접 처리한다. 태스크는 명시적 액션을 통해서만 Board에 도달한다.
2. **Worker 수**: 자동 스폰되는 Worker 1개 (`id="worker"`). 멀티 워커 확장은 추후로 미룬다.
3. **Board 포스팅**: `/task` TUI 명령과 Agent 도구 사용(Board CLI), 두 가지 경로를 제공한다.

## 변경사항

### 1. main.rs — raw Agent를 Operator + Worker로 교체

**현재**: `create_agent()`가 `(Agent, String)`을 반환한다. TUI와 headless 모드가 Agent를 직접 호출한다. Agent 생성 로직이 `main.rs::create_agent()`와 `evolver.rs::create_evolver_agent()`에 중복되어 있다.

**목표**:
- 공통 Agent 생성 로직을 `create_base_agent(session_name)` → `(Agent, String)`으로 추출한다.
- 역할별 래퍼가 base를 호출한 뒤 `extend_system_prompt`를 적용한다:
  - `create_operator_agent()` — Board CLI 지시문 (기존 프롬프트)
  - `create_worker_agent()` — 태스크 지향 프롬프트
  - `evolver.rs::create_evolver_agent()` — base 호출로 리팩터링 (비차단, 추후 진행)
- Operator Agent를 `Operator::without_board()`로 래핑하여 채팅 처리.
- Worker Agent를 생성하고 `Rig::new()` + `TaskMode`로 래핑.
- 기존 Evolver 스폰과 나란히 `tokio::spawn(worker.run())` 실행.
- `Rig<M>`은 `tokio::spawn`을 위해 `Send + Sync`이어야 한다. 충족됨 — `Agent`, `Arc<Board>`, `CancellationToken`, `RigId`, `TaskMode` 모두 `Send + Sync`.

```rust
// 공통 Agent 생성
async fn create_base_agent(session_name: &str) -> Result<(Agent, String)> { ... }

// Operator — 채팅
let (agent, session_id) = create_operator_agent().await?;
let operator = Arc::new(Operator::without_board(
    RigId::new("operator"),
    agent,
    &session_id,
));

// Worker — 풀 루프
let (worker_agent, _) = create_worker_agent().await?;
let worker = Arc::new(Worker::new(
    RigId::new("worker"),
    Arc::clone(&board),
    worker_agent,
    TaskMode,
));
let worker_cancel = worker.cancel_token();
tokio::spawn({
    let w = Arc::clone(&worker);
    async move { w.run().await }
});
```

### 2. TUI — Operator 스트리밍 + /task 명령

**현재**: `tui::run_tui(board, agent, session_id)`가 raw `Arc<Agent>`를 받는다. `/task` 핸들러(event.rs:271-281)가 Board에 포스팅한 뒤 Operator Agent에게 수동으로 claim/submit을 지시한다.

**목표**:
- `Arc<Agent>` 대신 `Arc<Operator>`를 받도록 변경.
- **스트리밍**: TUI는 채팅 표시를 위해 토큰 단위 스트리밍이 필요하다. `Operator.chat()`은 내부적으로 스트림을 숨긴다. 해결: Operator에 스트리밍 메서드를 추가하여 `AgentEvent`를 채널 또는 스트림 반환을 통해 호출자에게 전달한다. 세션 관리는 중앙 집중화를 유지하면서 TUI에 적절한 스트리밍 접근을 제공한다.

```rust
// Rig<M> (공유) 또는 Operator (전용):
pub async fn process_streaming(&self, input: WorkInput)
    -> anyhow::Result<impl Stream<Item = Result<AgentEvent>>>
{
    let session_config = self.mode.session_config(&input);
    let message = Message::user().with_text(&input.text);
    self.agent.reply(message, session_config, Some(self.cancel.clone())).await
}

// Operator 편의 메서드:
pub async fn chat_streaming(&self, input: &str)
    -> anyhow::Result<impl Stream<Item = Result<AgentEvent>>>
{
    self.process_streaming(WorkInput::chat(input)).await
}
```

TUI가 `operator.chat_streaming(input)`을 호출하고 스트림을 소비하여 토큰 단위로 표시한다. raw Agent 접근 불필요.
- **`/task` 핸들러 변경**: "Agent에게 claim 지시" 블록(event.rs:271-281) 제거. Worker가 Board 아이템을 자동으로 픽업한다. 포스팅 + 확인 메시지 출력만 수행.
- Worker 진행 상황은 TUI에 표시하지 않음 (범위 밖 — 2초 틱으로 Board 상태만 표시).

### 3. Headless 모드 (Commands::Run)

**현재**: Board에 포스팅한 뒤 `run_agent_streaming`을 통해 Agent를 직접 실행한다.

**목표**:
- Board에 포스팅하고 반환된 `item.id`를 **캡처**한다.
- Board `notify` (폴링 아님)를 사용하여 완료를 감지한다. `board.submit()`이 `notify.notify_waiters()`를 호출하므로 Worker 완료 시 headless 모드가 즉시 깨어난다.
- 상태 확인 전에 `notified()`를 등록한다 (Worker 풀 루프와 동일한 패턴 — 알림 손실 없음).
- `Option::None` (아이템 삭제됨)을 에러로 처리한다.
- `tokio::time::sleep`으로 10분 타임아웃.
- `ctrl_c`와 함께 `tokio::select!`로 감싼다 (현재 패턴 유지).
- 완료 후 Worker의 대화 로그를 출력한다.

```rust
let item = board.post(PostWorkItem { ... }).await?;
let timeout = tokio::time::sleep(Duration::from_secs(600));
tokio::pin!(timeout);

loop {
    let notified = board.notify_handle().notified();

    match board.get(item.id).await? {
        Some(wi) if wi.status == Status::Done => break,
        Some(_) => {}
        None => anyhow::bail!("작업 항목 #{}이 삭제되었습니다", item.id),
    }

    tokio::select! {
        _ = notified => {}
        _ = &mut timeout => anyhow::bail!("작업 항목 #{} 대기 시간 초과", item.id),
        _ = tokio::signal::ctrl_c() => {
            eprintln!("\n중단되었습니다.");
            return Ok(());
        }
    }
}
```

### 4. Worker Agent 시스템 프롬프트

`create_worker_agent()`가 `create_base_agent("worker")`를 호출한 뒤 다음 내용으로 `extend_system_prompt().await`를 수행한다:

```
You are an OpenGoose Worker rig. You receive tasks from the Board and execute them autonomously.
Focus on completing the task. Use available tools. Do not ask clarifying questions — make reasonable assumptions and proceed.
```

참고: `extend_system_prompt`는 비동기이며 반드시 await해야 한다.

### 5. Worker 실패 — Board 정리

`Worker::try_claim_and_execute()` (rig.rs)에서 `self.process(input).await`가 실패하면 작업 항목이 `Claimed` 상태로 영구히 남는다. 처리 실패 시 `board.abandon(item.id)`를 추가한다:

```rust
async fn try_claim_and_execute(&self) -> anyhow::Result<()> {
    // ... claim ...
    let result = self.process(input).await;
    if let Err(e) = &result {
        warn!(rig = %self.id, item_id = item.id, error = %e, "실행 실패, 포기 처리");
        board_arc.abandon(item.id).await.ok();
    } else {
        board_arc.submit(item.id, &self.id).await?;
    }
    result
}
```

### 6. 알림 손실 방지 — register-before-check 패턴

`tokio::sync::Notify::notify_waiters()`는 현재 `.notified()`를 대기 중인 태스크만 깨운다. 해결: 작업 확인 **전에** `notified()` 퓨처를 등록한다. 실행 중 도착하는 모든 알림은 이미 등록된 퓨처가 캡처한다.

또한 `try_claim_and_execute`는 "작업 발견"과 "대기할 작업 없음"을 구분해야 루프가 대기할지 즉시 재확인할지 판단할 수 있다.

```rust
// Worker::run()
loop {
    // 1. 관심 먼저 등록 — 이 시점 이후의 모든 알림을 캡처
    let notified = board.notify_handle().notified();

    // 2. 준비된 항목 확인 + 실행
    match self.try_claim_and_execute().await {
        Ok(true) => continue,  // 작업 발견, 즉시 추가 확인
        Ok(false) => {}        // 작업 없음, 대기로 이동
        Err(e) => warn!(rig = %self.id, error = %e, "실행 실패"),
    }

    // 3. 작업 없음 — 알림 대기 (손실 불가능)
    tokio::select! {
        _ = notified => {}
        _ = self.cancel.cancelled() => break,
    }
}
```

`try_claim_and_execute`의 반환 타입 변경: `Result<bool>` — `true` = claim 후 실행 완료, `false` = 준비된 항목 없음.

### 7. 정상 종료 (Graceful Shutdown)

TUI 종료 또는 headless 완료 시 `CancellationToken`을 통해 Worker를 취소한다:

```rust
worker.cancel(); // Worker::run() 루프에서 break 트리거
```

## 범위 밖

- 멀티 워커 스폰 (rig-per-worker)
- 태그 기반 작업 항목 라우팅
- TUI 실시간 Worker 출력 스트리밍
- Worker 반복 실패 시 재시도/백오프
- Operator → Board 자동 라우팅 (AI가 채팅 vs 태스크 판단)

## 테스트

- 기존 테스트 (10/10) 통과 필수.
- 새 단위 테스트: `try_claim_and_execute`가 처리 실패 시 `abandon`을 호출하는지 검증.
- 수동 스모크 테스트: `opengoose` → TUI → `/task "echo hello"` → Board 상태가 claimed → done으로 변경되는지 확인.
- 수동 스모크 테스트: `opengoose run "echo hello"` → Worker가 실행 후 종료되는지 확인.
