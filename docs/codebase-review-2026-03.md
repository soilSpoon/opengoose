# OpenGoose 코드베이스 리뷰 (2026-03, rev3)

> 본 문서는 프로젝트의 지향점을 다음 레퍼런스 철학과 함께 해석한다:
> - `openclaw`, `nanobot`, `nanoclaw`, `zeroclaw`, `openfang`, `ironclaw`의 오케스트레이션/모듈화 접근
> - `zeroclaw`, `openfang`, `pi-mono` 스타일의 **미니멀 코어 + 강한 모듈 분리 + 테스트 용이성**
> - 그리고 무엇보다 **Goose Native**하게 goose 기능을 우선 활용하는 방향

---

## 1) 아키텍처/철학 요약

- 프로젝트는 `goose`를 엔진으로 두고, 채널 어댑터를 분리하는 오케스트레이터 철학을 채택한다.
- 실제 런타임 구성은 `opengoose-cli`에서 공통 `Engine` + 채널별 `Gateway`들을 조립해 병렬 구동한다.
- 팀 오케스트레이션은 `Engine`에서 우선 처리되고, 팀 미활성 시 goose 단일 에이전트 경로로 폴백한다.
- 방향성 측면에서 현재 구조는 "코어는 얇게, 기능은 모듈로 분리"라는 목표와 대체로 일치한다.

### Goose-native 관점 체크

- 좋은 점: gateway 연동, 페어링/핸들러/단일 에이전트 경로를 goose 흐름에 맞춰 구성해 재사용성이 높다.
- 보완점: 채널별로 반복되는 구현이 늘어나면서, 코어를 얇게 유지하려는 목표가 점차 약해질 위험이 있다.
- 원칙: 새 기능을 추가할 때 "goose에서 이미 제공되는가?"를 먼저 확인하고, opengoose는 조립/정책/운영 계층에만 집중한다.

### 12-크레이트 의존성 그래프

```
opengoose-types (기반 — Platform, SessionKey, EventBus, StreamChunk)
  ├── opengoose-secrets
  ├── opengoose-persistence (Database, SessionStore, OrchestrationStore)
  ├── opengoose-profiles → opengoose-types
  ├── opengoose-teams → profiles, persistence, types
  ├── opengoose-provider-bridge → secrets
  ├── opengoose-core → types, profiles, teams, persistence
  ├── opengoose-{discord,slack,telegram} → core, types (채널 어댑터)
  ├── opengoose-tui → types, secrets, provider-bridge, teams
  └── opengoose-cli → 모든 크레이트 조립
```

## 2) 구조적 강점

- **GatewayBridge 패턴**: 채널 공통 흐름(페어링, 이벤트, 스트리밍, 영속화, single-agent 폴백)을 캡슐화해 채널별 차이를 최소화.
- **SessionManager**: DB를 source of truth로 두고 DashMap write-through cache를 사용하는 일관성 우선 전략.
- **스트리밍 추상화**: `StreamResponder` + `drive_stream` + `ThrottlePolicy`로 플랫폼 독립적 스트리밍 지원.
- **EventBus Pub-Sub**: broadcast 기반 느슨한 결합으로 컴포넌트 간 의존성 최소화.
- **팀 오케스트레이션**: Strategy 패턴(`OrchestrationPattern`: Chain/FanOut/Router)으로 실행 전략 교체 가능.
- **handle_team_command 중앙화**: `/team` 커맨드 정책 로직이 Engine에 집중되어 어댑터는 입력 추출/출력 전달만 담당.
- **message_utils 공통화**: `split_message`와 `truncate_for_display`가 core에 위치하여 어댑터에서 재사용.

## 3) 중복 코드 분석 (코드 근거 포함)

### A. `send_message` 구조 반복 (3곳)

Discord(`gateway.rs:152-178`), Slack(`gateway.rs:371-387`), Telegram(`gateway.rs:414-436`)이 모두 동일한 구조:
1. `bridge.on_outgoing_message()` 호출 → 영속화/페어링/이벤트
2. `SessionKey::from_stable_id()` 로 세션키 재추출
3. 플랫폼별 전송 API 호출

`on_outgoing_message`가 이미 내부에서 `SessionKey::from_stable_id`를 수행하므로, 반환값으로 session_key를 제공하면 어댑터의 중복 파싱을 제거할 수 있다.

**rev3 적용**: `on_outgoing_message`가 `SessionKey`를 반환하도록 변경, 어댑터에서 중복 파싱 제거.

### B. 릴레이 에러 처리 (3곳)

Discord/Slack/Telegram 각 gateway에서 `relay_and_drive_stream` 호출 후 동일한 에러 처리:
```rust
if let Err(e) = self.bridge.relay_and_drive_stream(...).await {
    self.event_bus.emit(AppEventKind::Error { context: "relay".into(), message: e.to_string() });
    error!(%e, "failed to relay message");
}
```

**rev3 적용**: `relay_and_drive_stream` 내부에서 에러 시 자동으로 event_bus에 emit하도록 변경. 어댑터는 에러 로깅만 담당.

### C. `split_message` 테스트 중복

- Discord `gateway.rs:402-467`: 7개 split_message 테스트
- Slack `gateway.rs:482-499`: 2개 split_message 테스트
- Core `message_utils.rs:42-110`: 이미 포괄적인 테스트 존재

이 테스트들은 `core::message_utils`의 함수를 테스트하므로, 어댑터에서 제거하고 core 테스트만 유지하는 것이 적절하다.

**rev3 적용**: Discord/Slack의 중복 split_message 테스트 제거. 커버리지가 부족한 케이스(multiple chunks)는 core로 이동.

### D. `SessionStore` 반복 생성

