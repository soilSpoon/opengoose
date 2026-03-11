# Goose 심층 분석 — 소스코드 기반

> **분석 대상:** block/goose (commit `a28c306b2`)
> **분석일:** 2026-03-11
> **통합된 문서:** goose-architecture-analysis.md, opengoose-goose-alignment-audit.md

---

## 1. 서브에이전트 시스템

Goose의 서브에이전트 시스템은 메인 에이전트가 하위 작업을 독립적인 에이전트 인스턴스에 위임하는 구조다. 핵심 파일 세 개로 구성된다.

### 1.1 TaskConfig -- 서브에이전트 설정

**파일**: `crates/goose/src/agents/subagent_task_config.rs`

```rust
// 라인 9-19
pub const DEFAULT_SUBAGENT_MAX_TURNS: usize = 25;

#[derive(Clone)]
pub struct TaskConfig {
    pub provider: Arc<dyn Provider>,
    pub parent_session_id: String,
    pub parent_working_dir: PathBuf,
    pub extensions: Vec<ExtensionConfig>,
    pub max_turns: Option<usize>,
}
```

`TaskConfig`는 서브에이전트에 필요한 모든 의존성을 캡슐화한다. 주목할 점:

- `provider`는 `Arc<dyn Provider>` -- 부모와 동일한 LLM 프로바이더를 공유한다
- `max_turns`는 `GOOSE_SUBAGENT_MAX_TURNS` 환경변수로 전역 오버라이드 가능 (라인 47-49)
- `extensions`는 부모가 사용하는 확장을 선택적으로 전달할 수 있다

```rust
// 라인 34-51 -- 생성자에서 글로벌 설정 참조
impl TaskConfig {
    pub fn new(
        provider: Arc<dyn Provider>,
        parent_session_id: &str,
        parent_working_dir: &Path,
        extensions: Vec<ExtensionConfig>,
    ) -> Self {
        Self {
            provider,
            parent_session_id: parent_session_id.to_owned(),
            parent_working_dir: parent_working_dir.to_owned(),
            extensions,
            max_turns: Some(
                Config::global()
                    .get_param::<usize>("GOOSE_SUBAGENT_MAX_TURNS")
                    .unwrap_or(DEFAULT_SUBAGENT_MAX_TURNS),
            ),
        }
    }
}
```

### 1.2 SubagentRunParams와 실행 플로우

**파일**: `crates/goose/src/agents/subagent_handler.rs`

```rust
// 라인 23-46
pub type OnMessageCallback = Arc<dyn Fn(&Message) + Send + Sync>;

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

`SubagentRunParams`는 서브에이전트 실행에 필요한 전체 파라미터를 하나의 구조체로 집약한다. 핵심 필드:

- **`cancellation_token`**: `tokio_util::sync::CancellationToken` -- 부모가 서브에이전트를 취소할 수 있는 메커니즘
- **`on_message`**: 서브에이전트가 메시지를 생성할 때마다 호출되는 콜백 (부모에게 진행 상황 전달)
- **`notification_tx`**: MCP 알림을 부모에게 전달하는 unbounded 채널
- **`return_last_only`**: true이면 마지막 응답 텍스트만, false이면 전체 대화 텍스트를 반환

### 1.3 서브에이전트 실행 핵심 로직

`get_agent_messages()` (라인 122-226)가 실제 실행 엔진이다. 동작 순서:

```
1. Recipe에서 system_instructions와 user_task 추출
2. Agent::with_config(config)로 새 Agent 인스턴스 생성
3. task_config.provider로 프로바이더 설정
4. task_config.extensions를 순회하며 add_extension() 호출
5. recipe.response가 있으면 apply_recipe_components()로 구조화된 출력 설정
6. 서브에이전트 전용 시스템 프롬프트 빌드 및 오버라이드
7. agent.reply()로 스트림 시작 (CancellationToken 전달)
8. 스트림을 소비하며 on_message 콜백과 notification_tx로 이벤트 전파
```

특히 라인 175-180에서 `SessionConfig`를 구성할 때:

```rust
let session_config = SessionConfig {
    id: session_id.clone(),
    schedule_id: None,
    max_turns: task_config.max_turns.map(|v| v as u32),
    retry_config: recipe.retry,
};
```

`retry_config`가 레시피에서 직접 전달되므로, 서브에이전트도 자체적인 재시도 로직을 가질 수 있다.

### 1.4 취소 메커니즘

라인 183-189에서 `CancellationToken`이 `agent.reply()`에 전달된다:

```rust
let mut stream =
    crate::session_context::with_session_id(Some(session_id.to_string()), async {
        agent
            .reply(user_message, session_config, cancellation_token)
            .await
    })
    .await?;
```

부모 에이전트가 토큰을 취소하면, 서브에이전트의 reply 스트림이 조기 종료된다. 이는 협력적 취소(cooperative cancellation) 패턴이다.

### 1.5 이벤트 전파 (MCP 알림)

라인 267-293의 `create_tool_notification()`은 서브에이전트의 도구 호출을 MCP `LoggingMessageNotification`으로 변환한다:

```rust
// 라인 120
pub const SUBAGENT_TOOL_REQUEST_TYPE: &str = "subagent_tool_request";

// 라인 274-289
Some(ServerNotification::LoggingMessageNotification(
    Notification::new(
        LoggingMessageNotificationParam::new(
            LoggingLevel::Info,
            serde_json::json!({
                "type": SUBAGENT_TOOL_REQUEST_TYPE,
                "subagent_id": subagent_id,
                "tool_call": {
                    "name": tool_call.name,
                    "arguments": tool_call.arguments
                }
            }),
        )
        .with_logger(format!("subagent:{}", subagent_id)),
    ),
))
```

### 1.6 SessionType::SubAgent

**파일**: `crates/goose/src/session/session_manager.rs` (라인 27-35)

```rust
pub enum SessionType {
    #[default]
    User,
    Scheduled,
    SubAgent,
    Hidden,
    Terminal,
    Gateway,
}
```

서브에이전트 세션은 `SessionType::SubAgent`로 마킹되어, UI에서 메인 세션과 구분된다.

### 1.7 알림 이벤트 시스템

**파일**: `crates/goose/src/agents/subagent_execution_tool/notification_events.rs`

다중 서브에이전트 병렬 실행 시 진행 상황을 추적하는 구조:

```rust
// 라인 5-10
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

