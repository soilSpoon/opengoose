# Goose 심층 분석 — 소스코드 기반

> **분석일:** 2026-03-11
> **소스:** `/home/user/goose-upstream` (block/goose)
> **목적:** OpenGoose가 Goose-native하게 멀티에이전트 오케스트레이션을 구현하기 위한 내부 구조 분석

---

## 1. 서브에이전트 시스템

### 1.1 핵심 파일 구조

| 파일 | 역할 |
|------|------|
| `crates/goose/src/agents/subagent_handler.rs` | 서브에이전트 생성, 실행, 메시지 스트리밍 |
| `crates/goose/src/agents/subagent_task_config.rs` | TaskConfig 구조체 — 서브에이전트 실행 파라미터 |
| `crates/goose/src/agents/subagent_execution_tool/mod.rs` | TaskStatus re-export 모듈 |
| `crates/goose/src/agents/subagent_execution_tool/notification_events.rs` | 태스크 실행 알림 이벤트 타입 |
| `crates/goose/src/agents/platform_extensions/summon.rs` | Summon 확장 — delegate/load 도구 제공 |

### 1.2 SubagentRunParams — 서브에이전트 생성 파라미터

```rust
// subagent_handler.rs:37-46
pub struct SubagentRunParams {
    pub config: AgentConfig,
    pub recipe: Recipe,
    pub task_config: TaskConfig,
    pub return_last_only: bool,
    pub session_id: String,
    pub cancellation_token: Option<CancellationToken>,
    pub on_message: Option<OnMessageCallback>,
    pub notification_tx: Option<tokio::sync::mpsc::UnboundedSender<ServerNotification>>,
}
```

**핵심 필드 분석:**

- **`config: AgentConfig`** — 새 Agent 인스턴스 생성 시 사용되는 설정. `Agent::with_config(config)`로 자식 에이전트가 생성됨 (L141)
- **`recipe: Recipe`** — 서브에이전트의 지시사항(instructions), 프롬프트(prompt), 응답 스키마(response)를 포함
- **`task_config: TaskConfig`** — Provider, 확장 목록, max_turns, 부모 세션 정보
- **`cancellation_token: Option<CancellationToken>`** — tokio_util의 CancellationToken으로 외부에서 태스크 취소 가능
- **`on_message: Option<OnMessageCallback>`** — 스트리밍 감독용 콜백. 타입: `Arc<dyn Fn(&Message) + Send + Sync>` (L23)
- **`notification_tx`** — MCP ServerNotification을 부모에게 전파하는 unbounded 채널

### 1.3 TaskConfig — 서브에이전트 실행 설정

```rust
// subagent_task_config.rs:9-19
pub const DEFAULT_SUBAGENT_MAX_TURNS: usize = 25;

pub struct TaskConfig {
    pub provider: Arc<dyn Provider>,
    pub parent_session_id: String,
    pub parent_working_dir: PathBuf,
    pub extensions: Vec<ExtensionConfig>,
    pub max_turns: Option<usize>,
}
```

- `max_turns`는 환경변수 `GOOSE_SUBAGENT_MAX_TURNS`로 전역 설정 가능 (L47)
- 기본값 25턴은 복잡한 태스크에 충분한 여유를 줌
- `with_max_turns()` 빌더 패턴으로 레시피별 오버라이드 지원 (L53-58)

### 1.4 서브에이전트 실행 흐름

`run_subagent_task()` (L48-63)의 실행 순서:

1. **Agent 생성**: `Agent::with_config(config)` — Arc로 래핑
2. **Provider 설정**: `agent.update_provider(task_config.provider, &session_id)` (L143-146)
3. **확장 등록**: 부모의 extensions를 순회하며 `agent.add_extension()` 호출 (L148-156)
4. **레시피 응답 스키마 적용**: `agent.apply_recipe_components(recipe.response, true)` (L159-161)
5. **시스템 프롬프트 빌드**: `build_subagent_prompt()` — 도구 목록, max_turns, task_instructions를 템플릿에 주입 (L163-165)
6. **대화 시작**: `agent.reply(user_message, session_config, cancellation_token)` (L183-188)
7. **스트리밍 감독**: `stream.next()` 루프에서 AgentEvent를 수신하며 `on_message` 콜백 실행 (L191-220)
8. **결과 추출**: 응답 스키마가 있으면 `final_output_tool`에서, 없으면 텍스트 추출 (L222-224)

### 1.5 알림 이벤트 시스템

```rust
// notification_events.rs:4-10
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
}
```

```rust
// notification_events.rs:23-38
pub enum TaskExecutionNotificationEvent {
    LineOutput { task_id: String, output: String },
    TasksUpdate { stats: TaskExecutionStats, tasks: Vec<TaskInfo> },
    TasksComplete { stats: TaskCompletionStats, failed_tasks: Vec<FailedTaskInfo> },
}
```

