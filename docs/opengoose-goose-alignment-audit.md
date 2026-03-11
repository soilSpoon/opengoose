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
**현재**: `AgentRunner::run()`/`run_streaming()`이 `AgentEvent::Message`만 처리하고 나머지 이벤트는 무시한다.

```rust
// 현재 코드 (runner.rs)
if let AgentEvent::Message(msg) = event_result? { ... }
```

**기회**: `AgentEvent::McpNotification`, `ModelChange`, `HistoryReplaced` 이벤트를 활용하면:
- Extension 알림을 팀 오케스트레이션 컨텍스트에 전파 가능
- 모델 전환 이벤트를 EventBus에 emit하여 TUI/대시보드에 표시 가능
- 컨텍스트 압축 이벤트를 로그하여 디버깅에 활용 가능

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
| 에이전트 간 통신 | ⚠️ 텍스트 파싱 | MCP 도구 기반으로 전환 권장 |
| AgentEvent 활용 | 🔧 개선 가능 | Message 외 이벤트 미활용 |
| 권한/모드 관리 | 🔧 개선 가능 | Goose의 Permission/Mode API 미활용 |

**결론**: OpenGoose는 Goose의 핵심 기능을 잘 활용하면서, Goose에 없는 멀티 채널/팀 오케스트레이션 기능을 적절히 추가한 프로젝트다. 재구현은 거의 없다. 가장 큰 개선 포인트는 텍스트 파싱 기반 에이전트 통신을 MCP 도구 기반으로 전환하는 것이다.