// 라인 24-38
pub enum TaskExecutionNotificationEvent {
    LineOutput { task_id: String, output: String },
    TasksUpdate {
        stats: TaskExecutionStats,
        tasks: Vec<TaskInfo>,
    },
    TasksComplete {
        stats: TaskCompletionStats,
        failed_tasks: Vec<FailedTaskInfo>,
    },
}
```

`TaskExecutionStats`는 total/pending/running/completed/failed 카운터를 가지며, `TaskCompletionStats`는 success_rate (f64)를 포함한다.

`TaskInfo` (라인 57-68)는 각 서브태스크의 상태를 실시간 추적:

```rust
pub struct TaskInfo {
    pub id: String,
    pub status: TaskStatus,
    pub duration_secs: Option<f64>,
    pub current_output: String,
    pub task_type: String,
    pub task_name: String,
    pub task_metadata: String,
    pub error: Option<String>,
    pub result_data: Option<Value>,
}
```

### 1.8 OpenGoose 매핑

OpenGoose의 `FanOutExecutor` (`crates/opengoose-teams/src/fan_out_executor.rs`)는 유사한 패턴을 사용한다:

```rust
// fan_out_executor.rs 라인 51
let mut join_set = JoinSet::new();

// 라인 78 -- 각 에이전트를 spawn
join_set.spawn(async move {
    let runner = AgentRunner::from_profile_keyed(&profile, session_id).await?;
    // ...
});
```

**핵심 차이점**:

| 측면 | Goose | OpenGoose |
|------|-------|-----------|
| 병렬화 | 단일 서브에이전트를 순차적으로 실행, 다중은 Summon 확장이 관리 | `JoinSet`으로 모든 에이전트 동시 spawn |
| 취소 | `CancellationToken` (tokio_util) | JoinSet의 `abort_all()` |
| 이벤트 전파 | MCP `LoggingMessageNotification` | `[BROADCAST]:` 접두사 기반 텍스트 파싱 |
| 세션 관리 | `SessionType::SubAgent`로 별도 세션 생성 | 결정적 session_id (`{session_key}::{profile}`) 재사용 |

**OpenGoose가 재사용할 수 있는 것**:
- `TaskExecutionNotificationEvent` 구조체의 패턴 (진행 상황 추적 UI용)
- `CancellationToken` 기반 협력적 취소 패턴 (현재 OpenGoose는 JoinSet abort에 의존)
- `SubagentRunParams`처럼 실행 파라미터를 단일 구조체로 캡슐화하는 접근

**독립적으로 구축해야 하는 것**:
- OpenGoose의 `MergeStrategy` (fan-out 결과 병합)는 Goose에 없다
- 팀 기반 오케스트레이션 (Goose는 단일 에이전트 위임 모델)

---

## 2. 퍼미션 시스템

Goose의 퍼미션 시스템은 도구 호출 시 보안 검사를 수행하는 다층 구조다.

### 2.1 GooseMode -- 전역 동작 모드

**파일**: `crates/goose/src/config/goose_mode.rs`

```rust
// 라인 4-24
pub enum GooseMode {
    Auto,          // 모든 도구 자동 허용
    Approve,       // 모든 도구에 대해 사용자 승인 필요
    SmartApprove,  // 읽기 전용 도구는 자동, 쓰기 도구는 승인
    Chat,          // 도구 사용 없이 대화만
}
```

네 가지 모드가 퍼미션 시스템의 기본 행동을 결정한다.

### 2.2 PermissionManager -- 영구 설정 관리

**파일**: `crates/goose/src/config/permission.rs`

```rust
// 라인 17-23
pub enum PermissionLevel {
    AlwaysAllow, // 항상 허용
    AskBefore,   // 사전 승인 필요
    NeverAllow,  // 절대 불허
}

// 라인 26-31
pub struct PermissionConfig {
    pub always_allow: Vec<String>,
    pub ask_before: Vec<String>,
    pub never_allow: Vec<String>,
}

// 라인 34-38
pub struct PermissionManager {
    config_path: PathBuf,
    permission_map: RwLock<HashMap<String, PermissionConfig>>,
}
```

`PermissionManager`는 두 가지 카테고리의 권한을 관리한다:
- **`user`** -- 사용자가 명시적으로 설정한 도구별 권한
- **`smart_approve`** -- LLM 기반 자동 판별 결과의 캐시

설정은 `permission.yaml` 파일에 영구 저장된다 (라인 141-143에서 YAML 직렬화 후 파일 기록).

### 2.3 PermissionManager의 어노테이션 기반 설정

라인 101-117에서 MCP 도구의 `ToolAnnotations`를 활용한 자동 권한 설정:

```rust
pub fn apply_tool_annotations(&self, tools: &[Tool]) {
    let mut write_annotated = Vec::new();
    for tool in tools {
        let Some(anns) = &tool.annotations else { continue; };
        if anns.read_only_hint == Some(false) {
            write_annotated.push(tool.name.to_string());
        }
    }
    if !write_annotated.is_empty() {
        self.bulk_update_smart_approve_permissions(
            &write_annotated,
            PermissionLevel::AskBefore,
        );
    }
}
```

`read_only_hint == false`인 도구(즉, 쓰기 가능 도구)를 자동으로 `AskBefore`로 설정한다.

### 2.4 PermissionConfirmation -- 사용자 응답 처리

**파일**: `crates/goose/src/permission/permission_confirmation.rs`

```rust
// 라인 6-12
pub enum Permission {
    AlwaysAllow,  // 이 도구를 항상 허용 (영구)
    AllowOnce,    // 이번 한 번만 허용
    Cancel,       // 작업 취소
    DenyOnce,     // 이번 한 번만 거부
    AlwaysDeny,   // 이 도구를 항상 거부 (영구)
}

// 라인 20-24
pub struct PermissionConfirmation {
    pub principal_type: PrincipalType,  // Extension 또는 Tool
    pub permission: Permission,
}
```

`AllowOnce`/`DenyOnce`는 일회성이고, `AlwaysAllow`/`AlwaysDeny`는 `PermissionManager`에 영구 저장된다.

### 2.5 PermissionInspector -- 검사 엔진

**파일**: `crates/goose/src/permission/permission_inspector.rs`

`PermissionInspector`는 `ToolInspector` 트레잇을 구현하며, 도구 호출에 대한 권한 판단을 수행한다.

```rust
// 라인 15-19
pub struct PermissionInspector {
    pub permission_manager: Arc<PermissionManager>,
    provider: SharedProvider,
    readonly_tools: RwLock<HashSet<String>>,
}
```

`inspect()` 메서드 (라인 124-261)의 판단 플로우:

```
GooseMode별 분기:
  Chat     -> skip (도구 사용 안 함)
  Auto     -> 무조건 Allow
  Approve / SmartApprove ->
    1단계: user 권한 확인 (AlwaysAllow/NeverAllow/AskBefore)
    2단계: read_only 어노테이션 확인 -> Allow
    3단계: SmartApprove 캐시 확인 -> AlwaysAllow면 Allow
    4단계: 확장 관리 도구면 -> RequireApproval (보안)
    5단계: SmartApprove인데 캐시 미스 -> LLM 판별 대상에 추가
    6단계: 기본값 -> RequireApproval
