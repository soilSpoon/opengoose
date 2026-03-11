# OpenGoose - Goose 활용 현황 분석

## 요약

OpenGoose는 전반적으로 Goose를 **잘 활용**하고 있다. 핵심 에이전트 루프(`Agent::reply()`), Provider 시스템, Extension/MCP 도구, Session 관리 모두 Goose의 네이티브 API를 직접 사용하며, 불필요한 재구현은 거의 없다. 다만 몇 가지 개선 가능 지점이 존재한다.

---

## 1. 잘 활용하고 있는 부분 (Good)

### 1.1 Agent/Provider를 그대로 사용
- `AgentRunner`가 Goose의 `Agent::new()` → `update_provider()` → `add_extension()` → `reply()` 흐름을 정확히 따름
- Provider 생성에 `goose::providers::create_with_named_model()` 사용
- LLM 호출을 직접 구현하지 않고 Goose에 완전 위임

### 1.2 Session 관리를 Goose에 위임
- `SessionManager::create_session()`, `add_message()` 등 Goose의 SQLite 기반 세션 스토리지 사용
- `seed_history()`에서 `Message::user()`/`Message::assistant()` 네이티브 타입으로 이력 주입
- Extension 상태 저장/복원도 `persist_extension_state()` / `load_extensions_from_session()` 활용

### 1.3 Recipe 호환성 유지
- `TeamDefinition::to_recipe()`로 팀 정의를 Goose Recipe로 변환 가능
- `recipe_bridge` 모듈로 Profile ↔ Recipe 양방향 변환 지원
- `GOOSE_RECIPE_PATH` 환경변수에 프로필 디렉토리 등록하여 Summon Extension과 통합
- `goose run --recipe` CLI로도 실행 가능한 호환 경로 확보

### 1.4 Gateway/AgentManager 아키텍처 활용
- `goose::gateway::{Gateway, GatewayConfig, GatewayHandler}` 인터페이스를 정확히 구현
- `goose::execution::manager::AgentManager` 싱글턴을 공유
- `goose::gateway::pairing::PairingStore`로 페어링 관리

### 1.5 Extension 시스템 그대로 사용
- `goose::agents::extension::ExtensionConfig`의 Builtin/Stdio/Sse/Streamable 타입 활용
- Extension 관리를 Goose에 위임, 자체 도구 디스패치 구현 없음

---

## 2. 중복 구현 영역 (Overlap)

### 2.1 세션 영속화 이중화 (경미)
**현상**: `opengoose-persistence`의 `SessionStore`가 Goose의 `SessionManager`와 별도로 메시지를 저장한다.

```
Engine::record_user_message()  → opengoose-persistence의 SQLite
AgentRunner::seed_history()    → goose의 SessionManager SQLite
```

**분석**: OpenGoose는 채널(Discord/Telegram/Slack) 단위 세션 키, 팀 활성화 상태, 오케스트레이션 실행 이력 등 Goose에 없는 메타데이터를 관리해야 하므로 **별도 DB는 필수적**이다. 그러나 메시지 본문이 양쪽 DB에 중복 저장된다.

**판정**: 의도적 설계. OpenGoose DB는 채팅 이력 표시/검색용, Goose DB는 Agent의 대화 컨텍스트용. 다만 디스크 낭비가 커지면 OpenGoose 측은 요약만 저장하는 것을 검토할 수 있다.

### 2.3 MessageBus 및 AgentMessageStore

**현상**: 감사 이후 추가된 두 개의 에이전트 간 통신 인프라가 존재한다.

**MessageBus** (`opengoose-teams/src/message_bus.rs`):
- 인메모리 `tokio::sync::broadcast` 기반 실시간 메시징
- 두 가지 패턴: `send_directed()` (1:1) + `publish()` (pub/sub 채널)
- `subscribe_agent()`, `subscribe_channel()`, `subscribe_all()` 구독 옵션
- 프로세스 내부에서만 동작, 재시작 시 소멸

