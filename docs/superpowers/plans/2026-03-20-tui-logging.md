# TUI 로깅 시스템 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** TUI에서 tracing 로그가 화면을 깨뜨리지 않도록 파일 로깅으로 전환하고, 탭 기반 네비게이션 + Logs 뷰를 추가하여 Worker/Evolver 활동을 실시간으로 확인.

**Architecture:** tracing subscriber를 모드별로 분기 (TUI: 파일+채널, headless: stderr+파일, CLI: stderr). TUI를 탭 기반 (Chat|Board|Logs)으로 재구성하고, 채널을 통해 수신한 로그를 Logs 탭에 표시.

**Tech Stack:** Rust, tracing/tracing-subscriber, tokio mpsc, ratatui, crossterm

**Spec:** `docs/superpowers/specs/2026-03-20-tui-logging-design.md`

---

### Task 1: LogEntry 타입 + 세션 로그 파일 유틸리티

파일 로깅의 기반이 되는 타입과 유틸리티. 독립적.

**Files:**
- Create: `crates/opengoose/src/tui/log_entry.rs`
- Modify: `crates/opengoose/src/tui/mod.rs`

- [ ] **Step 1: `log_entry.rs` 작성**

```rust
use chrono::{DateTime, Utc};
use tracing::Level;

/// TUI Logs 뷰에 표시되는 로그 항목.
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub level: Level,
    pub target: String,
    pub message: String,
    /// 구조화된 이벤트인지 여부 (verbose 필터링용).
    /// target이 opengoose_rig::rig 또는 opengoose::evolver이고 INFO 이상이면 true.
    pub structured: bool,
}

impl LogEntry {
    pub fn is_structured_target(target: &str) -> bool {
        target.starts_with("opengoose_rig::rig") || target.starts_with("opengoose::evolver")
    }
}

/// 세션 로그 파일 생성. ~/.opengoose/logs/opengoose-{timestamp}.log
pub fn create_session_log_file() -> anyhow::Result<std::fs::File> {
    let home = dirs::home_dir().unwrap_or_else(|| ".".into());
    let log_dir = home.join(".opengoose/logs");
    std::fs::create_dir_all(&log_dir)?;

    let timestamp = Utc::now().format("%Y-%m-%dT%H-%M-%S");
    let path = log_dir.join(format!("opengoose-{timestamp}.log"));
    let file = std::fs::File::create(&path)?;
    Ok(file)
}

/// 오래된 세션 로그 삭제. keep개만 유지.
pub fn cleanup_old_logs(keep: usize) -> anyhow::Result<()> {
    let home = dirs::home_dir().unwrap_or_else(|| ".".into());
    let log_dir = home.join(".opengoose/logs");
    if !log_dir.is_dir() {
        return Ok(());
    }

    let mut logs: Vec<_> = std::fs::read_dir(&log_dir)?
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("opengoose-")
                && e.path().extension().is_some_and(|ext| ext == "log")
        })
        .collect();

    if logs.len() <= keep {
        return Ok(());
    }

    // 수정 시간 기준 정렬 (오래된 것 먼저)
    logs.sort_by_key(|e| {
        e.metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    });

    let to_remove = logs.len() - keep;
    for entry in logs.into_iter().take(to_remove) {
        std::fs::remove_file(entry.path()).ok();
    }

    Ok(())
}
```

- [ ] **Step 2: `tui/mod.rs`에 모듈 추가**

```rust
mod app;
mod event;
pub mod log_entry;
mod ui;

pub use event::run_tui;
```

- [ ] **Step 3: `cleanup_old_logs` 유닛 테스트 추가**