```

**LLM 기반 읽기 전용 판별** (라인 213-257):

SmartApprove 모드에서 캐시에 없는 도구는 별도의 LLM 호출로 읽기 전용 여부를 판별한다:

```rust
let detected: HashSet<String> = match self.provider.lock().await.clone() {
    Some(provider) => {
        detect_read_only_tools(provider, session_id, llm_detect_candidates.to_vec())
            .await
            .into_iter()
            .collect()
    }
    None => Default::default(),
};
```

판별 결과는 `smart_approve` 카테고리에 캐시되어 이후 동일 도구에 대해 LLM 호출을 건너뛴다.

### 2.6 PermissionJudge -- LLM 기반 판별

**파일**: `crates/goose/src/permission/permission_judge.rs`

`detect_read_only_tools()` (라인 122-154)는 별도의 LLM 호출로 도구의 읽기 전용 여부를 판단한다:

```rust
// 라인 19-65 -- 판별용 도구 정의
fn create_read_only_tool() -> Tool {
    Tool::new(
        "platform__tool_by_tool_permission".to_string(),
        // ... 읽기 전용 판단 가이드라인 ...
        object!({
            "type": "object",
            "properties": {
                "read_only_tools": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "..."
                }
            },
            "required": []
        })
    )
}
```

LLM에게 도구 목록을 주고, 읽기 전용 도구 이름 배열을 반환받는 tool-use 패턴이다.

### 2.7 ToolPermissionStore -- 컨텍스트별 권한 캐시

**파일**: `crates/goose/src/permission/permission_store.rs`

```rust
// 라인 11-20
pub struct ToolPermissionRecord {
    tool_name: String,
    allowed: bool,
    context_hash: String,   // blake3 해시 (도구 인자 기반)
    readable_context: Option<String>,
    timestamp: i64,
    expiry: Option<i64>,    // 만료 타임스탬프
}

// 라인 22-28
pub struct ToolPermissionStore {
    permissions: HashMap<String, Vec<ToolPermissionRecord>>,
    version: u32,
    permissions_dir: PathBuf,
}
```

도구+인자 조합의 blake3 해시를 키로, 이전 권한 판단 결과를 캐시한다. 만료 시간이 지나면 자동 정리된다 (라인 129-143의 `cleanup_expired()`). 저장은 원자적 파일 교체(temp + rename) 패턴을 사용한다 (라인 70-76).

### 2.8 PermissionCheckResult

```rust
// permission_judge.rs 라인 157-162
pub struct PermissionCheckResult {
    pub approved: Vec<ToolRequest>,
    pub needs_approval: Vec<ToolRequest>,
    pub denied: Vec<ToolRequest>,
}
```

모든 검사 결과가 세 카테고리로 분류되어 에이전트 루프에 반환된다.

### 2.9 에이전트 보호(babysitting)에 대한 시사점

Goose의 퍼미션 시스템은 **도구 수준(tool-level)** 세분화를 제공한다. OpenGoose가 에이전트 보호를 구현할 때 고려할 점:

- **SmartApprove**의 LLM 기반 판별은 비용/지연이 발생하지만, 결과를 캐시하므로 첫 호출에만 비용이 든다
- `ToolAnnotations` (MCP 표준)의 `read_only_hint`를 적극 활용하면 LLM 호출 없이 자동 분류 가능
- `ToolPermissionStore`의 인자별 해시 캐싱은 `ls` vs `rm -rf`를 구분할 수 있게 해준다
- 서브에이전트를 `GooseMode::Approve`로 실행하면 부모의 `on_message` 콜백에서 승인/거부를 결정하는 "감독 에이전트" 패턴이 가능하다

---

## 3. Recipe 시스템 내부

Recipe는 Goose의 재사용 가능한 에이전트 설정 + 작업 정의 단위다.

### 3.1 Recipe 구조체

**파일**: `crates/goose/src/recipe/mod.rs` (라인 42-86)

```rust
pub struct Recipe {
    pub version: String,           // 파일 포맷 버전 (sem ver)
    pub title: String,             // 짧은 제목
    pub description: String,       // 긴 설명

    pub instructions: Option<String>,  // 모델에 대한 시스템 지시
    pub prompt: Option<String>,        // 세션 시작 프롬프트

    pub extensions: Option<Vec<ExtensionConfig>>,
    pub settings: Option<Settings>,
    pub activities: Option<Vec<String>>,
    pub author: Option<Author>,
    pub parameters: Option<Vec<RecipeParameter>>,
    pub response: Option<Response>,      // 구조화된 출력 스키마
    pub sub_recipes: Option<Vec<SubRecipe>>,
    pub retry: Option<RetryConfig>,
}
```

### 3.2 SubRecipe -- 서브레시피 조합

```rust
// 라인 118-128
pub struct SubRecipe {
    pub name: String,
    pub path: String,
    pub values: Option<HashMap<String, String>>,
    pub sequential_when_repeated: bool,  // 핵심 플래그
    pub description: Option<String>,
}
```

`sequential_when_repeated`는 동일 서브레시피가 여러 번 호출될 때 병렬이 아닌 순차 실행을 강제한다. 이는 부작용이 있는 서브레시피(예: DB 마이그레이션)에서 중요하다.

서브레시피가 있으면 자동으로 `summon` 확장이 주입된다:

```rust
// 라인 254-270
fn ensure_summon_for_subrecipes(&mut self) {
    if self.sub_recipes.is_none() { return; }
    let summon = ExtensionConfig::Platform {
        name: "summon".to_string(),
        // ...
    };
    match &mut self.extensions {
        Some(exts) if !exts.iter().any(|e| e.name() == "summon") => exts.push(summon),
        None => self.extensions = Some(vec![summon]),
        _ => {}
    }
}
```

### 3.3 RetryConfig -- 재시도 설정

**파일**: `crates/goose/src/agents/types.rs` (라인 22-74)

```rust
pub struct RetryConfig {
    pub max_retries: u32,
    pub checks: Vec<SuccessCheck>,
    pub on_failure: Option<String>,      // 실패 시 정리 셸 명령
    pub timeout_seconds: Option<u64>,    // 기본 300초
    pub on_failure_timeout_seconds: Option<u64>,  // 기본 600초
}

pub enum SuccessCheck {
    Shell { command: String },  // 셸 명령의 종료 코드로 성공 판단
}
```

YAML 예시:

```yaml
retry:
  max_retries: 3
  checks:
    - type: shell
      command: "cargo test --no-fail-fast 2>&1 | tail -1 | grep -q 'test result: ok'"
  on_failure: "git checkout -- ."
  timeout_seconds: 120
