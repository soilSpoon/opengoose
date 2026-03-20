# TUI 로깅 시스템

## 배경

OpenGoose TUI에서 Worker/Evolver의 tracing 로그가 stderr로 출력되어 ratatui 화면과 겹치는 문제가 있다. 또한 패널이 늘어나면서 화면 공간이 부족해지고 있다.

이 스펙은 두 가지를 해결한다:
1. tracing 로그를 파일로 리다이렉트하여 TUI 깨짐 방지
2. 탭 기반 네비게이션으로 TUI 레이아웃 개선 + 로그 뷰 추가

## 결정사항

1. **파일 로깅**: TUI 모드에서 tracing을 파일에 기록. Headless/CLI는 stderr + 파일.
2. **세션 기반 로테이션**: 실행마다 새 파일, 최근 10개 유지.
3. **탭 네비게이션**: Chat | Board | Logs — 한 번에 하나의 뷰만 표시.
4. **로그 뷰 필터링**: 기본은 구조화된 이벤트만, verbose 토글로 전체 로그 표시.

## 변경사항

### 1. 파일 로깅 — tracing subscriber 분기

**현재**: `main.rs` 상단에서 `tracing_subscriber::fmt().init()` 한 번 호출. 모든 모드에서 stderr 출력.

**목표**:
- tracing subscriber 초기화를 `Cli::parse()` 이후, `match cli.command` 이전으로 이동
- TUI 모드: 두 개의 레이어 구성
  - **파일 레이어**: `~/.opengoose/logs/opengoose-{ISO8601}.log`에 전체 로그 기록
  - **채널 레이어** (`TuiLayer`): `mpsc::Sender<LogEntry>`로 TUI에 이벤트 전송 (Logs 뷰용)
  - stderr 출력 없음
- Headless/CLI 모드: stderr + 파일
- Board/Rigs/Skills 서브커맨드: 기존대로 stderr만 (파일 불필요)

```rust
let cli = Cli::parse();

// 모드에 따라 tracing subscriber 분기
let log_rx = match &cli.command {
    None => {
        // TUI 모드: 파일 + 채널 (stderr 없음)
        let log_file = create_session_log_file()?;
        cleanup_old_logs(10)?;
        let (log_tx, log_rx) = tokio::sync::mpsc::channel::<LogEntry>(1000);
        tracing_subscriber::registry()
            .with(fmt::layer().with_writer(Arc::new(log_file)))
            .with(TuiLayer::new(log_tx))
            .with(EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "opengoose=info,goose=error".into()))
            .init();
        Some(log_rx)
    }
    Some(Commands::Run { .. }) => {
        // Headless: stderr + 파일
        let log_file = create_session_log_file()?;
        cleanup_old_logs(10)?;
        tracing_subscriber::registry()
            .with(fmt::layer().with_writer(std::io::stderr))
            .with(fmt::layer().with_writer(Arc::new(log_file)))
            .with(EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "opengoose=info,goose=error".into()))
            .init();
        None
    }
    _ => {
        // Board/Rigs/Skills/Logs 서브커맨드: stderr만
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "opengoose=info,goose=error".into()))
            .init();
        None
    }
};
```

**세션 로그 파일**:
- 경로: `~/.opengoose/logs/opengoose-{timestamp}.log`
- 타임스탬프 형식: `2026-03-20T15-10-00` (파일명에 안전한 형태)
- 시작 시 `~/.opengoose/logs/opengoose-*.log` 파일을 수정 시간 기준 정렬, 10개 초과분 삭제

**TuiLayer**: `tracing_subscriber::Layer` 구현체. 각 이벤트를 `LogEntry`로 변환하여 채널로 전송.

중요: `Layer::on_event()`는 **동기 함수**이므로 `try_send()`를 사용해야 한다. 채널이 가득 차면 해당 로그 항목은 조용히 버린다 (파일에는 전체 기록됨).