`log_entry.rs` 하단에:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_old_logs_removes_excess() {
        let dir = tempfile::tempdir().unwrap();
        let log_dir = dir.path().join(".opengoose/logs");
        std::fs::create_dir_all(&log_dir).unwrap();

        // 5개 파일 생성
        for i in 0..5 {
            let path = log_dir.join(format!("opengoose-test-{i}.log"));
            std::fs::write(&path, "test").unwrap();
            // 수정 시간 차이를 위해 약간 대기
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // HOME을 tempdir로 오버라이드하기 어려우므로,
        // cleanup_old_logs_in(dir, keep) 헬퍼를 추출하여 테스트.
        // 또는 cleanup_old_logs를 log_dir를 받는 버전으로 리팩터링.
    }

    #[test]
    fn is_structured_target_matches_rig_and_evolver() {
        assert!(LogEntry::is_structured_target("opengoose_rig::rig"));
        assert!(LogEntry::is_structured_target("opengoose_rig::rig::something"));
        assert!(LogEntry::is_structured_target("opengoose::evolver"));
        assert!(!LogEntry::is_structured_target("goose::agents"));
        assert!(!LogEntry::is_structured_target("opengoose::web"));
    }
}
```

참고: `cleanup_old_logs`를 테스트하려면 로그 디렉토리를 파라미터로 받는 내부 함수 `cleanup_old_logs_in(dir, keep)`를 추출하고, 공개 API는 `~/.opengoose/logs`로 호출하는 래퍼로 유지.

- [ ] **Step 4: 빌드 + 테스트 확인**

Run: `cargo check -p opengoose && cargo test -p opengoose`
Expected: success

- [ ] **Step 5: 커밋**

```bash
git add crates/opengoose/src/tui/log_entry.rs crates/opengoose/src/tui/mod.rs
git commit -m "feat: LogEntry type + session log file utilities with tests"
```

---

### Task 2: TuiLayer — tracing subscriber Layer 구현

채널로 로그를 TUI에 전송하는 커스텀 tracing Layer.

**Files:**
- Create: `crates/opengoose/src/tui/tui_layer.rs`
- Modify: `crates/opengoose/src/tui/mod.rs`

- [ ] **Step 1: `tui_layer.rs` 작성**

```rust
use super::log_entry::LogEntry;
use chrono::Utc;
use tokio::sync::mpsc;
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

/// tracing Layer: 이벤트를 LogEntry로 변환하여 mpsc 채널로 전송.
/// on_event()는 동기 함수이므로 try_send() 사용.
/// 채널 가득 차면 조용히 버림 (파일에는 전체 기록됨).
pub struct TuiLayer {
    tx: mpsc::Sender<LogEntry>,
}

impl TuiLayer {
    pub fn new(tx: mpsc::Sender<LogEntry>) -> Self {
        Self { tx }
    }
}

impl<S: Subscriber> Layer<S> for TuiLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();
        let level = *metadata.level();
        let target = metadata.target().to_string();

        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);

        let structured =
            LogEntry::is_structured_target(&target) && level <= Level::INFO;

        let entry = LogEntry {
            timestamp: Utc::now(),
            level,
            target,
            message: visitor.message,
            structured,
        };

        // 동기 전송 — 채널 가득 차면 drop
        let _ = self.tx.try_send(entry);
    }
}

#[derive(Default)]
struct MessageVisitor {
    message: String,
}

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
        } else if self.message.is_empty() {
            self.message = format!("{} = {:?}", field.name(), value);
        } else {
            self.message
                .push_str(&format!(" {} = {:?}", field.name(), value));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        } else if self.message.is_empty() {
            self.message = format!("{} = {}", field.name(), value);
        } else {
            self.message
                .push_str(&format!(" {} = {}", field.name(), value));
        }
    }
}
```

- [ ] **Step 2: `tui/mod.rs` 업데이트**

```rust
mod app;
mod event;
pub mod log_entry;
pub mod tui_layer;
mod ui;

pub use event::run_tui;
```

- [ ] **Step 3: TuiLayer 유닛 테스트 추가**

`tui_layer.rs` 하단에:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn tui_layer_sends_log_entry() {
        let (tx, mut rx) = mpsc::channel(10);
        let layer = TuiLayer::new(tx);

        // tracing subscriber에 등록하여 테스트
        use tracing_subscriber::layer::SubscriberExt;
        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(target: "opengoose_rig::rig", "test message");
        });

        let entry = rx.try_recv().unwrap();
        assert!(entry.structured);
        assert_eq!(entry.level, Level::INFO);
    }

    #[tokio::test]
    async fn tui_layer_does_not_panic_when_channel_full() {
        let (tx, _rx) = mpsc::channel(1);
        let layer = TuiLayer::new(tx);

        use tracing_subscriber::layer::SubscriberExt;
        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, || {
            // 채널 크기 1 — 두 번째부터 drop되지만 패닉 없어야 함
            for _ in 0..100 {
                tracing::info!("flood");
            }
        });
    }
}
```