`TaskInfo` 구조체 (L57-68)는 각 서브태스크의 상태를 실시간 추적:
- `id`, `status`, `duration_secs` — 기본 메타
- `current_output` — 현재 출력 (스트리밍)
- `task_type`, `task_name`, `task_metadata` — 분류 정보
- `result_data: Option<Value>` — 구조화된 결과

### 1.6 도구 알림 전파

`create_tool_notification()` (L267-293)은 서브에이전트의 ToolRequest를 MCP LoggingMessageNotification으로 변환하여 부모에게 전달:

```rust
// subagent_handler.rs:120
pub const SUBAGENT_TOOL_REQUEST_TYPE: &str = "subagent_tool_request";
```

이 메커니즘으로 부모 에이전트는 자식의 도구 호출을 실시간 모니터링 가능.

### 1.7 SessionExecutionMode — 세션 격리

```rust
// execution/mod.rs:12-16
pub enum SessionExecutionMode {
    Interactive,
    Background,
    SubTask { parent_session: String },
}
```

`SubTask` 모드는 서브에이전트가 부모 세션을 참조하면서도 격리된 컨텍스트에서 실행되도록 보장.

### 1.8 OpenGoose FanOutExecutor와의 매핑

OpenGoose의 `crates/opengoose-teams/src/fan_out_executor.rs`는 `JoinSet`을 사용한 병렬 실행 패턴:

```rust
// fan_out_executor.rs:51
let mut join_set = JoinSet::new();
```

**매핑 분석:**

| Goose 서브에이전트 | OpenGoose FanOutExecutor |
|---|---|
| `SubagentRunParams` | `AgentRunner::from_profile_keyed()` |
| `CancellationToken` | JoinSet의 `abort_all()` |
| `on_message` 콜백 | `[BROADCAST]:` 프리픽스 기반 에이전트 간 통신 |
| `notification_tx` 채널 | `process_agent_communications()` |
| `TaskConfig.max_turns` | 프로필 레벨 설정 |

**갭**: OpenGoose는 Goose의 `SubagentRunParams`를 직접 사용하지 않고 자체 `AgentRunner` 추상화를 사용. Goose의 `on_message` 콜백이 더 세밀한 감독을 제공하므로, v2에서 직접 통합하면 `[BROADCAST]:` 파싱 대신 구조화된 알림을 활용 가능.

---

## 2. 퍼미션 시스템

### 2.1 GooseMode — 4가지 운영 모드

```rust
// config/goose_mode.rs:4-24
pub enum GooseMode {
    Auto,           // 모든 도구 자동 승인
    Approve,        // 모든 도구에 사용자 승인 필요
    SmartApprove,   // 읽기 전용은 자동, 쓰기는 승인 필요
    Chat,           // 도구 사용 불가, 대화만
}
```

### 2.2 PermissionLevel — 3가지 정책

```rust
// config/permission.rs:18-23
pub enum PermissionLevel {
    AlwaysAllow,  // 프롬프트 없이 항상 허용
    AskBefore,    // 사용 전 승인 필요
    NeverAllow,   // 절대 허용 안 함
}
```

### 2.3 PermissionManager — 전역 싱글턴

```rust
// config/permission.rs:34-38
pub struct PermissionManager {
    config_path: PathBuf,
    permission_map: RwLock<HashMap<String, PermissionConfig>>,
}
```

- `LazyLock` 기반 전역 싱글턴 (`PERMISSION_MANAGER`, L13-14)
- YAML 파일 기반 영속화 (`permission.yaml`)
- 두 가지 권한 네임스페이스: `"user"` (사용자 명시적 설정)와 `"smart_approve"` (LLM 판단 캐시)

`PermissionConfig` (L26-31):
```rust
pub struct PermissionConfig {
    pub always_allow: Vec<String>,
    pub ask_before: Vec<String>,
    pub never_allow: Vec<String>,
}
```

### 2.4 도구 호출 인가 파이프라인

`PermissionInspector` (permission_inspector.rs:15-19)가 `ToolInspector` 트레이트를 구현:

```rust
pub struct PermissionInspector {
    pub permission_manager: Arc<PermissionManager>,
    provider: SharedProvider,
    readonly_tools: RwLock<HashSet<String>>,
}
```

**`inspect()` 메서드의 판단 로직** (L124-261):

1. **GooseMode::Chat** → 건너뜀 (도구 사용 불가)
2. **GooseMode::Auto** → 무조건 `InspectionAction::Allow`
3. **GooseMode::Approve / SmartApprove** → 5단계 판단:
   - (1) 사용자 정의 퍼미션 확인 (`get_user_permission`)
   - (2) MCP 도구 어노테이션 기반 read-only 확인 (`is_readonly_annotated_tool`)
   - (3) SmartApprove 캐시 확인 (`get_smart_approve_permission`)
   - (4) 확장 관리 도구는 항상 승인 필요 (`MANAGE_EXTENSIONS_TOOL_NAME_COMPLETE`)
   - (5) SmartApprove에서 캐시 미스 시 LLM 기반 읽기 전용 감지