```rust
impl<S: Subscriber> Layer<S> for TuiLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let entry = LogEntry::from_event(event);
        // 동기 전송 — 채널 가득 차면 drop
        let _ = self.tx.try_send(entry);
    }
}
```

```rust
struct LogEntry {
    timestamp: DateTime<Utc>,
    level: Level,
    target: String,
    message: String,
    /// 구조화된 이벤트인지 여부 (verbose 필터링용)
    structured: bool,
}
```

구조화된 이벤트 판별: `target`이 `opengoose_rig::rig` 또는 `opengoose::evolver`이고 `level`이 INFO 이상이면 `structured = true`. 나머지는 `false`.

### 2. 탭 기반 TUI 레이아웃

**현재**: Board + Rigs (상단 좌우) + Chat (중앙) + Input (하단) 고정 배치.

**목표**: 3개 탭, 한 번에 하나의 뷰만 표시.

**탭 구조**:

| 탭 | 단축키 | 내용 |
|---|---|---|
| Chat | `Ctrl+1` | Chat 영역 + Input (현재 메인 뷰) |
| Board | `Ctrl+2` | Board 목록 + Rigs 패널 (현재 상단 패널들) |
| Logs | `Ctrl+3` | 로그 뷰 (새로 추가) |

**단축키 호환성**: `Ctrl+1/2/3`은 일부 터미널에서 전달되지 않을 수 있다. 대체 단축키로 `Tab` (다음 탭) / `Shift+Tab` (이전 탭)도 지원한다. 단, `Tab`은 Chat 탭에서 입력 중일 때는 탭 문자 입력이 아닌 탭 전환으로 동작한다 (Chat 입력에 탭 문자는 불필요).

**탭 바**:
- 화면 최상단에 `[ Chat ] [ Board ] [ Logs ]` 표시
- 현재 탭 하이라이트 (Bold 또는 반전)
- `Ctrl+\`로 탭 바 토글 (접기/펼치기)
- 탭 바가 접혀있어도 `Ctrl+1/2/3`, `Tab`/`Shift+Tab`으로 전환 가능

**레이아웃 렌더링**:

```
탭 바 표시 시:
┌ [ Chat ] [ Board ] [ Logs ] ──────────────────┐  ← 1줄
│                                                │
│           현재 탭의 뷰 콘텐츠                    │  ← 나머지 전체
│                                                │
└────────────────────────────────────────────────┘

탭 바 숨김 시:
│                                                │
│           현재 탭의 뷰 콘텐츠                    │  ← 전체 화면
│                                                │
```

**Chat 탭**: Chat + Input 영역만. Board/Rigs 패널 없음.
**Board 탭**: Board 목록 (전체 높이) + Rigs 패널 (우측). 기존 레이아웃과 동일하되 전체 화면 활용.
**Logs 탭**: 로그 뷰. 스크롤 가능. Input 바 없음.

### 3. Logs 뷰

**기본 모드 (structured)**: Worker/Evolver의 의미 있는 이벤트만 표시.
```
15:10:03 [worker]  claimed #5 "echo hello"
15:10:20 [worker]  ✓ submitted #5
15:10:21 [evolver] processing stamp #3 (Quality: 0.2)
15:10:45 [evolver] generated skill 'error-handling' for stamp #3
```

**Verbose 모드**: `v` 키 토글. 전체 tracing 로그 표시.
```
15:10:03 INFO  opengoose_rig::rig  claimed work item rig=worker item_id=5 title=echo hello
15:10:03 DEBUG goose::agents::reply_parts  WAITING_LLM_STREAM_START
15:10:20 DEBUG opengoose_rig::rig  agent message: Assistant rig=worker
15:10:20 INFO  opengoose_rig::rig  submitted work item rig=worker item_id=5
```

**입력 바와 단축키**: Input 바는 Chat 탭에서만 렌더링하고 활성화한다. Board/Logs 탭에서는 텍스트 입력을 받지 않으므로, `v` 같은 단일 키 단축키가 입력과 충돌하지 않는다.

**스크롤**: Up/Down, PageUp/PageDown. 최하단이면 자동 스크롤 (tail -f 동작). 위로 스크롤하면 자동 스크롤 중지, 최하단으로 돌아오면 재개.

**로그 버퍼**: 최근 1000줄 유지 (메모리 제한). 오래된 항목은 버퍼에서 제거 (파일에는 전체 기록).

### 4. App 상태 변경

`tui/app.rs`에 추가:

```rust
pub enum Tab {
    Chat,
    Board,
    Logs,
}