- [ ] **Step 4: 빌드 + 테스트 확인**

Run: `cargo check -p opengoose && cargo test -p opengoose`
Expected: success

- [ ] **Step 5: 커밋**

```bash
git add crates/opengoose/src/tui/tui_layer.rs crates/opengoose/src/tui/mod.rs
git commit -m "feat: TuiLayer — tracing subscriber Layer with try_send and tests"
```

---

### Task 3: main.rs + event.rs — tracing subscriber 분기 + run_tui 시그니처 변경

tracing 초기화를 모드별로 분기하고, `run_tui`에 `log_rx`를 전달.
**Task 3과 Task 5(event.rs 시그니처)는 컴파일 의존성이 있으므로 함께 진행.**

**Files:**
- Modify: `crates/opengoose/src/main.rs:125-155` (main 함수 상단)
- Modify: `crates/opengoose/src/tui/event.rs:29-30` (run_tui 시그니처)
- Modify: `crates/opengoose/Cargo.toml` (tracing-subscriber features)

- [ ] **Step 1: Cargo.toml 의존성 확인/추가**

`tracing-subscriber`에 `registry`, `fmt`, `env-filter` features가 필요. 워크스페이스 설정에서 이미 있는지 확인. 없으면 추가.

Run: `grep -r "tracing-subscriber" Cargo.toml` (루트)

- [ ] **Step 2: main.rs — subscriber 초기화를 모드별 분기로 변경**

기존 코드 (lines 127-132):
```rust
tracing_subscriber::fmt()
    .with_env_filter(...)
    .init();

let cli = Cli::parse();
```

변경:
```rust
let cli = Cli::parse();

let log_rx = match &cli.command {
    None => {
        // TUI 모드: 파일 + TuiLayer (stderr 없음)
        let log_file = tui::log_entry::create_session_log_file()?;
        tui::log_entry::cleanup_old_logs(10)?;
        let (log_tx, log_rx) = tokio::sync::mpsc::channel::<tui::log_entry::LogEntry>(1000);
        use tracing_subscriber::layer::SubscriberExt;
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer()
                .with_writer(std::sync::Mutex::new(log_file)))
            .with(tui::tui_layer::TuiLayer::new(log_tx))
            .with(tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "opengoose=info,goose=error".into()))
            .init();
        Some(log_rx)
    }
    Some(Commands::Run { .. }) => {
        // Headless: stderr + 파일
        let log_file = tui::log_entry::create_session_log_file()?;
        tui::log_entry::cleanup_old_logs(10)?;
        use tracing_subscriber::layer::SubscriberExt;
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
            .with(tracing_subscriber::fmt::layer()
                .with_writer(std::sync::Mutex::new(log_file)))
            .with(tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "opengoose=info,goose=error".into()))
            .init();
        None
    }
    _ => {
        // CLI 서브커맨드: stderr만
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "opengoose=info,goose=error".into()),
            )
            .init();
        None
    }
};
```

- [ ] **Step 3: event.rs — `run_tui` 시그니처에 `log_rx` 추가**

`run_tui`에 `log_rx` 파라미터를 추가하고, select 루프에 수신 arm 추가 (탭 전환은 Task 5에서):

```rust
use super::log_entry::LogEntry;
// run_tui 시그니처 변경:
pub async fn run_tui(
    board: Arc<Board>,
    operator: Arc<Operator>,
    mut log_rx: tokio::sync::mpsc::Receiver<LogEntry>,
) -> Result<()> {
```

select 루프에 `log_rx` arm 추가:
```rust
Some(entry) = log_rx.recv() => {
    app.push_log(entry);
}
```

- [ ] **Step 4: main.rs TUI 분기 — `log_rx`를 `run_tui`에 전달**

```rust
None => {
    let log_rx = log_rx.expect("TUI mode must have log_rx");
    let rt = init_runtime(cli.port).await?;
    let (agent, session_id) = create_operator_agent().await?;
    let operator = Arc::new(opengoose_rig::rig::Operator::without_board(
        RigId::new("operator"),
        agent,
        &session_id,
    ));
    let result = tui::run_tui(rt.board, operator, log_rx).await;
    rt.worker.cancel();
    result
}
```

- [ ] **Step 4: 빌드 확인**