**AgentMessageStore** (`opengoose-persistence/src/agent_messages.rs`):
- SQLite 기반 영속 메시지 저장소 (MessageBus의 영속 보완)
- 3단계 상태: Pending → Delivered → Acknowledged
- `receive_pending()`, `channel_history(since_id)`, `mark_delivered()`, `acknowledge()` API
- 프로세스 재시작 후에도 미전달 메시지 복구 가능

**분석**: 텍스트 파싱 안티패턴(3.1)을 구조적으로 해결할 수 있는 인프라가 이미 갖추어져 있다. 그러나 에이전트가 이 인프라를 직접 사용하지 않는다 — 에이전트는 여전히 `@mention`/`[BROADCAST]` 텍스트를 출력하고, `parse_agent_output()`이 파싱한 결과를 MessageBus/AgentMessageStore에 전달하는 구조다. 에이전트가 MCP 도구를 통해 직접 MessageBus에 접근하는 것이 권장된다.

### 2.2 WorkItem 시스템 vs Goose Session
**현상**: `opengoose-persistence::WorkItemStore`가 팀 오케스트레이션의 작업 단위를 추적 (상태, 입출력, 부모-자식 관계).

**분석**: Goose의 `Session`은 단일 에이전트의 대화 단위이지, 다중 에이전트 오케스트레이션의 작업 추적 기능이 없다. **이것은 재구현이 아니라 Goose에 없는 기능의 추가**다.

**판정**: 적절. Goosetown의 "Beads" 이슈 트래커와 유사한 개념을 자체 구현한 것.

---

## 3. Goose 컨셉에 반하는 코드 (Anti-pattern)

### 3.1 `@mention`/`[BROADCAST]` 텍스트 파싱 기반 에이전트 통신 ⚠️
**현상**: `runner.rs`의 `parse_agent_output()`이 에이전트 응답 텍스트에서 `@agent_name: message`와 `[BROADCAST]: message` 패턴을 정규식 없이 파싱한다.

**문제점**:
- Goose의 도구(tool) 시스템이 구조화된 에이전트 간 통신을 위해 설계되었음
- 텍스트 파싱은 LLM 출력의 비결정성에 취약 (포맷 오류, 거짓 양성)
- Goose의 Summon Extension이 이미 `sub_recipes`를 통한 에이전트 위임을 지원

**권장**: MCP 도구 기반 통신으로 전환 검토.
```
// 현재: 텍스트 파싱
"@reviewer: please check this" → parse_mention() → delegation

// 권장: 전용 MCP 도구
delegate_to(agent="reviewer", message="please check this") → 구조화된 JSON
broadcast(message="found critical bug") → 구조화된 JSON
```

이렇게 하면 Goose의 도구 검사(보안, 권한, 반복 체크) 파이프라인도 자연스럽게 적용된다.

### 3.2 `unsafe { set_var }` 패턴 (경미)
**현상**: `goose_bridge.rs`에서 `GOOSE_RECIPE_PATH`를 `unsafe { std::env::set_var }` 으로 설정.

**분석**: Rust 2024 에디션에서 `set_var`가 `unsafe`로 변경된 것은 맞고, 코드에 충분한 문서화와 안전장치(ENV_LOCK, "반드시 멀티스레드 전에 호출" 제약)가 있다. 하지만 Goose 자체가 `GOOSE_RECIPE_PATH`를 config 파일이나 API로도 설정할 수 있다면 env var 의존성을 줄이는 것이 더 안전하다.

---

## 4. Goose를 더 잘 활용할 수 있는 기회

### 4.1 Goose Recipe의 `sub_recipes` 활용도 높이기
**현재**: `TeamDefinition::to_recipe()`로 변환은 가능하지만, 실제 팀 실행은 자체 `TeamOrchestrator`가 담당한다.

**기회**: Goose의 `sub_recipes` + Summon Extension은 이미 에이전트 위임을 네이티브로 지원한다. Chain 워크플로우의 경우 Goose의 `sequential_when_repeated` 플래그를 활용하면 자체 `ChainExecutor` 로직의 일부를 Goose에 위임할 수 있다.