### 2.5 LLM 기반 읽기 전용 감지 (Smart Approval)

`detect_read_only_tools()` (permission_judge.rs:122-154):

- `platform__tool_by_tool_permission` 도구를 정의하여 Provider에게 읽기 전용 분석 요청
- 도구 요청 목록을 LLM에 전달 → 읽기 전용 도구 목록을 반환
- 결과를 `PermissionManager`의 `smart_approve` 네임스페이스에 캐시
- 캐시된 후에는 LLM 호출 없이 즉시 판단

### 2.6 PermissionStore — Blake3 해싱 기반 영속화

```rust
// permission_store.rs:11-20
pub struct ToolPermissionRecord {
    tool_name: String,
    allowed: bool,
    context_hash: String,        // Blake3 해시
    readable_context: Option<String>,
    timestamp: i64,
    expiry: Option<i64>,         // TTL 지원
}
```

```rust
// permission_store.rs:23-28
pub struct ToolPermissionStore {
    permissions: HashMap<String, Vec<ToolPermissionRecord>>,
    version: u32,
    permissions_dir: PathBuf,    // ~/.config/goose/permissions/
}
```

- 도구 인자를 Blake3로 해싱하여 동일 도구의 다른 컨텍스트를 구분 (L117-127)
- JSON 파일로 영속화 (`tool_permissions.json`)
- 원자적 쓰기: tmp 파일 작성 후 rename (L63-76)
- TTL 기반 만료 + 로드 시 자동 정리 (`cleanup_expired`, L129-143)

### 2.7 PermissionConfirmation — UI 상호작용

```rust
// permission_confirmation.rs:6-12
pub enum Permission {
    AlwaysAllow,
    AllowOnce,
    Cancel,
    DenyOnce,
    AlwaysDeny,
}
```

`PrincipalType`이 `Extension`과 `Tool` 레벨의 구분을 지원 (L15-18).

### 2.8 에이전트 감독 관점

**ManualApproval 모드의 서브에이전트 감독 활용:**

서브에이전트를 `GooseMode::Approve`로 실행하면, 모든 도구 호출이 부모 에이전트 또는 사용자의 승인을 거쳐야 함. `on_message` 콜백과 결합하면:

1. 자식 에이전트가 도구 호출 시도
2. `PermissionInspector`가 `RequireApproval` 반환
3. `ActionRequired` 이벤트 생성
4. 부모의 `on_message` 콜백에서 승인/거부 결정

이 패턴은 OpenGoose의 멀티에이전트 시스템에서 "감독 에이전트"가 "작업 에이전트"를 통제하는 데 직접 활용 가능.

---

## 3. Recipe 시스템 내부

### 3.1 Recipe 구조체

```rust
// recipe/mod.rs:41-86
pub struct Recipe {
    pub version: String,
    pub title: String,
    pub description: String,
    pub instructions: Option<String>,
    pub prompt: Option<String>,
    pub extensions: Option<Vec<ExtensionConfig>>,
    pub settings: Option<Settings>,
    pub activities: Option<Vec<String>>,
    pub author: Option<Author>,
    pub parameters: Option<Vec<RecipeParameter>>,
    pub response: Option<Response>,
    pub sub_recipes: Option<Vec<SubRecipe>>,
    pub retry: Option<RetryConfig>,
}
```

### 3.2 SubRecipe — 서브레시피 시스템

```rust
// recipe/mod.rs:118-128
pub struct SubRecipe {
    pub name: String,
    pub path: String,
    pub values: Option<HashMap<String, String>>,
    pub sequential_when_repeated: bool,    // 반복 시 순차 실행 플래그
    pub description: Option<String>,
}
```

- `sequential_when_repeated: bool` — 동일 서브레시피가 여러 번 호출될 때 순차 실행을 강제
- `values` — 파라미터 값을 주입하여 서브레시피 커스터마이징
- `ensure_summon_for_subrecipes()` (L254-270) — 서브레시피가 있으면 자동으로 `summon` 확장 추가

### 3.3 RetryConfig — 재시도 시스템

```rust
// agents/types.rs:22-37
pub struct RetryConfig {
    pub max_retries: u32,
    pub checks: Vec<SuccessCheck>,
    pub on_failure: Option<String>,
    pub timeout_seconds: Option<u64>,           // 기본 300초
    pub on_failure_timeout_seconds: Option<u64>, // 기본 600초
}
```

```rust
// agents/types.rs:66-74
pub enum SuccessCheck {
    Shell { command: String },
}
```

- `SuccessCheck::Shell` — 셸 명령어 실행 결과의 종료 코드로 성공 여부 판단
- `RetryManager` (retry.rs:41-46)가 재시도 상태를 관리
- 재시도 시 `RepetitionInspector`도 리셋하여 반복 감지 오탐 방지

### 3.4 RecipeParameter — 타입화된 입력

```rust
// recipe/mod.rs:173-184
pub enum RecipeParameterInputType {
    String,
    Number,
    Boolean,
    Date,
    File,     // 파일 경로에서 내용 임포트. 보안상 기본값 불가
    Select,   // 선택지 목록에서 선택
}
```