```

`SuccessCheck::Shell`은 셸 명령의 종료 코드(exit status)로 레시피 실행 성공을 판단한다. 에이전트가 작업을 마친 후 자동으로 검증 명령을 실행하고, 실패하면 `on_failure` 정리 명령 후 재시도한다.

### 3.4 Response -- 구조화된 출력

```rust
// 라인 112-116
pub struct Response {
    pub json_schema: Option<serde_json::Value>,
}
```

`response.json_schema`가 설정되면, 에이전트의 최종 출력이 해당 JSON Schema에 맞춰 구조화된다. `subagent_handler.rs`의 라인 158-161에서:

```rust
let has_response_schema = recipe.response.is_some();
agent
    .apply_recipe_components(recipe.response.clone(), true)
    .await;
```

### 3.5 RecipeParameter -- 매개변수화

```rust
// 라인 155-161
pub enum RecipeParameterRequirement {
    Required,
    Optional,
    UserPrompt,  // UI에서 사용자에게 직접 입력 요청
}

// 라인 173-185
pub enum RecipeParameterInputType {
    String, Number, Boolean, Date,
    File,    // 파일 경로에서 내용 임포트 (보안상 기본값 불가)
    Select,  // 선택지 목록에서 고르기
}
```

`File` 타입 파라미터는 보안상 기본값을 가질 수 없다 (validate_recipe.rs 라인 146-153에서 검증):

```rust
let file_params_with_defaults: Vec<String> = params
    .iter()
    .filter(|p| matches!(p.input_type, RecipeParameterInputType::File) && p.default.is_some())
    .map(|p| p.key.clone())
    .collect();

if !file_params_with_defaults.is_empty() {
    return Err(anyhow::anyhow!(
        "File parameters cannot have default values to avoid importing sensitive user files: {}",
        file_params_with_defaults.join(", ")
    ));
}
```

### 3.6 레시피 검증 파이프라인

**파일**: `crates/goose/src/recipe/validate_recipe.rs`

`validate_recipe_template_from_content()` (라인 39-55):

```
1. parse_and_validate_parameters() -- 템플릿 변수와 파라미터 정의 매칭 검증
2. validate_prompt_or_instructions() -- instructions 또는 prompt 중 최소 하나 필수
3. validate_retry_config() -- retry 설정값 유효성 검사
4. validate_json_schema() -- response.json_schema의 JSON Schema 유효성 검증
```

### 3.7 확장 직렬화 어댑터

**파일**: `crates/goose/src/recipe/recipe_extension_adapter.rs`

레시피 YAML에서 확장을 역직렬화할 때, 내부용 `RecipeExtensionConfigInternal` enum을 거쳐 `ExtensionConfig`로 변환한다. 이는 `description`의 기본값 처리 등을 위한 중간 계층이다:

```rust
// 라인 113-160 -- macro를 통한 자동 변환
macro_rules! map_recipe_extensions {
    ($value:expr; $( $variant:ident { $( $field:ident ),* } ),+) => {{
        match $value {
            $(
                RecipeExtensionConfigInternal::$variant {
                    name, description, $( $field ),*
                } => ExtensionConfig::$variant {
                    name,
                    description: description.unwrap_or_default(),
                    $( $field ),*
                },
            )+
        }
    }};
}
```

지원하는 확장 타입: `Stdio`, `Builtin`, `Platform`, `StreamableHttp`, `Frontend`, `InlinePython`.

### 3.8 보안 검사

```rust
// mod.rs 라인 273-289
pub fn check_for_security_warnings(&self) -> bool {
    if [self.instructions.as_deref(), self.prompt.as_deref()]
        .iter()
        .flatten()
        .any(|&field| contains_unicode_tags(field))
    {
        return true;
    }
    // ...
}
```

유니코드 태그 문자(U+E0041 등)를 이용한 인젝션 공격을 탐지한다.

### 3.9 OpenGoose의 recipe_bridge.rs와 비교

**파일**: `opengoose/crates/opengoose-teams/src/recipe_bridge.rs`

OpenGoose는 `AgentProfile`과 Goose `Recipe` 간의 양방향 변환을 구현한다:

```rust
// recipe_bridge.rs 라인 41 -- profile -> recipe 변환
pub fn profile_to_recipe(profile: &AgentProfile) -> Recipe { ... }
```

```rust
// recipe_bridge.rs 라인 22-38 -- retry config 변환
pub fn settings_to_retry_config(settings: &ProfileSettings) -> Option<RetryConfig> {
    let max_retries = settings.max_retries?;
    let checks = settings
        .retry_checks
        .iter()
        .map(|cmd| SuccessCheck::Shell { command: cmd.clone() })
        .collect();
    Some(RetryConfig { max_retries, checks, on_failure: settings.on_failure.clone(), ... })
}
```

**아직 미구현인 부분**:
- `sequential_when_repeated` -- OpenGoose의 `SubRecipeRef`에 해당 필드 없음
- `RecipeParameterInputType::File` -- 파일 임포트 파라미터
- `RecipeParameterRequirement::UserPrompt` -- UI 프롬프트 연동
- 보안 검사 (`contains_unicode_tags`)

---

## 4. Extension 디스패치 플로우

### 4.1 ExtensionManager 구조

**파일**: `crates/goose/src/agents/extension_manager.rs` (라인 107-115)

```rust
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

내부 `Extension` 구조체 (라인 61-100):

```rust
struct Extension {
    pub config: ExtensionConfig,
    client: McpClientBox,           // Arc<dyn McpClientTrait>
    server_info: Option<ServerInfo>,
    _temp_dir: Option<tempfile::TempDir>,
}
```

### 4.2 도구 수집 플로우 (Tool Aggregation)

`fetch_all_tools()` (라인 945-1040):

```
1. extensions 잠금 -> (name, config, client) 튜플 목록 추출
2. 각 확장에 대해 비동기 future 생성:
   a. client.list_tools()로 도구 목록 조회
   b. pagination 처리 (next_cursor)
   c. available_tools 필터링 (config.is_tool_available())
   d. unprefixed 여부에 따라 이름 결정:
      - unprefixed: 원래 이름 유지 (예: "shell")
      - prefixed: "확장명__도구명" (예: "developer__shell")
   e. 메타데이터에 goose_extension 키 추가
3. future::join_all()로 모든 확장 동시 조회
4. 중복 도구 이름 건너뛰기 (경고 출력)
```

핵심 코드 (라인 974-996):

```rust
let expose_unprefixed = is_unprefixed_extension(&config);

for mut tool in client_tools.tools {
    if config.is_tool_available(&tool.name) {
        let public_name = if expose_unprefixed {
            tool.name.to_string()
        } else {
            format!("{}__{}", name, tool.name)
        };

        let mut meta_map = tool.meta.as_ref()
            .map(|m| m.0.clone())
            .unwrap_or_default();
        meta_map.insert(
            TOOL_EXTENSION_META_KEY.to_string(),
            serde_json::Value::String(name.clone()),
        );

        tool.name = public_name.into();
        tool.meta = Some(rmcp::model::Meta(meta_map));
        tools.push(tool);
    }
}
```