**트레이드오프**: OpenGoose의 `TeamOrchestrator`는 오케스트레이션 DB 추적, 위임 큐, dead letter 처리, resume 지원 등 Goose의 sub_recipe보다 훨씬 풍부한 기능을 제공한다. 완전 대체는 비현실적이지만, 단순한 Chain 케이스에서는 Goose 네이티브 경로를 옵션으로 제공할 수 있다.

### 4.2 Goose의 `AgentEvent` 스트림 더 활용하기

**현재**: 세 가지 실행 메서드가 존재하며 이벤트 처리 수준이 다르다:

| 메서드 | Message | ModelChange | HistoryReplaced | McpNotification | 실시간 전파 |
|---|---|---|---|---|---|
| `run()` (runner.rs:356) | ✅ | ❌ 무시 | ❌ 무시 | ❌ 무시 | ❌ |
| `run_streaming()` (runner.rs:394) | ✅ (StreamChunk) | ❌ 무시 | ❌ 무시 | ❌ 무시 | ✅ (텍스트만) |
| `run_with_events()` (runner.rs:436) | ✅ | ✅ 수집 | ✅ 수집 | ✅ 수집 | ❌ (사후 요약) |

`run_with_events()`가 `AgentEventSummary` (runner.rs:42-50)를 반환하지만, 이는 **사후 요약**이지 실시간 이벤트 전파가 아니다. EventBus로의 실시간 포워딩이 실행 중에는 발생하지 않는다.

**기회**: `run()`/`run_streaming()`에서도 모든 AgentEvent를 EventBus로 실시간 포워딩하면:
- Witness 패턴의 에이전트 liveness 감지 기반 (마지막 이벤트 시간으로 stuck 판단)
- Extension 알림을 팀 오케스트레이션 컨텍스트에 전파 가능
- 모델 전환/컨텍스트 압축을 대시보드에 즉시 표시 가능

### 4.2.1 EventBus::subscribe_reliable() — Witness 패턴의 기반

`EventBus` (opengoose-types/src/events.rs:366)에는 `subscribe_reliable()` 메서드가 있다:

```rust
pub fn subscribe_reliable(&self) -> mpsc::UnboundedReceiver<AppEvent>
```

일반 `subscribe()`가 `broadcast` 채널을 사용해 버퍼 초과 시 이벤트를 잃을 수 있는 반면, `subscribe_reliable()`은 `mpsc::unbounded_channel`을 사용하여 **이벤트 누락이 없다**. Gas Town의 Witness(에이전트 헬스 순찰)처럼 모든 이벤트를 빠짐없이 추적해야 하는 감독 컴포넌트의 기반이 된다.

### 4.2.2 WorkStatus::Cancelled 및 chain resume — Polecat 상태머신 프리미티브

`WorkItemStore` (opengoose-persistence/src/work_items.rs)에 이미 존재하는 프리미티브들:

- `WorkStatus::Cancelled` — Gas Town Polecat의 상태 전이 중 취소에 해당
- `find_resume_point(parent_id)` (work_items.rs:231-244) — Chain 워크플로우에서 마지막 완료 단계를 찾아 다음 단계 번호와 출력을 반환. 에이전트 실패 후 재시작 시 처음부터 다시 실행하지 않고 중단 지점부터 재개 가능

이들은 Gas Town의 Polecat 상태머신 (Working→Idle→Stuck→Zombie→Done) 전체를 구현하기 위한 기존 기반이다.

### 4.2.3 RemoteAgent WebSocket 프로토콜 — 분산 에이전트 헬스 모니터링

`RemoteAgent` (opengoose-teams/src/remote.rs)는 WebSocket 기반 원격 에이전트 참여 프로토콜을 정의한다:

- **ProtocolMessage** 8종: Handshake, HandshakeAck, Heartbeat, MessageRelay, Broadcast, Disconnect, Error, Reconnect/ReconnectAck
- **ConnectionState** 4단계: Connecting → Connected → Disconnecting, Reconnecting
- **RemoteAgentRegistry**: 연결된 에이전트 추적, stale 감지, 연결 수/업타임 메트릭
- **Reconnect**: `last_event_id`로 이벤트 리플레이 지원