```rust
// recipe/mod.rs:196-206
pub struct RecipeParameter {
    pub key: String,
    pub input_type: RecipeParameterInputType,
    pub requirement: RecipeParameterRequirement,
    pub description: String,
    pub default: Option<String>,
    pub options: Option<Vec<String>>,   // Select 타입 전용
}
```

`RecipeParameterRequirement` (L155-161): `Required`, `Optional`, `UserPrompt`

### 3.5 Response — 구조화된 JSON 출력

```rust
// recipe/mod.rs:112-116
pub struct Response {
    pub json_schema: Option<serde_json::Value>,
}
```

응답 스키마가 설정되면 `FinalOutputTool`이 에이전트에 주입되어 구조화된 JSON 출력을 강제.

### 3.6 레시피 로딩/빌드/검증 파이프라인

1. **로딩**: `Recipe::from_file_path()` → `read_recipe_file()` → YAML/JSON 파싱
2. **중첩 처리**: `recipe:` 키 아래 중첩된 형식 지원 (L323-335)
3. **자동 확장 주입**:
   - `ensure_analyze_for_developer()` — developer 확장이 있으면 analyze 자동 추가 (L230-252)
   - `ensure_summon_for_subrecipes()` — sub_recipes가 있으면 summon 자동 추가 (L254-270)
4. **보안 검증**: `check_for_security_warnings()` — Unicode 태그 공격 감지 (L273-289)
5. **빌더 패턴**: `RecipeBuilder`로 프로그래매틱 생성 (L208-433)

### 3.7 OpenGoose의 goose_bridge.rs와의 비교

OpenGoose의 `crates/opengoose-profiles/src/goose_bridge.rs`는 프로필 디렉토리를 `GOOSE_RECIPE_PATH` 환경변수에 등록하여 Goose의 Summon 확장이 OpenGoose 프로필을 서브레시피로 발견할 수 있게 함:

```rust
// goose_bridge.rs:19
pub fn register_profiles_path(profiles_dir: &Path) -> ProfileResult<()>
```

이는 Goose의 `local_recipes::load_local_recipe_file()`과 Summon의 `Source` 디스커버리에 직접 연동됨.

---

## 4. Extension 디스패치

### 4.1 ExtensionManager 구조

```rust
// agents/extension_manager.rs:107-115
pub struct ExtensionManager {
    extensions: Mutex<HashMap<String, Extension>>,
    context: PlatformExtensionContext,
    provider: SharedProvider,
    tools_cache: Mutex<Option<Arc<Vec<Tool>>>>,
    tools_cache_version: AtomicU64,
    client_name: String,
    capabilities: ExtensionManagerCapabilities,
}
```

- `extensions` — 이름으로 인덱싱된 Extension 맵 (Mutex 보호)
- `tools_cache` — 도구 목록 캐시 (모든 확장의 도구를 평탄화하여 통합)
- `tools_cache_version` — AtomicU64로 캐시 무효화 관리
- `capabilities` — `mcpui` 플래그 (MCP UI 프록시 지원 여부)

### 4.2 Extension 내부 구조

```rust
// agents/extension_manager.rs:61-100
struct Extension {
    pub config: ExtensionConfig,
    client: McpClientBox,           // Arc<dyn McpClientTrait>
    server_info: Option<ServerInfo>,
    _temp_dir: Option<TempDir>,     // stdio 확장의 임시 디렉토리
}
```

각 Extension은:
- MCP 클라이언트를 래핑 (`McpClientTrait`)
- 서버 정보에서 리소스 지원 여부 확인 (`supports_resources()`)
- 서버 지시사항 추출 (`get_instructions()`)

### 4.3 도구 목록 통합 (Tool Aggregation)

ExtensionManager가 모든 확장에서 도구를 수집하여 단일 벡터로 통합. 도구 이름은 `{extension_name}__{tool_name}` 형식으로 네임스페이싱됨. 이를 통해:

- 다른 확장의 동명 도구 충돌 방지
- 도구 호출 시 타겟 확장 식별
- 퍼미션 시스템에서 확장 단위 제어 가능

### 4.4 플랫폼 확장 (Platform Extensions)

`PLATFORM_EXTENSIONS` 상수로 등록되는 내장 확장:

| 확장 | 파일 | 역할 |
|------|------|------|
| developer | `platform_extensions/developer/` | shell, edit, tree 도구 |
| analyze | `platform_extensions/analyze/` | 코드 분석, 의존성 그래프 |
| summon | `platform_extensions/summon.rs` | 서브에이전트 delegate/load |
| todo | `platform_extensions/todo.rs` | 태스크 목록 관리 |
| apps | `platform_extensions/apps.rs` | MCP 앱 관리 |
| summarize | `platform_extensions/summarize.rs` | 요약 도구 |
| chatrecall | `platform_extensions/chatrecall.rs` | 대화 기억 |
| tom | `platform_extensions/tom.rs` | Team of Models |
| code_execution | `platform_extensions/code_execution.rs` | 코드 실행 |