Run: `cargo check -p opengoose`
Expected: success (Task 5와 함께 진행 시)

- [ ] **Step 5: 커밋**

```bash
git add crates/opengoose/src/main.rs
git commit -m "feat: tracing subscriber mode-based branching (TUI=file+channel, headless=stderr+file)"
```

---

### Task 4: App 상태 — Tab, 로그 버퍼, verbose 토글

TUI 앱 상태에 탭 관련 필드를 추가.

**Files:**
- Modify: `crates/opengoose/src/tui/app.rs`

- [ ] **Step 1: `Tab` enum 추가 + App 필드 추가**

app.rs 상단에:
```rust
use std::collections::VecDeque;
use super::log_entry::LogEntry;
```

`Tab` enum 추가:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Chat,
    Board,
    Logs,
}

impl Tab {
    pub fn next(self) -> Self {
        match self {
            Tab::Chat => Tab::Board,
            Tab::Board => Tab::Logs,
            Tab::Logs => Tab::Chat,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Tab::Chat => Tab::Logs,
            Tab::Board => Tab::Chat,
            Tab::Logs => Tab::Board,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Tab::Chat => "Chat",
            Tab::Board => "Board",
            Tab::Logs => "Logs",
        }
    }

    pub const ALL: [Tab; 3] = [Tab::Chat, Tab::Board, Tab::Logs];
}
```

App 구조체에 필드 추가:
```rust
pub struct App {
    // ... 기존 필드 ...
    pub current_tab: Tab,
    pub tab_bar_visible: bool,
    pub log_entries: VecDeque<LogEntry>,
    pub log_verbose: bool,
    pub log_scroll_offset: usize,
    pub log_auto_scroll: bool,
}
```

`App::new()`에 초기값:
```rust
current_tab: Tab::Chat,
tab_bar_visible: true,
log_entries: VecDeque::new(),
log_verbose: false,
log_scroll_offset: 0,
log_auto_scroll: true,
```

`scroll_offset` 타입을 `u16` → `usize`로 변경 (통일).

- [ ] **Step 2: 로그 관련 헬퍼 메서드 추가**

```rust
pub fn push_log(&mut self, entry: LogEntry) {
    if self.log_entries.len() >= 1000 {
        self.log_entries.pop_front();
    }
    self.log_entries.push_back(entry);
    // 자동 스크롤: 최하단에 있으면 따라감
    if self.log_auto_scroll {
        self.log_scroll_offset = 0;
    }
}

pub fn visible_logs(&self) -> Vec<&LogEntry> {
    if self.log_verbose {
        self.log_entries.iter().collect()
    } else {
        self.log_entries.iter().filter(|e| e.structured).collect()
    }
}
```

- [ ] **Step 3: 기존 테스트에서 `scroll_offset` 타입 변경 반영**

`scroll_offset: u16` → `usize`로 변경했으므로 테스트의 리터럴도 확인. `0`, `4`, `3` 등은 두 타입 모두 호환이므로 변경 불필요할 가능성이 높음.

- [ ] **Step 4: 빌드 + 테스트**

Run: `cargo check -p opengoose && cargo test -p opengoose`
Expected: success

- [ ] **Step 5: 커밋**

```bash
git add crates/opengoose/src/tui/app.rs
git commit -m "feat: Tab enum + log buffer + verbose toggle in App state"
```

---

### Task 5: event.rs — 탭 전환 키 핸들링 + log_rx 수신

이벤트 루프에 탭 전환, 탭 바 토글, verbose 토글을 추가하고 `log_rx`에서 로그를 수신.

**Files:**
- Modify: `crates/opengoose/src/tui/event.rs`

- [ ] **Step 1: `run_tui` 시그니처 변경 + log_rx 수신 추가**

```rust
use super::log_entry::LogEntry;
use tokio::sync::mpsc;