### 4.3 도구 캐싱

```rust
// 라인 918-938
async fn get_all_tools_cached(&self, session_id: &str) -> ExtensionResult<Arc<Vec<Tool>>> {
    {
        let cache = self.tools_cache.lock().await;
        if let Some(ref tools) = *cache {
            return Ok(Arc::clone(tools));
        }
    }

    let version_before = self.tools_cache_version.load(Ordering::SeqCst);
    let tools = Arc::new(self.fetch_all_tools(session_id).await?);

    {
        let mut cache = self.tools_cache.lock().await;
        let version_after = self.tools_cache_version.load(Ordering::SeqCst);
        if version_after == version_before && cache.is_none() {
            *cache = Some(Arc::clone(&tools));
        }
    }

    Ok(tools)
}
```

버전 카운터 기반 낙관적 캐싱 -- 캐시 미스 동안 다른 스레드가 확장을 추가/제거하면 캐시를 갱신하지 않는다.

### 4.4 도구 호출 디스패치

`resolve_tool()` (라인 1304-1362):

```
1. 도구 이름에 "__"가 있으면 prefix로 확장 찾기
2. 없으면 캐시된 도구 목록에서 이름으로 검색
3. 메타데이터의 goose_extension으로 소유 확장 확인
4. 확장의 MCP 클라이언트 반환
```

`dispatch_tool_call()` (라인 1364-1426):

```rust
pub async fn dispatch_tool_call(
    &self,
    session_id: &str,
    tool_call: CallToolRequestParams,
    working_dir: Option<&std::path::Path>,
    cancellation_token: CancellationToken,
) -> Result<ToolCallResult> {
    let resolved = self.resolve_tool(session_id, &tool_name_str).await?;

    // available_tools 검사
    if !extension.config.is_tool_available(&resolved.actual_tool_name) {
        return Err(...);
    }

    // 실제 MCP 호출
    let fut = async move {
        client.call_tool(
            &session_id,
            &actual_tool_name,
            arguments,
            working_dir_str.as_deref(),
            cancellation_token,
        ).await
    };

    Ok(ToolCallResult {
        result: Box::new(fut.boxed()),
        notification_stream: Some(Box::new(ReceiverStream::new(notifications_receiver))),
    })
}
```

`ToolCallResult`는 결과 future와 알림 스트림을 함께 반환하여, 호출자가 실행 중 알림을 수신할 수 있게 한다.

### 4.5 플랫폼 확장 (내장 확장)

**파일**: `crates/goose/src/agents/platform_extensions/mod.rs`

```rust
// 라인 207-215
pub struct PlatformExtensionDef {
    pub name: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    pub default_enabled: bool,
    pub unprefixed_tools: bool,     // 도구 접두사 없이 노출
    pub client_factory: fn(PlatformExtensionContext) -> Box<dyn McpClientTrait>,
}
```

내장 플랫폼 확장 목록:

| 이름 | unprefixed | 기본 활성 | 설명 |
|------|-----------|-----------|------|
| `analyze` | O | O | Tree-sitter 기반 코드 분석 |
| `todo` | X | O | 에이전트의 할 일 목록 |
| `apps` | X | O | HTML/CSS/JS 앱 생성/관리 |
| `chatrecall` | X | X | 과거 대화 검색 |
| `extensionmanager` | X | O | 확장 관리 |
| `summon` | O | O | 서브에이전트 위임 |
| `summarize` | X | X | 파일/디렉터리 LLM 요약 |
| `developer` | O | O | 파일 편집 + 셸 실행 |
| `tom` | X | O | 커스텀 컨텍스트 주입 |
| `code_execution` | O | X | 코드 실행 모드 (feature flag) |

`unprefixed_tools = true`인 확장의 도구는 `shell`, `read_file` 등 직관적인 이름으로 노출된다. `false`이면 `todo__add_task` 같은 접두사 형식이 된다.

### 4.6 Frontend vs Backend 도구 분류

`ExtensionConfig::Frontend`는 도구 호출이 백엔드가 아닌 프론트엔드(UI)에서 처리되는 도구를 정의한다:

```rust
// recipe_extension_adapter.rs 라인 69-80
Frontend {
    name: String,
    description: Option<String>,
    tools: Vec<Tool>,
    instructions: Option<String>,
    bundled: Option<bool>,
    available_tools: Vec<String>,
}
```

Frontend 확장의 도구는 에이전트가 호출하면 UI 레이어로 전달되어 처리되며, 결과가 다시 에이전트로 반환된다.

### 4.7 보안/퍼미션/반복 검사 파이프라인

도구 호출 전 다중 인스펙터가 순차적으로 검사한다:

1. **PermissionInspector** -- 퍼미션 시스템 (섹션 2 참조)
2. **SecurityInspector** -- 보안 위협 감지
3. **RepetitionInspector** -- 동일 도구 반복 호출 감지

`PermissionInspector.process_inspection_results()` (라인 53-111)가 모든 인스펙터의 결과를 통합한다:
- permission 인스펙터의 결과가 기본(baseline)
- 다른 인스펙터의 결과가 오버라이드 (보안 거부는 퍼미션 허용을 뒤집을 수 있음)

---

## 5. 서버 모드 (goose-server)

### 5.1 AppState -- 전역 상태

**파일**: `crates/goose-server/src/state.rs`

```rust
// 라인 21-29
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

`AgentManager`가 핵심이며, 세션별 에이전트를 관리한다:

```rust
// 라인 110-112
pub async fn get_agent(&self, session_id: String) -> anyhow::Result<Arc<goose::agents::Agent>> {
    self.agent_manager.get_or_create_agent(session_id).await
}
```

`ExtensionLoadingTasks` (라인 17-18)는 확장 로딩을 백그라운드에서 수행하고, 나중에 결과를 가져오는 패턴:

```rust
type ExtensionLoadingTasks =
    Arc<Mutex<HashMap<String, Arc<Mutex<Option<JoinHandle<Vec<ExtensionLoadResult>>>>>>>>;
```

### 5.2 SSE 스트리밍 패턴 -- reply 엔드포인트

**파일**: `crates/goose-server/src/routes/reply.rs`

```rust
// 라인 79-90
pub struct ChatRequest {
    user_message: Message,
    override_conversation: Option<Vec<Message>>,
    session_id: String,
    recipe_name: Option<String>,
    recipe_version: Option<String>,
}
```

SSE 응답 구현 (라인 92-124):

```rust
pub struct SseResponse {
    rx: ReceiverStream<String>,
}

impl Stream for SseResponse {
    type Item = Result<Bytes, Infallible>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.rx).poll_next(cx)
            .map(|opt| opt.map(|s| Ok(Bytes::from(s))))
    }
}