### 4.5 PlatformExtensionContext 알림

플랫폼 확장은 `PlatformExtensionContext`를 통해 에이전트와 통신:
- 확장 관리 알림
- 도구 캐시 무효화 신호
- Provider 상태 공유

### 4.6 MCP 디스패치 메커니즘

도구 호출 시:
1. Agent가 도구 이름에서 확장 이름 파싱 (`extension_name__tool_name`)
2. ExtensionManager에서 해당 Extension 조회
3. `McpClientTrait.call_tool()` 호출 — MCP 프로토콜로 확장에 전달
4. 확장이 결과 반환 → `CallToolResult`로 변환
5. 플랫폼 확장은 직접 Rust 함수 호출 (MCP 오버헤드 없음)

### 4.7 Frontend vs Backend 도구 분류

- **Backend 도구**: MCP 서버/플랫폼 확장에서 실행
- **Frontend 도구**: `FrontendTool` 타입 — UI에서 실행 (예: 파일 선택 다이얼로그)
- `FrontendToolRequest` 메시지 콘텐츠 타입으로 구분

### 4.8 보안/퍼미션/반복 검사 파이프라인

도구 호출 전 3단계 검사:

1. **PermissionInspector** — 퍼미션 시스템 (섹션 2 참조)
2. **SecurityInspector** — 보안 위협 감지 (`security/security_inspector.rs`)
3. **RepetitionInspector** — 동일 도구 반복 호출 감지 (`tool_monitor.rs`)

`ToolInspectionManager`가 이 3개 인스펙터를 조율하여 `InspectionResult` 벡터를 생성, `process_inspection_results()`로 최종 `PermissionCheckResult`를 산출.

---

## 5. 서버 모드 (goose-server)

### 5.1 Axum HTTP API 라우트 구성

```rust
// routes/mod.rs:28-48
pub fn configure(state: Arc<AppState>, secret_key: String) -> Router {
    Router::new()
        .merge(status::routes(state.clone()))
        .merge(reply::routes(state.clone()))
        .merge(action_required::routes(state.clone()))
        .merge(agent::routes(state.clone()))
        .merge(dictation::routes(state.clone()))
        .merge(local_inference::routes(state.clone()))
        .merge(config_management::routes(state.clone()))
        .merge(prompts::routes())
        .merge(recipe::routes(state.clone()))
        .merge(session::routes(state.clone()))
        .merge(schedule::routes(state.clone()))
        .merge(setup::routes(state.clone()))
        .merge(telemetry::routes(state.clone()))
        .merge(tunnel::routes(state.clone()))
        .merge(gateway::routes(state.clone()))
        .merge(mcp_ui_proxy::routes(secret_key.clone()))
        .merge(mcp_app_proxy::routes(secret_key))
        .merge(sampling::routes(state))
}
```

**주요 라우트 그룹:**

| 모듈 | 엔드포인트 | 역할 |
|------|-----------|------|
| `reply` | `POST /reply` | 에이전트 메시지 전송, SSE 스트리밍 응답 |
| `agent` | `POST /agent/start`, `POST /agent/stop` 등 | 에이전트 라이프사이클 |
| `session` | `GET/PUT/DELETE /sessions/{id}` | 세션 CRUD |
| `recipe` | 레시피 관련 | 레시피 로딩/실행 |
| `action_required` | 승인 요청/응답 | 도구 승인 워크플로우 |
| `schedule` | 스케줄 관리 | 예약 실행 |
| `config_management` | 설정 관리 | 프로바이더/모드 설정 |
| `gateway` | 게이트웨이 | 텔레그램 등 외부 연동 |
| `sampling` | 샘플링 | MCP 샘플링 지원 |

### 5.2 SSE 스트리밍 — AgentEvent 타입

```rust
// agents/agent.rs:155-160
pub enum AgentEvent {
    Message(Message),
    McpNotification((String, ServerNotification)),
    ModelChange { model: String, mode: String },
    HistoryReplaced(Conversation),
}
```

`reply` 라우트에서 SSE 스트림을 통해 이벤트를 클라이언트에 전달:
- `Message` — 에이전트 메시지 (텍스트, 도구 호출/응답 포함)
- `McpNotification` — MCP 서버 알림 (서브에이전트 진행 상황 등)
- `ModelChange` — 모델/모드 변경 알림
- `HistoryReplaced` — 컨텍스트 압축 후 대화 이력 교체

### 5.3 AppState — 서버 상태 관리

```rust
// state.rs:21-29
pub struct AppState {
    pub(crate) agent_manager: Arc<AgentManager>,
    pub recipe_file_hash_map: Arc<Mutex<HashMap<String, PathBuf>>>,
    recipe_session_tracker: Arc<Mutex<HashSet<String>>>,
    pub tunnel_manager: Arc<TunnelManager>,
    pub gateway_manager: Arc<GatewayManager>,
    pub extension_loading_tasks: ExtensionLoadingTasks,
    pub inference_runtime: Arc<InferenceRuntime>,
}
```

