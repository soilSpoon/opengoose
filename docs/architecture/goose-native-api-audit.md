# Goose Native API 활용 범위 감사

> **작성일:** 2026-03-12
> **목적:** OpenGoose가 Goose API를 어디까지 직접 활용하는지, 자체 구현이 어디까지 필요한지 정리

---

## 1. 현재 사용 중인 Goose API

### 1.1 에이전트 실행 (핵심)

| Goose API | 위치 | 용도 |
|-----------|------|------|
| `Agent::new()` | `opengoose-teams/runner.rs` | 에이전트 인스턴스 생성 |
| `Agent::reply(msg, session_config, cancel_token)` | `runner.rs:340,366,408,446` | 스트리밍 응답 |
| `agent.update_provider(provider, session_id)` | `runner.rs` | LLM 프로바이더 설정 |
| `agent.override_system_prompt(prompt)` | `runner.rs` | 시스템 프롬프트 주입 |
| `agent.add_extension(config, session_id)` | `runner.rs` | MCP 확장 등록 |
| `SessionConfig { id, schedule_id, max_turns, retry_config }` | `runner.rs` | 세션 설정 구조체 |
| `CancellationToken` | `runner.rs` | 협력적 취소 |

### 1.2 프로바이더 관리

| Goose API | 위치 | 용도 |
|-----------|------|------|
| `goose::providers::providers()` | `provider-bridge/lib.rs` | 프로바이더 목록 조회 |
| `goose::providers::create(name, config, vec![])` | `provider-bridge/lib.rs` | 프로바이더 인스턴스 생성 |
| `provider.fetch_recommended_models()` | `provider-bridge/lib.rs` | 모델 목록 |
| `provider.configure_oauth()` | `provider-bridge/lib.rs` | OAuth 인증 |

### 1.3 세션 타입

| Goose API | 용도 |
|-----------|------|
| `SessionType::Gateway` | OpenGoose 채널 어댑터 세션 |
| `SessionType::SubAgent` | Goose 서브에이전트 세션 |

---

## 2. OpenGoose 자체 구현 (Goose에 없는 것)

### 2.1 멀티 채널 라우팅

Goose는 **단일 세션** 모델. OpenGoose가 추가한 것:

| 컴포넌트 | 기능 | Goose 대응 |
|----------|------|-----------|
| `GatewayBridge` | 다중 플랫폼 → 단일 엔진 라우팅 | 없음 |
| `Engine` | 팀 모드 vs 단일 에이전트 분기 | 없음 |
| `SessionManager` (OpenGoose) | session_key→team 매핑 + DashMap 캐시 | Goose SessionManager는 세션 생명주기만 |
| 채널 어댑터 (Discord/Slack/Telegram/Matrix) | 플랫폼별 API 연동 | 없음 |

### 2.2 팀 오케스트레이션

Goose의 서브에이전트 시스템을 확장:

| 컴포넌트 | 기능 | Goose 관계 |
|----------|------|-----------|
| `TeamOrchestrator` | Fan-out/chain/router 멀티에이전트 패턴 | `SubagentRunParams` 위에 구축 |
| `AgentRunner` | 에이전트 생명주기 관리 | `Agent::reply()` 래핑 |
| `MessageBus` | 에이전트간 통신 | Goose에 없음 |
| `Witness` | 좀비/정체 에이전트 감지 | Goose에 없음 |

### 2.3 작업 추적 (Beads)

Goose에 없는 완전 자체 구현:

| 컴포넌트 | 기능 |
|----------|------|
| `hash_id()` | SHA-256 + base36 작업 ID |
| `ready()` | 의존성 인식 실행 가능 태스크 필터 |
| `prime()` | 에이전트 컨텍스트 생성 |
| `compact()` | 완료 태스크 요약/다이제스트 |
| `WorkItemStore` | 계층적 작업 CRUD |
| `RelationStore` | blocks/depends_on 관계 |
| `MemoryStore` | 에이전트 키-값 메모리 |
| `CompactStore` | 다이제스트 관리 |

---

## 3. Goose native 확장 가능성

### 3.1 활용할 수 있지만 현재 미사용

| Goose API | 잠재적 용도 | 현재 상태 |
|-----------|-----------|----------|
| `SubagentRunParams` 직접 사용 | TeamOrchestrator에서 Goose 서브에이전트 시스템으로 위임 | AgentRunner가 자체 구현 |
| `Recipe.sub_recipes` | 멀티에이전트 구성을 YAML 레시피로 선언 | 자체 TeamConfig 사용 |
| `fix_conversation()` | 컨텍스트 윈도우 관리 | 미사용 |
| `AgentConfig` | 에이전트 동작 세부 설정 | 기본값 사용 |
| `PermissionManager` / `GooseMode` | 도구 승인 모드 (smart_approval 등) | 미사용 |

### 3.2 Goose native로 가기 위한 경로

**현재:** OpenGoose → AgentRunner → Agent::reply()
**목표:** OpenGoose → SubagentRunParams → Goose 서브에이전트 시스템

장점:
- Goose의 세션 관리, 이벤트 전파, MCP 알림 자동 활용
- `CancellationToken` + `on_message` 콜백 자동 연결
- 서브에이전트 세션이 Goose UI에서도 가시적

주의점:
- SubagentRunParams는 Recipe 기반 — OpenGoose의 TeamConfig를 Recipe로 변환 필요
- Goose 서브에이전트는 플랫 계층 (서브에이전트가 서브에이전트 생성 불가) — Gas Town의 계층적 감독과 다름

### 3.3 권장 액션

1. **단기 (현재):** AgentRunner 유지, Goose API 직접 호출 패턴 안정화
2. **중기:** TeamConfig → Recipe 변환 레이어 추가, SubagentRunParams 직접 사용 검토
3. **장기:** Goose의 서브에이전트 시스템이 계층적 감독 지원 시 전면 위임

---

## 4. 버전 호환성

| 크레이트 | Goose 버전 | 비고 |
|----------|-----------|------|
| opengoose-provider-bridge | v1.26.1 (git tag) | 프로바이더 API만 사용 |
| opengoose-core | v1.27.2 (git tag) | Agent, SessionConfig, 이벤트 |
| opengoose-teams | opengoose-core 경유 | AgentRunner → Agent::reply() |

Goose 버전 업그레이드 시 주의:
- `Agent::reply()` 시그니처 변경
- `SessionConfig` 필드 추가/변경
- `SubagentRunParams` 구조 변경