impl IntoResponse for SseResponse {
    fn into_response(self) -> axum::response::Response {
        http::Response::builder()
            .header("Content-Type", "text/event-stream")
            .header("Cache-Control", "no-cache")
            .header("Connection", "keep-alive")
            .body(axum::body::Body::from_stream(self))
            .unwrap()
    }
}
```

### 5.3 MessageEvent -- 이벤트 타입

```rust
// 라인 126-153
pub enum MessageEvent {
    Message { message: Message, token_state: TokenState },
    Error { error: String },
    Finish { reason: String, token_state: TokenState },
    ModelChange { model: String, mode: String },
    Notification { request_id: String, message: ServerNotification },
    UpdateConversation { conversation: Conversation },
    Ping,
}
```

### 5.4 reply() 핸들러의 이벤트 루프

라인 335-395의 핵심 루프:

```rust
loop {
    tokio::select! {
        _ = task_cancel.cancelled() => {
            tracing::info!("Agent task cancelled");
            break;
        }
        _ = heartbeat_interval.tick() => {
            stream_event(MessageEvent::Ping, &tx, &cancel_token).await;
        }
        response = timeout(Duration::from_millis(500), stream.next()) => {
            match response {
                Ok(Some(Ok(AgentEvent::Message(message)))) => { ... }
                Ok(Some(Ok(AgentEvent::HistoryReplaced(new_messages)))) => {
                    all_messages = new_messages.clone();
                    stream_event(MessageEvent::UpdateConversation { ... }).await;
                }
                Ok(Some(Ok(AgentEvent::ModelChange { model, mode }))) => { ... }
                Ok(Some(Ok(AgentEvent::McpNotification(...)))) => { ... }
                Ok(Some(Err(e))) => { ... break; }
                Ok(None) => { break; }
                Err(_) => { /* timeout, check client alive */ }
            }
        }
    }
}
```

핵심 패턴:
- `tokio::select!`로 취소/하트비트/응답 세 가지를 동시 대기
- 500ms 하트비트로 SSE 연결 유지 (Ping 이벤트)
- 500ms 타임아웃으로 스트림 폴링 -- 클라이언트 연결 해제 감지
- `HistoryReplaced` 이벤트가 오면 전체 대화 교체 (컴팩션 결과)
- 클라이언트가 끊으면(`tx.is_closed()`) `cancel_token.cancel()`로 에이전트에 취소 전파

### 5.5 세션 관리 API

**파일**: `crates/goose-server/src/routes/session.rs`

REST API 엔드포인트:

| 메서드 | 경로 | 기능 |
|--------|------|------|
| GET | `/sessions` | 세션 목록 |
| GET | `/sessions/{id}` | 세션 상세 |
| DELETE | `/sessions/{id}` | 세션 삭제 |
| PUT | `/sessions/{id}/name` | 이름 변경 (최대 200자) |
| PUT | `/sessions/{id}/user_recipe_values` | 레시피 파라미터 업데이트 |
| GET | `/sessions/{id}/export` | JSON 내보내기 |
| POST | `/sessions/import` | JSON 가져오기 (25MB 제한) |
| POST | `/sessions/{id}/fork` | 세션 분기/잘라내기 |
| GET | `/sessions/{id}/extensions` | 세션의 확장 목록 |
| GET | `/sessions/search` | 대화 검색 (키워드, 날짜 필터) |
| GET | `/sessions/insights` | 세션 통계 |

fork 엔드포인트 (라인 379-448)는 `truncate`와 `copy` 플래그를 조합하여:
- `copy=true, truncate=false` -- 세션 복사
- `copy=true, truncate=true` -- 특정 시점까지 복사 후 잘라내기
- `copy=false, truncate=true` -- 원본 세션을 잘라내기

### 5.6 opengoose-web과의 중복 분석

OpenGoose의 웹 서버와 Goose 서버의 구조적 유사성:

- 둘 다 Axum 프레임워크 사용
- SSE 기반 스트리밍 패턴 동일
- 세션 관리 CRUD 패턴 유사

**차이점**:
- Goose는 `AgentManager`가 세션별 에이전트를 on-demand 생성
- Goose는 `CancellationToken`으로 세밀한 취소 제어
- Goose의 `override_conversation` 기능 (클라이언트가 대화 히스토리를 덮어쓸 수 있음)
- 확장 로딩을 백그라운드 태스크로 분리 (`ExtensionLoadingTasks`)

---

## 6. 컨텍스트 관리

### 6.1 fix_conversation() 파이프라인

**파일**: `crates/goose/src/conversation/mod.rs`

`fix_conversation()` (라인 164-200)은 LLM에 전송하기 전 대화를 정규화하는 파이프라인이다.

핵심: agent_visible 메시지만 수정하고, non-visible 메시지는 원래 위치를 유지한다 (shadow map 패턴):

```rust
pub fn fix_conversation(conversation: Conversation) -> (Conversation, Vec<String>) {
    // Shadow map: 각 메시지를 Visible(index) 또는 NonVisible(Message)로 분류
    enum MessageSlot {
        Visible(usize),
        NonVisible(Message),
    }

    let shadow_map: Vec<MessageSlot> = all_messages.iter().map(|msg| {
        if msg.metadata.agent_visible {
            let idx = agent_visible_messages.len();
            agent_visible_messages.push(msg.clone());
            MessageSlot::Visible(idx)
        } else {
            MessageSlot::NonVisible(msg.clone())
        }
    }).collect();

    // visible 메시지만 수정
    let (fixed_visible, issues) = fix_messages(agent_visible_messages);

    // shadow map으로 원래 위치에 재배치
    let final_messages: Vec<Message> = shadow_map.into_iter().filter_map(|slot| match slot {
        MessageSlot::Visible(idx) => fixed_visible.get(idx).cloned(),
        MessageSlot::NonVisible(msg) => Some(msg),
    }).collect();
}
```

`fix_messages()` (라인 202-221)는 7단계 프로세서 체인:

```rust
fn fix_messages(messages: Vec<Message>) -> (Vec<Message>, Vec<String>) {
    [
        merge_text_content_items,       // 1. 연속 텍스트 콘텐츠 병합
        trim_assistant_text_whitespace,  // 2. 어시스턴트 메시지 끝 공백 제거
        remove_empty_messages,           // 3. 빈 메시지 제거
        fix_tool_calling,               // 4. 도구 호출 정합성 수정
        merge_consecutive_messages,      // 5. 같은 역할 연속 메시지 병합
        fix_lead_trail,                 // 6. 첫/끝 메시지가 user인지 확인
        populate_if_empty,              // 7. 빈 대화면 "Hello" 추가
    ]
    .into_iter()
    .fold((messages, Vec::new()), |(msgs, mut all_issues), processor| {
        let (new_msgs, issues) = processor(msgs);
        all_issues.extend(issues);
        (new_msgs, all_issues)
    })
}
```

### 6.2 fix_tool_calling 상세

라인 307-399의 도구 호출 수정 로직:

```
1단계 (순방향 스캔):
  - user 메시지에서 ToolRequest 발견 -> 제거 (도구 요청은 assistant만 가능)
  - user 메시지에서 Thinking/RedactedThinking 발견 -> 제거
  - assistant 메시지에서 ToolResponse 발견 -> 제거 (도구 응답은 user만 가능)
  - ToolRequest가 있으면 pending_tool_requests에 추가
  - ToolResponse가 있으면 매칭되는 request가 있는지 확인, 없으면 제거 (고아 응답)