- `AgentManager` — 에이전트 인스턴스 풀 관리, 세션별 에이전트 생성/조회
- `recipe_file_hash_map` — 레시피 파일 해시 → 경로 매핑
- `recipe_session_tracker` — 레시피 중복 실행 방지
- `extension_loading_tasks` — 백그라운드 확장 로딩 태스크 관리

### 5.4 에이전트 시작 요청

```rust
// routes/agent.rs:60-70
pub struct StartAgentRequest {
    working_dir: String,
    recipe: Option<Recipe>,
    recipe_id: Option<String>,
    recipe_deeplink: Option<String>,
    extension_overrides: Option<Vec<ExtensionConfig>>,
}
```

### 5.5 세션 관리 API

```rust
// routes/session.rs:69-92
// GET /sessions — 세션 목록 조회
// GET /sessions/{session_id} — 세션 상세
// PUT /sessions/{session_id} — 세션 이름 수정
// DELETE /sessions/{session_id} — 세션 삭제
// POST /sessions/{session_id}/fork — 세션 분기
// POST /sessions/import — 세션 가져오기
```

### 5.6 opengoose-web과의 중복 분석

OpenGoose의 `opengoose-web` 크레이트가 자체 웹 서버를 제공하는 반면, goose-server는 완전한 Axum 기반 HTTP 서버를 제공. 주요 차이:

- **goose-server**: 세션/에이전트/레시피의 완전한 REST API + SSE 스트리밍 + OpenAPI 문서
- **opengoose-web**: 가벼운 대시보드 + OpenGoose 특화 기능 (팀, 프로필, 스케줄)

통합 가능성: goose-server의 세션/에이전트 API를 opengoose-web의 백엔드로 활용하여 중복 제거.

---

## 6. 컨텍스트 관리

### 6.1 핵심 상수 및 설정

```rust
// context_mgmt/mod.rs:19
pub const DEFAULT_COMPACTION_THRESHOLD: f64 = 0.8;  // 80% 사용 시 자동 압축
```

환경변수 `GOOSE_AUTO_COMPACT_THRESHOLD`로 오버라이드 가능.
임계값이 0.0 이하 또는 1.0 이상이면 자동 압축 비활성화 (L217-219).

### 6.2 fix_conversation() 파이프라인

`conversation.rs`에 정의된 대화 수정 함수. 메시지 순서/구조를 Provider가 기대하는 형식으로 정규화:
- 연속된 동일 역할 메시지 병합
- 도구 요청/응답 쌍 정합성 검증
- 빈 메시지 제거

### 6.3 자동 컨텍스트 압축

**`check_if_compaction_needed()`** (L182-223):

1. Provider의 컨텍스트 리밋 조회 (`context_limit()`)
2. 현재 토큰 수 계산 (세션 메타데이터 우선, 없으면 토큰 카운터)
3. 사용 비율 = 현재 토큰 / 컨텍스트 리밋
4. 비율 > 임계값이면 압축 필요

**`compact_messages()`** (L62-179):

1. 가장 최근 사용자 메시지를 보존 대상으로 마킹
2. `do_compact()` 호출하여 요약 생성
3. 원본 메시지를 `agent_invisible`로 설정 (사용자에게는 보이지만 에이전트에게는 숨김)
4. 요약 메시지를 `agent_only`로 설정 (에이전트에게만 보임)
5. 계속 메시지 추가 ("context was compacted" 안내)
6. 보존된 사용자 메시지를 새 메시지로 추가

### 6.4 점진적 도구 응답 제거 (Progressive Removal)

`do_compact()` (L275-340)은 컨텍스트 리밋 초과 시 도구 응답을 점진적으로 제거:

```rust
// context_mgmt/mod.rs:287
let removal_percentages = [0, 10, 20, 50, 100];
```

- 0% 제거로 시도 → 실패 시 10% → 20% → 50% → 100%
- `filter_tool_responses()` (L225-273)는 "middle-out" 전략으로 중간부터 제거
- `ContextLengthExceeded` 에러 시에만 다음 단계로 진행

### 6.5 Shadow Map — 보이지 않는 메시지 보존

`MessageMetadata`의 visibility 시스템:

| 플래그 | 설명 |
|--------|------|
| `agent_visible: true, user_visible: true` | 일반 메시지 |
| `agent_visible: false, user_visible: true` | 압축된 원본 (사용자 UI에서 보임) |
| `agent_visible: true, user_visible: false` | 요약/시스템 메시지 (에이전트만 봄) |
| `agent_visible: false, user_visible: false` | 완전히 숨겨진 메시지 |

이 이중 가시성 시스템으로:
- 사용자는 전체 대화 이력을 유지
- 에이전트는 요약된 컨텍스트만 사용
- UI에서 원본 메시지 펼쳐보기 가능

### 6.6 HistoryReplaced 이벤트 전파