pub async fn run_tui(
    board: Arc<Board>,
    operator: Arc<Operator>,
    mut log_rx: mpsc::Receiver<LogEntry>,
) -> Result<()> {
```

select 루프에 `log_rx` arm 추가:
```rust
// 로그 수신
Some(entry) = log_rx.recv() => {
    app.push_log(entry);
}
```

- [ ] **Step 2: 탭 전환 키 핸들링**

`handle_key`에서 탭 전환 추가. 기존 키 핸들링 전에 전역 단축키를 먼저 처리:

```rust
async fn handle_key(
    key: KeyEvent,
    app: &mut App,
    agent_tx: &mpsc::Sender<AgentMsg>,
    board: &Arc<Board>,
    operator: &Arc<Operator>,
) -> bool {
    match (key.code, key.modifiers) {
        // 종료
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
            app.should_quit = true;
            return true;
        }
        // 탭 전환: Ctrl+1/2/3
        (KeyCode::Char('1'), KeyModifiers::CONTROL) => {
            app.current_tab = Tab::Chat;
        }
        (KeyCode::Char('2'), KeyModifiers::CONTROL) => {
            app.current_tab = Tab::Board;
        }
        (KeyCode::Char('3'), KeyModifiers::CONTROL) => {
            app.current_tab = Tab::Logs;
        }
        // 탭 전환: Tab / Shift+Tab
        (KeyCode::Tab, KeyModifiers::NONE) => {
            app.current_tab = app.current_tab.next();
        }
        (KeyCode::BackTab, KeyModifiers::SHIFT) | (KeyCode::BackTab, _) => {
            app.current_tab = app.current_tab.prev();
        }
        // 탭 바 토글: Ctrl+\
        (KeyCode::Char('\\'), KeyModifiers::CONTROL) => {
            app.tab_bar_visible = !app.tab_bar_visible;
        }
        // Logs 탭: v = verbose 토글
        (KeyCode::Char('v'), KeyModifiers::NONE) if app.current_tab == Tab::Logs => {
            app.log_verbose = !app.log_verbose;
            app.log_scroll_offset = 0;
        }
        // Enter — Chat 탭에서만 입력 전송
        (KeyCode::Enter, _) if app.current_tab == Tab::Chat => {
            if let Some(text) = app.submit_input() {
                handle_input(app, &text, agent_tx, board, operator).await;
            }
        }
        // 텍스트 입력 — Chat 탭에서만
        (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT)
            if app.current_tab == Tab::Chat =>
        {
            let byte_pos = app.cursor_byte_pos();
            app.input.insert(byte_pos, c);
            app.cursor_pos += 1;
        }
        // Backspace — Chat 탭에서만
        (KeyCode::Backspace, _) if app.current_tab == Tab::Chat => {
            if app.cursor_pos > 0 {
                app.cursor_pos -= 1;
                let byte_pos = app.cursor_byte_pos();
                let ch = app.input[byte_pos..].chars().next().unwrap();
                app.input.replace_range(byte_pos..byte_pos + ch.len_utf8(), "");
            }
        }
        // Delete — Chat 탭에서만
        (KeyCode::Delete, _) if app.current_tab == Tab::Chat => {
            if app.cursor_pos < app.char_count() {
                let byte_pos = app.cursor_byte_pos();
                let ch = app.input[byte_pos..].chars().next().unwrap();
                app.input.replace_range(byte_pos..byte_pos + ch.len_utf8(), "");
            }
        }
        // 커서 이동 — Chat 탭에서만
        (KeyCode::Left, _) if app.current_tab == Tab::Chat => {
            app.cursor_pos = app.cursor_pos.saturating_sub(1);
        }
        (KeyCode::Right, _) if app.current_tab == Tab::Chat => {
            if app.cursor_pos < app.char_count() {
                app.cursor_pos += 1;
            }
        }
        (KeyCode::Home, _) if app.current_tab == Tab::Chat => {
            app.cursor_pos = 0;
        }
        (KeyCode::End, _) if app.current_tab == Tab::Chat => {
            app.cursor_pos = app.char_count();
        }
        // 스크롤 — 현재 탭에 따라 다르게
        (KeyCode::Up, KeyModifiers::NONE) => {
            match app.current_tab {
                Tab::Chat if app.input.is_empty() => {
                    app.scroll_offset = app.scroll_offset.saturating_add(1);
                }
                Tab::Logs => {
                    app.log_scroll_offset = app.log_scroll_offset.saturating_add(1);
                }
                _ => {}
            }
        }
        (KeyCode::Down, KeyModifiers::NONE) => {
            match app.current_tab {
                Tab::Chat if app.input.is_empty() => {
                    app.scroll_offset = app.scroll_offset.saturating_sub(1);
                }
                Tab::Logs => {
                    app.log_scroll_offset = app.log_scroll_offset.saturating_sub(1);
                }
                _ => {}
            }
        }
        (KeyCode::PageUp, _) => {
            match app.current_tab {
                Tab::Chat => {
                    app.scroll_offset = app.scroll_offset.saturating_add(10);
                }
                Tab::Logs => {
                    app.log_scroll_offset = app.log_scroll_offset.saturating_add(10);
                }
                _ => {}
            }
        }
        (KeyCode::PageDown, _) => {
            match app.current_tab {
                Tab::Chat => {
                    app.scroll_offset = app.scroll_offset.saturating_sub(10);
                }
                Tab::Logs => {
                    app.log_scroll_offset = app.log_scroll_offset.saturating_sub(10);
                }
                _ => {}
            }
        }
        _ => {}
    }

    false
}
```

- [ ] **Step 3: 빌드 확인**

Run: `cargo check -p opengoose`
Expected: success (Task 3과 함께 — subscriber 분기에서 `run_tui`에 `log_rx` 전달)

- [ ] **Step 4: 커밋**

```bash
git add crates/opengoose/src/tui/event.rs
git commit -m "feat: tab switching, tab bar toggle, verbose toggle, log_rx in TUI event loop"
```

---

### Task 6: ui.rs — 탭 기반 레이아웃 + Logs 뷰 렌더링

렌더링을 탭 기반으로 재구성.

**Files:**
- Modify: `crates/opengoose/src/tui/ui.rs`

- [ ] **Step 1: `render()` 함수를 탭 기반으로 재작성**

```rust
use super::app::{App, ChatLine, Tab};
use super::log_entry::LogEntry;

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();

    if app.tab_bar_visible {
        let chunks = Layout::vertical([
            Constraint::Length(1), // 탭 바
            Constraint::Min(1),   // 뷰 콘텐츠
        ])
        .split(area);

        render_tab_bar(frame, app, chunks[0]);
        render_current_tab(frame, app, chunks[1]);
    } else {
        render_current_tab(frame, app, area);
    }
}