2단계 (역방향 스캔):
  - pending_tool_requests에 남은 것은 응답이 없는 고아 요청 -> 제거
```

### 6.3 자동 컴팩션 (80% 임계값)

**파일**: `crates/goose/src/context_mgmt/mod.rs`

```rust
// 라인 19
pub const DEFAULT_COMPACTION_THRESHOLD: f64 = 0.8;
```

`check_if_compaction_needed()` (라인 182-223):

```rust
pub async fn check_if_compaction_needed(
    provider: &dyn Provider,
    conversation: &Conversation,
    threshold_override: Option<f64>,
    session: &crate::session::Session,
) -> Result<bool> {
    let threshold = threshold_override.unwrap_or_else(|| {
        config.get_param::<f64>("GOOSE_AUTO_COMPACT_THRESHOLD")
            .unwrap_or(DEFAULT_COMPACTION_THRESHOLD)
    });

    let context_limit = provider.get_model_config().context_limit();

    // 세션 메타데이터의 토큰 수 우선, 없으면 직접 계산
    let (current_tokens, _) = match session.total_tokens {
        Some(tokens) => (tokens as usize, "session metadata"),
        None => {
            let token_counter = create_token_counter().await?;
            let counts: Vec<_> = messages.iter()
                .filter(|m| m.is_agent_visible())
                .map(|msg| token_counter.count_chat_tokens("", &[msg], &[]))
                .collect();
            (counts.iter().sum(), "estimated")
        }
    };

    let usage_ratio = current_tokens as f64 / context_limit as f64;

    // threshold가 0 이하 또는 1 이상이면 자동 컴팩션 비활성화
    let needs_compaction = if threshold <= 0.0 || threshold >= 1.0 {
        false
    } else {
        usage_ratio > threshold
    };
    Ok(needs_compaction)
}
```

### 6.4 컴팩션 실행

`compact_messages()` (라인 62-179):

```
1. 가장 최근 user 텍스트 메시지를 보존 대상으로 식별 (manual_compact이 아닌 경우)
2. do_compact()로 요약 생성 (progressive tool removal 포함)
3. 원본 메시지를 agent_invisible로 마킹 (user_visible은 유지)
4. 요약 메시지를 agent_only로 추가
5. 연속 텍스트 (continuation text) 추가 -- 컨텍스트에 따라 세 종류:
   - 일반 대화: "Just continue the conversation naturally"
   - 도구 루프 중: "Continue calling tools as necessary"
   - 수동 컴팩트: "compacted at the user's request"
6. 보존된 user 메시지를 마지막에 추가
```

### 6.5 Progressive Tool Removal

`do_compact()` (라인 275-340)는 컨텍스트 길이 초과 시 점진적으로 도구 응답을 제거한다:

```rust
let removal_percentages = [0, 10, 20, 50, 100];

for (attempt, &remove_percent) in removal_percentages.iter().enumerate() {
    let filtered_messages = filter_tool_responses(&agent_visible_messages, remove_percent);
    // ...
    match provider.complete_fast(...).await {
        Ok((response, usage)) => return Ok((response, usage)),
        Err(e) if matches!(e, ProviderError::ContextLengthExceeded(_)) => {
            if attempt < removal_percentages.len() - 1 { continue; }
            else { return Err(...); }
        }
        Err(e) => return Err(e.into()),
    }
}
```

`filter_tool_responses()` (라인 225-273)는 "middle-out" 전략으로 제거한다:

```rust
let middle = tool_indices.len() / 2;
for i in 0..num_to_remove {
    if i % 2 == 0 {
        // 중간에서 위쪽으로
        indices_to_remove.push(tool_indices[middle - offset - 1]);
    } else {
        // 중간에서 아래쪽으로
        indices_to_remove.push(tool_indices[middle + offset]);
    }
}
```

가장 오래된 것이나 최신 것이 아닌, **중간 부분**의 도구 응답부터 제거한다. 이는 대화 시작과 최근 컨텍스트를 최대한 보존하기 위한 전략이다.

### 6.6 HistoryReplaced 이벤트

컴팩션이 완료되면 `AgentEvent::HistoryReplaced(updated_conversation)` 이벤트가 스트림으로 전달된다.

`subagent_handler.rs` 라인 212-214:
```rust
Ok(AgentEvent::HistoryReplaced(updated_conversation)) => {
    conversation = updated_conversation;
}
```

`reply.rs` 라인 357-359:
```rust
Ok(Some(Ok(AgentEvent::HistoryReplaced(new_messages)))) => {
    all_messages = new_messages.clone();
    stream_event(MessageEvent::UpdateConversation { conversation: new_messages }, ...).await;
}
```

서버의 경우 `UpdateConversation` SSE 이벤트로 클라이언트에도 전파되어, UI가 대화 히스토리를 갱신할 수 있다.

### 6.7 도구 쌍 요약 (Tool Pair Summarization)

라인 510-536의 `maybe_summarize_tool_pair()`는 현재 **비활성화** 상태다:

```rust
const ENABLE_TOOL_PAIR_SUMMARIZATION: bool = false;
// TODO: Re-enable once tool summarization stability issues are resolved.
```

비활성화 상태이지만, 구현은 완전하다: 오래된 도구 호출+응답 쌍을 LLM으로 요약하여, 원본은 `agent_invisible`로, 요약은 `agent_only`로 설정한다.

### 6.8 Conversation 구조체

**파일**: `crates/goose/src/conversation/mod.rs` (라인 11-12)

```rust
pub struct Conversation(Vec<Message>);
```

내부는 `Vec<Message>`이지만, `push()` 메서드 (라인 44-63)에서 스트리밍 텍스트 청크를 자동 병합한다:

```rust
pub fn push(&mut self, message: Message) {
    if let Some(last) = self.0.last_mut()
        .filter(|m| m.id.is_some() && m.id == message.id)
    {
        match (last.content.last_mut(), message.content.last()) {
            (Some(MessageContent::Text(ref mut last)), Some(MessageContent::Text(new)))
                if message.content.len() == 1 =>
            {
                last.text.push_str(&new.text);  // 같은 ID의 텍스트 청크 병합
            }
            (_, _) => {
                last.content.extend(message.content);
            }
        }
    } else {
        self.0.push(message);
    }
}
```

가시성 필터:

```rust
pub fn agent_visible_messages(&self) -> Vec<Message> {
    self.filtered_messages(|meta| meta.agent_visible)
}