```rust
// agents/agent.rs:159
HistoryReplaced(Conversation),
```

컨텍스트 압축 후 `AgentEvent::HistoryReplaced`가 스트림을 통해 전파. 서브에이전트의 경우 `subagent_handler.rs` L212-214에서 이를 처리:

```rust
Ok(AgentEvent::HistoryReplaced(updated_conversation)) => {
    conversation = updated_conversation;
}
```

### 6.7 도구 쌍 요약 (Tool Pair Summarization)

**현재 비활성화** (`ENABLE_TOOL_PAIR_SUMMARIZATION: bool = false`, L24)

활성화 시 동작:
- `tool_id_to_summarize()` — cutoff 이상의 도구 호출 존재 시 가장 오래된 ID 반환
- `summarize_tool_call()` — 해당 도구 호출/응답 쌍을 LLM으로 요약
- 요약된 메시지는 `agent_only`로, 원본은 `agent_invisible`로 설정

---

## 7. OpenGoose 활용 전략 — Gap 분석

### 7.1 서브에이전트 시스템

| 항목 | 현재 상태 | 권장 |
|------|-----------|------|
| Goose `SubagentRunParams` | 미사용 — OpenGoose는 자체 `AgentRunner` 사용 | v2에서 `SubagentRunParams` 직접 활용 검토 |
| `CancellationToken` | `JoinSet.abort_all()`로 대체 | Goose의 토큰 기반 취소가 더 세밀 — 개별 서브에이전트 취소 가능 |
| `on_message` 콜백 | `[BROADCAST]:` 텍스트 파싱 | Goose의 구조화된 콜백으로 전환 시 파싱 오류 제거 |
| `notification_tx` | 미사용 | 서브에이전트 진행률 UI에 직접 활용 가능 |
| `TaskExecutionNotificationEvent` | 미사용 | 팬아웃 실행 통계(성공률, 소요시간)를 UI에 표시 |
| Summon 확장 통합 | `goose_bridge.rs`로 경로 등록 | 이미 활용 중 — 프로필을 레시피로 디스커버리 |

**v2 권장 아키텍처:**

OpenGoose의 `FanOutExecutor`가 Goose의 `SubagentRunParams`를 직접 생성하여:
- `on_message`로 실시간 진행 상황 수집
- `notification_tx`로 UI 스트리밍
- `CancellationToken`으로 타임아웃/실패 시 개별 취소

### 7.2 퍼미션 시스템

| 항목 | 현재 상태 | 권장 |
|------|-----------|------|
| GooseMode 4종 | 기본 Auto 모드만 사용 | 팀별 GooseMode 설정 — 위험한 에이전트는 SmartApprove |
| PermissionManager | Goose 기본 사용 | OpenGoose 프로필에 per-agent 퍼미션 레벨 추가 |
| Smart Approval | 미활용 | LLM 기반 읽기 전용 감지를 자동 활성화 |
| ToolPermissionStore | 미활용 | 팀 실행 시 도구별 허용 목록을 YAML로 관리 |
| SecurityInspector | Goose 기본 사용 | 추가 커스터마이징 불필요 |
| 감독 에이전트 패턴 | 미구현 | `GooseMode::Approve` + `on_message`로 감독 구현 가능 |

**OpenGoose 고유 구현 필요:**
- 팀 레벨 퍼미션 정책 (팀 정의 파일에 GooseMode 포함)
- 에이전트 간 퍼미션 위임 (감독자가 작업자의 도구 승인)

### 7.3 Recipe 시스템

| 항목 | 현재 상태 | 권장 |
|------|-----------|------|
| Recipe 구조체 | `goose_bridge.rs`로 프로필→레시피 연동 | 이미 활용 중 |
| SubRecipe | 미활용 | 팀 워크플로우를 sub_recipes로 표현 가능 |
| `sequential_when_repeated` | 미활용 | 체인 실행 시 순차 보장에 활용 |
| RetryConfig | 미활용 | 스케줄 실행에서 retry 로직 활용 검토 |
| RecipeParameter | 미활용 | OpenGoose CLI에서 프로필 파라미터를 RecipeParameter로 매핑 |
| Response (JSON 스키마) | 미활용 | 구조화된 에이전트 출력에 활용 — 팀 merge_strategy에 유용 |

**v2 양방향 변환 설계:**

현재 `goose_bridge.rs`는 단방향(프로필→Goose)이지만, Goose 레시피를 OpenGoose 프로필로 역변환하면:
- Goose 생태계의 레시피를 OpenGoose 팀에서 직접 사용
- `sub_recipes`를 팀의 `fan_out` 에이전트 목록으로 매핑
- `sequential_when_repeated`를 체인 실행기에서 활용

### 7.4 Extension 디스패치