fn render_tab_bar(frame: &mut Frame, app: &App, area: Rect) {
    let tabs: Vec<Span> = Tab::ALL
        .iter()
        .enumerate()
        .flat_map(|(i, tab)| {
            let style = if *tab == app.current_tab {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let mut spans = vec![
                Span::styled(format!(" {} ", tab.label()), style),
            ];
            if i < Tab::ALL.len() - 1 {
                spans.push(Span::styled(" │ ", Style::default().fg(Color::DarkGray)));
            }
            spans
        })
        .collect();

    let line = Line::from(tabs);
    frame.render_widget(Paragraph::new(line), area);
}

fn render_current_tab(frame: &mut Frame, app: &App, area: Rect) {
    match app.current_tab {
        Tab::Chat => render_chat_tab(frame, app, area),
        Tab::Board => render_board_tab(frame, app, area),
        Tab::Logs => render_logs_tab(frame, app, area),
    }
}
```

- [ ] **Step 2: Chat 탭 — 기존 Chat + Input 결합**

```rust
fn render_chat_tab(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::vertical([
        Constraint::Min(6),    // Chat
        Constraint::Length(3), // Input
    ])
    .split(area);

    render_chat(frame, app, chunks[0]);
    render_input(frame, app, chunks[1]);
}
```

기존 `render_chat`과 `render_input` 함수는 그대로 유지.

- [ ] **Step 3: Board 탭 — 기존 Board + Rigs**

```rust
fn render_board_tab(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::horizontal([
        Constraint::Percentage(65),
        Constraint::Percentage(35),
    ])
    .split(area);

    render_board(frame, app, chunks[0]);
    render_rigs(frame, app, chunks[1]);
}
```

기존 `render_board`와 `render_rigs` 함수는 그대로 유지.

- [ ] **Step 4: Logs 탭 — 새 렌더링 함수**

```rust
fn render_logs_tab(frame: &mut Frame, app: &App, area: Rect) {
    let inner_height = area.height.saturating_sub(2) as usize; // borders
    let visible = app.visible_logs();
    let total = visible.len();

    // 스크롤 계산 (scroll_offset 0 = 최하단)
    let skip = if app.log_scroll_offset == 0 {
        total.saturating_sub(inner_height)
    } else {
        total.saturating_sub(inner_height + app.log_scroll_offset)
    };

    let lines: Vec<Line> = visible
        .into_iter()
        .skip(skip)
        .take(inner_height)
        .map(|entry| format_log_entry(entry, app.log_verbose))
        .collect();

    let mode_label = if app.log_verbose { "verbose" } else { "structured" };
    let title = format!(" Logs ({mode_label}) — press v to toggle ");

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(Style::default().fg(Color::Blue)),
    );

    frame.render_widget(paragraph, area);
}