// App 구조체에 추가
pub current_tab: Tab,
pub tab_bar_visible: bool,
pub log_entries: VecDeque<LogEntry>,  // 최근 1000줄
pub log_verbose: bool,
pub log_scroll_offset: usize,
```

참고: 기존 `scroll_offset: u16`도 `usize`로 통일한다.

### 5. 이벤트 처리 변경

`tui/event.rs`에서:

- `Ctrl+1/2/3`: `app.current_tab` 전환
- `Tab` / `Shift+Tab`: 다음/이전 탭 전환 (대체 단축키)
- `Ctrl+\`: `app.tab_bar_visible` 토글
- `v` (Logs 탭에서만): `app.log_verbose` 토글
- Up/Down/PageUp/PageDown: 현재 탭에 따라 Chat 스크롤 또는 로그 스크롤
- 텍스트 입력 (`KeyCode::Char`): Chat 탭에서만 활성. 다른 탭에서는 무시.

**`run_tui` 시그니처 변경**: `log_rx`를 파라미터로 받는다.

```rust
pub async fn run_tui(
    board: Arc<Board>,
    operator: Arc<Operator>,
    log_rx: mpsc::Receiver<LogEntry>,
) -> Result<()> {
```

`run_tui`의 select 루프에 `log_rx` 수신 추가:

```rust
Some(entry) = log_rx.recv() => {
    if app.log_entries.len() >= 1000 {
        app.log_entries.pop_front();
    }
    app.log_entries.push_back(entry);
    // 자동 스크롤: 최하단에 있으면 따라감
    if app.log_auto_scroll {
        app.log_scroll_offset = 0;
    }
}
```

**main.rs 연결**: `init_runtime` 이전에 tracing subscriber 설정, `log_rx`를 `run_tui`에 전달.

```rust
None => {
    let log_rx = log_rx.expect("TUI mode must have log_rx");
    let rt = init_runtime(cli.port).await?;
    let (agent, session_id) = create_operator_agent().await?;
    let operator = Arc::new(Operator::without_board(...));
    let result = tui::run_tui(rt.board, operator, log_rx).await;
    rt.worker.cancel();
    result
}
```

## 범위 밖

- Worker stale claim resume (별도 스펙)
- 로그 검색/필터링
- 로그 내보내기
- 탭 순서 변경
- Board 탭 내 상세 작업 뷰

## 테스트

- 기존 테스트 (114개) 통과 필수.
- 새 단위 테스트: `TuiLayer`가 `LogEntry`를 채널로 `try_send`하는지 검증.
- 새 단위 테스트: `cleanup_old_logs(10)`이 10개 초과 파일을 삭제하는지 검증.
- 새 단위 테스트: 채널 가득 찬 상태에서 `TuiLayer`가 패닉 없이 동작하는지 검증.
- 수동 스모크 테스트: TUI 시작 → `Ctrl+3` (또는 Tab 두 번) → Logs 뷰 표시 → Chat 탭에서 `/task "echo hello"` → Logs로 돌아가면 Worker 이벤트 표시 확인.
- 수동 스모크 테스트: `~/.opengoose/logs/` 에 세션 로그 파일 생성 확인.
- 수동 스모크 테스트: `Ctrl+\`로 탭 바 접기/펼치기 확인.