| 항목 | 현재 상태 | 권장 |
|------|-----------|------|
| ExtensionManager | Goose Agent 내부에서 자동 사용 | 기본 동작 유지 |
| 플랫폼 확장 | Goose 기본 사용 | developer, analyze 등은 그대로 활용 |
| Summon delegate | 프로필 디스커버리에 활용 | 서브에이전트 실행도 Summon 경유로 전환 검토 |
| 도구 어노테이션 | 미활용 | OpenGoose 커스텀 MCP 서버에 `read_only_hint` 추가 |
| 확장 멀웨어 검사 | Goose 기본 사용 | `extension_malware_check.rs` 활용 유지 |

**OpenGoose 고유 구현 필요:**
- 팀 컨텍스트에서 확장 공유/격리 정책
- 에이전트별 확장 프리셋 (프로필에 extensions 명시)

### 7.5 서버 모드

| 항목 | 현재 상태 | 권장 |
|------|-----------|------|
| goose-server API | 미사용 (OpenGoose 자체 웹 서버) | 통합 검토 — goose-server를 백엔드로 활용 |
| SSE 스트리밍 | opengoose-core의 자체 스트리밍 | AgentEvent 기반 SSE로 통일 가능 |
| Session CRUD | opengoose-persistence로 자체 관리 | 이중 관리 제거 — Goose SessionManager 단일 사용 검토 |
| OpenAPI 문서 | goose-server에 utoipa 기반 | opengoose-web에도 OpenAPI 추가 |
| 에이전트 매니저 | `AgentManager` 싱글턴 | OpenGoose가 이미 간접 사용 중 |

### 7.6 컨텍스트 관리

| 항목 | 현재 상태 | 권장 |
|------|-----------|------|
| 자동 압축 (80%) | Goose 기본 동작 | 유지 — 장시간 에이전트 세션에 필수 |
| 점진적 도구 제거 | Goose 내부 동작 | 유지 |
| Shadow map 가시성 | Goose 내부 동작 | opengoose-tui에서 invisible 메시지 표시 기능 검토 |
| HistoryReplaced | Goose 내부 처리 | FanOutExecutor에서 이벤트 전파 확인 필요 |
| 도구 쌍 요약 | 현재 비활성화 | Goose 업스트림에서 활성화 시 자동 혜택 |
| 토큰 카운터 | Goose 기본 사용 | 유지 |

**OpenGoose 고유 구현 필요:**
- 팀 레벨 컨텍스트 공유 (에이전트 간 요약 전파)
- 압축 임계값의 프로필별 커스터마이징
- 멀티에이전트 환경에서의 토큰 예산 관리

### 7.7 종합 우선순위

| 우선순위 | 작업 | 난이도 | 영향도 |
|---------|------|--------|--------|
| **P0** | `SubagentRunParams` + `on_message` 통합 | 중 | 높음 |
| **P0** | `Response` JSON 스키마로 구조화된 팀 출력 | 하 | 높음 |
| **P1** | 프로필별 `GooseMode` 설정 | 하 | 중 |
| **P1** | `RetryConfig`를 스케줄 실행에 활용 | 중 | 중 |
| **P1** | `notification_tx`로 팬아웃 진행률 스트리밍 | 중 | 중 |
| **P2** | SubRecipe → 팀 워크플로우 매핑 | 중 | 중 |
| **P2** | goose-server API를 opengoose-web 백엔드로 | 높 | 중 |
| **P2** | ToolPermissionStore 기반 팀별 퍼미션 | 중 | 하 |
| **P3** | 감독 에이전트 패턴 (Approve + on_message) | 높 | 높음 |
| **P3** | 팀 레벨 컨텍스트 공유/예산 관리 | 높 | 중 |

---

## 부록: 핵심 타입 시그니처 요약

```rust
// 서브에이전트 콜백 타입
pub type OnMessageCallback = Arc<dyn Fn(&Message) + Send + Sync>;

// 공유 프로바이더 타입
pub type SharedProvider = Arc<Mutex<Option<Arc<dyn Provider>>>>;

// 에이전트 이벤트 스트림
pub enum AgentEvent {
    Message(Message),
    McpNotification((String, ServerNotification)),
    ModelChange { model: String, mode: String },
    HistoryReplaced(Conversation),
}

// 세션 실행 모드
pub enum SessionExecutionMode {
    Interactive,
    Background,
    SubTask { parent_session: String },
}

// 퍼미션 판단 결과
pub struct PermissionCheckResult {
    pub approved: Vec<ToolRequest>,
    pub needs_approval: Vec<ToolRequest>,
    pub denied: Vec<ToolRequest>,
}

// 검사 액션
pub enum InspectionAction {
    Allow,
    Deny,
    RequireApproval(Option<String>),
}

// 태스크 상태
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

// 성공 검사
pub enum SuccessCheck {
    Shell { command: String },
}

// 레시피 파라미터 입력 타입
pub enum RecipeParameterInputType {
    String, Number, Boolean, Date, File, Select,
}
```

---

*이 문서는 Goose 소스코드를 직접 분석하여 작성됨. 모든 라인 번호와 타입 시그니처는 `/home/user/goose-upstream`의 실제 소스에 기반.*