pub fn user_visible_messages(&self) -> Vec<Message> {
    self.filtered_messages(|meta| meta.user_visible)
}
```

이 이중 가시성 시스템이 컴팩션의 핵심이다:
- 컴팩션된 원본: `user_visible=true, agent_visible=false` -- UI에는 보이지만 LLM에는 안 보임
- 요약: `user_visible=false, agent_visible=true` -- UI에는 안 보이지만 LLM에는 보임

### 6.9 OpenGoose에 대한 시사점

OpenGoose가 컨텍스트 관리를 구현할 때 참고할 핵심 패턴:

1. **이중 가시성 메타데이터**: `agent_visible`/`user_visible` 분리로 요약과 원본을 동시 유지
2. **Progressive tool removal**: 컨텍스트 초과 시 0% -> 10% -> 20% -> 50% -> 100% 단계적 도구 응답 제거
3. **Middle-out 전략**: 최신과 최초 컨텍스트 보존, 중간부터 제거
4. **fix_conversation() 파이프라인**: 7단계 정규화는 LLM 호출 전 필수 전처리
5. **Shadow map 패턴**: non-visible 메시지의 위치를 보존하면서 visible 메시지만 수정

---

## 부록: 파일 인덱스

| 파일 경로 | 핵심 내용 |
|-----------|----------|
| `crates/goose/src/agents/subagent_handler.rs` | SubagentRunParams, run_subagent_task(), 이벤트 전파 |
| `crates/goose/src/agents/subagent_task_config.rs` | TaskConfig 구조체, DEFAULT_SUBAGENT_MAX_TURNS |
| `crates/goose/src/agents/subagent_execution_tool/notification_events.rs` | TaskExecutionNotificationEvent, TaskStatus |
| `crates/goose/src/config/permission.rs` | PermissionManager, PermissionLevel, PermissionConfig |
| `crates/goose/src/config/goose_mode.rs` | GooseMode enum |
| `crates/goose/src/permission/permission_inspector.rs` | PermissionInspector, inspect() 판단 로직 |
| `crates/goose/src/permission/permission_judge.rs` | detect_read_only_tools(), LLM 기반 판별 |
| `crates/goose/src/permission/permission_confirmation.rs` | Permission enum (AlwaysAllow/AllowOnce 등) |
| `crates/goose/src/permission/permission_store.rs` | ToolPermissionStore, blake3 해시 기반 캐싱 |
| `crates/goose/src/recipe/mod.rs` | Recipe 구조체, SubRecipe, RecipeParameter |
| `crates/goose/src/recipe/validate_recipe.rs` | 레시피 검증 파이프라인 |
| `crates/goose/src/recipe/recipe_extension_adapter.rs` | RecipeExtensionConfigInternal 변환 |
| `crates/goose/src/agents/extension_manager.rs` | ExtensionManager, 도구 수집/캐싱/디스패치 |
| `crates/goose/src/agents/platform_extensions/mod.rs` | PlatformExtensionDef, 내장 확장 정의 |
| `crates/goose/src/agents/types.rs` | RetryConfig, SuccessCheck |
| `crates/goose-server/src/routes/reply.rs` | SSE 스트리밍, ChatRequest, MessageEvent |
| `crates/goose-server/src/routes/session.rs` | 세션 CRUD API |
| `crates/goose-server/src/state.rs` | AppState, AgentManager 래퍼 |
| `crates/goose/src/context_mgmt/mod.rs` | compact_messages(), check_if_compaction_needed() |
| `crates/goose/src/conversation/mod.rs` | fix_conversation(), Conversation 구조체 |
| `crates/goose/src/session/session_manager.rs` | SessionType enum |

---

## 부록 B: OpenGoose의 Goose 활용 현황

### B.1 잘 활용하고 있는 부분

| 영역 | 평가 | 설명 |
|------|:---:|------|
| **Agent/Provider** | ✅ | `Agent::new()` → `reply()` 흐름 정확히 사용 |
| **Session 관리** | ✅ | Goose SessionManager + OpenGoose 보조 DB 이중화 |
| **Recipe 호환** | ✅ | `TeamDefinition::to_recipe()` 양방향 변환 |
| **Gateway 아키텍처** | ✅ | Goose Gateway trait 정확히 구현 |
| **Extension 관리** | ✅ | Goose에 완전 위임, 자체 구현 없음 |

### B.2 개선 기회

**1. `@mention`/`[BROADCAST]` 텍스트 파싱 → MCP 도구 기반 전환**

현재 `parse_agent_output()`이 에이전트 응답에서 `@agent_name:` 패턴을 파싱하는데, 이는 LLM 출력의 비결정성에 취약하다.

```rust
// 현재: 텍스트 파싱
"@reviewer: please check this" → parse_mention() → delegation

// 권장: 전용 MCP 도구
delegate_to(agent="reviewer", message="please check this") → 구조화된 JSON
```

**2. AgentEvent 실시간 전파**

`run_with_events()`가 `AgentEventSummary`를 반환하지만 사후 요약이다. 실시간 EventBus 포워딩 구현 시:
- Witness 패턴의 에이전트 liveness 감지 가능
- Extension 알림을 팀 오케스트레이션에 전파
- 모델 전환/컨텍스트 압축을 대시보드에 즉시 표시

**3. PermissionManager/GooseMode 활용**

에이전트별 도구 권한 차등 적용:
- `reviewer` 프로필: 파일 수정 금지
- `developer` 프로필: 전체 접근 허용

### B.3 이미 준비된 인프라

| 인프라 | 위치 | Gas Town 대응 |
|--------|------|---------------|
| `EventBus::subscribe_reliable()` | opengoose-types | Witness 기반 |
| `WorkStatus::Cancelled` | opengoose-persistence | Polecat 상태 |
| `find_resume_point()` | work_items.rs | 세션 복구 |
| `RemoteAgent` + Heartbeat | opengoose-teams | 분산 헬스 |
| `MessageBus` + `AgentMessageStore` | opengoose-teams/persistence | 에이전트 통신 |

### B.4 결론

OpenGoose는 Goose의 핵심 기능을 잘 활용하면서, Goose에 없는 멀티 채널/팀 오케스트레이션 기능을 적절히 추가했다. **재구현은 거의 없다.** 가장 큰 개선 포인트는 텍스트 파싱 기반 에이전트 통신을 MCP 도구 기반으로 전환하는 것이다.