`Engine`에서 `record_user_message`, `record_assistant_message`, `sessions()` 호출 시마다 `SessionStore::new(self.db.clone())` 생성. Arc::clone은 경량이지만, SessionStore를 Engine 필드로 한 번 생성해 재사용하는 것이 의도를 더 명확하게 표현한다.

**rev3 적용**: Engine에 `session_store: SessionStore` 필드 추가, 반복 생성 제거.

### E. `finalize_draft` 패턴 중복

Discord/Slack의 finalize_draft: "첫 청크로 원본 수정 + 나머지를 새 메시지로 전송"
Telegram만 truncate_for_display 사용으로 약간 다름.

**상태**: P1에서 default trait 구현 또는 헬퍼로 추출 예정.

## 4) 아키텍처 개선 기회

### 경계 재정의

- `Engine::process_message_streaming`은 잘 통합되었으나, 팀 스트리밍이 아직 전체 응답 1회 방출 → 토큰 단위 전환 시 이 메서드만 변경하면 됨(인프라 준비 완료).
- `collect_gateways`의 정적 if-chain → Factory/Registry 패턴 전환으로 채널 추가 비용 절감.
- Pairing handler의 first-gateway 결합 → 멀티채널 라우팅 필요.
- TUI `app.rs` (1,162줄) → 상태 관리/이벤트 처리/렌더링 분리 검토.

## 5) 리팩터링 원칙 (프로젝트 철학 반영)

1. **Goose 우선 재사용**: goose에 있는 기능을 opengoose에서 재구현하지 않는다.
2. **코어 최소화**: core는 정책/추상화만, 채널 SDK/프로토콜 세부사항은 어댑터로 격리.
3. **중복 제거 우선순위**: 공통화로 인한 추상화 비용보다 중복 누적 비용이 큰 지점부터 통합.
4. **테스트 가능성 우선 설계**: 분리 기준을 "단위 테스트 가능한가?"로 판단.
5. **확장 계약 명확화**: 새 채널 추가 시 필요한 최소 인터페이스를 문서화/고정.

## 6) 실행 백로그 (DoD 포함)

### P0-1. `on_outgoing_message`가 SessionKey 반환 ✅ (rev3)
- `GatewayBridge::on_outgoing_message` → `SessionKey` 반환
- 어댑터에서 중복 `SessionKey::from_stable_id` 제거

### P0-2. 릴레이 에러 이벤트 공통화 ✅ (rev3)
- `relay_and_drive_stream` 내부에서 에러 시 `AppEventKind::Error` 자동 emit
- 어댑터에서 `event_bus.emit(Error {...})` 중복 제거

### P0-3. split_message 테스트 통합 ✅ (rev3)
- Discord/Slack 중복 테스트 제거
- 커버리지 부족분(multiple chunks 등) core 테스트로 이동

### P0-4. SessionStore 반복 생성 제거 ✅ (rev3)
- Engine에 `session_store: SessionStore` 필드 추가
- `record_user_message`, `record_assistant_message`, `sessions()` 에서 재사용

### P1-1. finalize_draft 공통 로직 추출
- "첫 청크로 수정 + 나머지 새 메시지" 패턴 → default trait 구현 또는 헬퍼 함수

### P1-2. gateway 수집 플러그인화
- `collect_gateways` 정적 if-chain → factory/registry 전환

### P1-3. Pairing 라우팅 개선
- first-gateway 결합 제거 → 플랫폼 지정형 라우팅

### P1-4. TUI app.rs 분할
- 1,162줄 → 상태 관리/이벤트 처리/렌더링 분리

### P2-1. 팀 스트리밍 토큰 단위 전환
- 현재 전체 응답 1회 Delta → LLM 토큰 스트리밍 연동

## 7) 외부 레퍼런스 비교 분석

### 관찰 요약

1. **OpenClaw** — 단일 장기 실행 gateway + 다채널 + WS control plane을 명확하게 문서화. protocol/schema/codegen 표준화.
2. **NanoBot** — "초경량 + 연구 친화성" 포지셔닝. 다채널 지원하면서 core 라인 수를 관리 대상으로 유지.
3. **NanoClaw** — "설정보다 코드 변경, 기능보다 스킬" 미니멀 철학. registry/self-registration 구조.
4. **ZeroClaw** — trait-driven, provider/channel/tool swappable. 런타임 계층 교체 가능성을 일급 개념으로 정의.
5. **IronClaw** — 보안(샌드박스/허용목록/비밀 주입)을 아키텍처 1급 요구사항으로 정의.
6. **Pi-mono** — 역할별 패키지 분할 + 개발/검증 루틴(`build -> check -> test`) 문서 고정.

### OpenGoose에 적용 가능한 실천 항목

- **Goose-native 계약 문서화**: "Goose가 제공하는 것 vs OpenGoose가 제공하는 것" README 동기화
- **미니멀 코어 지표**: core crate의 public API 수/모듈 수/중복 함수 수 추적
- **확장 계약 명문화**: 새 채널 추가 시 필수 구현 포인트 체크리스트화
- **테스트 계층 분리**: core(정책), adapter(contract), e2e(smoke)

## 8) 결론

현재 구조는 **"goose 엔진 + 멀티 채널 오케스트레이터"** 목표에 대체로 부합하며, `GatewayBridge` 중심 설계는 확장성 측면에서 좋은 선택이다. rev3에서 P0 항목(SessionKey 반환, 에러 공통화, 테스트 통합, SessionStore 캐싱)을 적용하여 채널별 반복 코드를 줄이고 core 경계를 강화했다.

다음 단계의 핵심은 P1 항목(finalize_draft 공통화, gateway 팩토리, pairing 라우팅, TUI 분할)으로, 이를 완료하면 새 채널 추가 비용과 유지보수 부담이 크게 줄어들 것이다.