fn format_log_entry(entry: &LogEntry, verbose: bool) -> Line<'static> {
    let time = entry.timestamp.format("%H:%M:%S").to_string();

    if verbose {
        let level_style = match entry.level {
            tracing::Level::ERROR => Style::default().fg(Color::Red),
            tracing::Level::WARN => Style::default().fg(Color::Yellow),
            tracing::Level::INFO => Style::default().fg(Color::Green),
            tracing::Level::DEBUG => Style::default().fg(Color::DarkGray),
            tracing::Level::TRACE => Style::default().fg(Color::DarkGray),
        };

        Line::from(vec![
            Span::styled(time, Style::default().fg(Color::DarkGray)),
            Span::raw(" "),
            Span::styled(format!("{:<5}", entry.level), level_style),
            Span::raw(" "),
            Span::styled(entry.target.clone(), Style::default().fg(Color::DarkGray)),
            Span::raw("  "),
            Span::raw(entry.message.clone()),
        ])
    } else {
        // Structured 모드: 간결한 형태
        let source = if entry.target.contains("::rig") {
            "worker"
        } else if entry.target.contains("evolver") {
            "evolver"
        } else {
            "system"
        };

        Line::from(vec![
            Span::styled(time, Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(" [{source}] "),
                Style::default().fg(Color::Cyan),
            ),
            Span::raw(entry.message.clone()),
        ])
    }
}
```

- [ ] **Step 5: 기존 `render()` 에서 호출하던 `render_top` 제거**

`render_top`, `top_panel_height`는 더 이상 최상위에서 호출되지 않음 (Board 탭 내부에서만 사용). `render_top` → `render_board_tab`으로 대체되었으므로 `render_top`과 `top_panel_height` 삭제.

- [ ] **Step 6: `scroll_offset` 타입 변경 반영**

`render_chat`에서 `app.scroll_offset as usize` 캐스팅이 있으면 제거 (이미 `usize`).

- [ ] **Step 7: 빌드 + 테스트**

Run: `cargo check -p opengoose && cargo test -p opengoose`
Expected: ui.rs 테스트 중 `top_panel_height` 관련 테스트가 실패할 수 있음 → 삭제하거나 `render_board_tab`으로 리팩터링.

기존 테스트 `top_panel_height_*` 2개는 삭제된 함수를 테스트하므로 제거.

- [ ] **Step 8: 커밋**

```bash
git add crates/opengoose/src/tui/ui.rs
git commit -m "feat: tab-based TUI layout with Chat, Board, Logs views"
```

---

### Task 7: 통합 빌드 + 최종 검증

Task 3-6은 서로 의존하므로 모두 합쳐서 빌드/테스트.

**Files:** (없음 — 검증만)

- [ ] **Step 1: 전체 빌드**

Run: `cargo check`
Expected: success

- [ ] **Step 2: 전체 테스트**

Run: `cargo test`
Expected: 기존 테스트 통과 (top_panel_height 테스트 2개 제거 후)

- [ ] **Step 3: cargo clippy**

Run: `cargo clippy --all-targets`
Expected: 새 경고 없음

- [ ] **Step 4: 수동 스모크 테스트**

1. `opengoose` 실행 → TUI 시작 → 탭 바 표시 확인
2. `Tab` 키로 Chat → Board → Logs 전환
3. `Ctrl+\`로 탭 바 접기/펼치기
4. Chat 탭에서 `/task "echo hello"` → Board 탭에서 상태 확인 → Logs 탭에서 Worker 이벤트 확인
5. Logs 탭에서 `v` 키로 verbose 토글
6. `~/.opengoose/logs/opengoose-*.log` 파일 생성 확인

- [ ] **Step 5: 커밋 (필요 시)**

최종 수정사항이 있으면 커밋.