이 프로토콜은 현재 팀 내부용이지만, 확장하면 Gas Town 수준의 분산 에이전트 헬스 모니터링 인프라가 될 수 있다. Heartbeat 메커니즘이 Witness의 stuck/zombie 감지 기반으로 활용 가능하다.

### 4.3 Goose의 `PermissionManager` 활용
**현재**: 도구 권한 관리에 대한 OpenGoose 측 구현이 보이지 않는다.

**기회**: 팀 오케스트레이션에서 에이전트별 도구 권한을 차등 적용할 수 있다. 예: reviewer 프로필은 파일 수정 금지, developer 프로필은 전체 접근 허용.

### 4.4 Goose의 `GooseMode` / `SessionExecutionMode` 활용
**현재**: OpenGoose가 자체적으로 팀 실행 모드를 관리한다.

**기회**: Goose의 `SessionExecutionMode::SubTask(parent)` 모드는 팀 내 위임 실행과 정확히 일치하는 개념이다. 이를 활용하면 Goose 내부의 컨텍스트 관리(부모 세션 참조 등)를 자동으로 활용할 수 있다.

### 4.5 Goose의 `Recipe::parameters` 활용
**현재**: `AgentProfile`에 `parameters` 필드가 있지만 실제 활용도가 낮아 보인다.

**기회**: 팀 정의에서 에이전트 파라미터를 선언하고, 오케스트레이션 시 동적으로 바인딩하면 재사용성이 높아진다. Goose의 `RecipeParameter` (String, Number, Boolean, Date, File, Select) 타입을 그대로 활용할 수 있다.

### 4.6 Goose의 `conversation::fix_conversation()` 활용
**현재**: `seed_history()`가 단순히 메시지를 순서대로 추가한다.

**기회**: 오래된 대화 이력을 로드할 때 Goose의 `fix_conversation()` 파이프라인 (고아 도구 호출 제거, 역할 교대 보장 등)을 통과시키면 더 견고한 대화 컨텍스트를 구성할 수 있다.

---

## 5. 종합 평가

| 영역 | 평가 | 비고 |
|---|---|---|
| Agent/Provider 활용 | ✅ 우수 | Goose API를 정확히 사용 |
| Session 관리 | ✅ 우수 | 네이티브 세션 + 보조 DB 이중화는 합리적 |
| Recipe 호환 | ✅ 우수 | 양방향 변환, CLI 호환 확보 |
| Gateway 아키텍처 | ✅ 우수 | Goose의 Gateway trait 정확히 구현 |
| Extension 관리 | ✅ 우수 | 자체 구현 없이 Goose에 위임 |
| 팀 오케스트레이션 | ⚠️ 독자 구현 (적절) | Goose에 없는 기능, 재구현 아님 |
| 에이전트 간 통신 | ⚠️ 텍스트 파싱 | MCP 도구 기반으로 전환 권장. MessageBus/AgentMessageStore 인프라는 존재하나 에이전트가 직접 사용하지 않음 |
| AgentEvent 활용 | 🔧 개선 가능 | `run_with_events()`로 수집은 가능하나 실시간 EventBus 포워딩 미구현 |
| 권한/모드 관리 | 🔧 개선 가능 | Goose의 Permission/Mode API 미활용 |
| 에이전트 헬스 모니터링 | 🔧 인프라 준비됨 | `EventBus::subscribe_reliable()`, RemoteAgent heartbeat, WorkStatus::Cancelled 등 기반 존재. Witness 구현 필요 |

**결론**: OpenGoose는 Goose의 핵심 기능을 잘 활용하면서, Goose에 없는 멀티 채널/팀 오케스트레이션 기능을 적절히 추가한 프로젝트다. 재구현은 거의 없다. 가장 큰 개선 포인트는 텍스트 파싱 기반 에이전트 통신을 MCP 도구 기반으로 전환하는 것이다.
