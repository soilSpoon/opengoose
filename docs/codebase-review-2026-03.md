# OpenGoose 코드베이스 리뷰 (2026-03, rev2)

> 본 문서는 프로젝트의 지향점을 다음 레퍼런스 철학과 함께 해석한다:
> - `openclaw`, `nanobot`, `nanoclaw`, `zeroclaw`, `openfang`, `ironclaw`의 오케스트레이션/모듈화 접근
> - `zeroclaw`, `openfang`, `pi-mono` 스타일의 **미니멀 코어 + 강한 모듈 분리 + 테스트 용이성**
> - 그리고 무엇보다 **Goose Native**하게 goose 기능을 우선 활용하는 방향

## 1) 아키텍처/철학 요약

- 프로젝트는 `goose`를 엔진으로 두고, 채널 어댑터를 분리하는 오케스트레이터 철학을 채택한다.
- 실제 런타임 구성은 `opengoose-cli`에서 공통 `Engine` + 채널별 `Gateway`들을 조립해 병렬 구동한다.
- 팀 오케스트레이션은 `Engine`에서 우선 처리되고, 팀 미활성 시 goose 단일 에이전트 경로로 폴백한다.
- 방향성 측면에서 현재 구조는 “코어는 얇게, 기능은 모듈로 분리”라는 목표와 대체로 일치한다.

### Goose-native 관점 체크

- 좋은 점: gateway 연동, 페어링/핸들러/단일 에이전트 경로를 goose 흐름에 맞춰 구성해 재사용성이 높다.
- 보완점: 채널별로 반복되는 구현이 늘어나면서, 코어를 얇게 유지하려는 목표가 점차 약해질 위험이 있다.
- 원칙: 새 기능을 추가할 때 “goose에서 이미 제공되는가?”를 먼저 확인하고, opengoose는 조립/정책/운영 계층에만 집중한다.

## 2) 구조적 강점

- 공통 브리지(`GatewayBridge`)로 채널 공통 흐름(페어링, 이벤트, single-agent 폴백)을 캡슐화해, 채널별 차이를 최소화했다.
- `SessionManager`는 DB를 source of truth로 두고 read-through cache를 사용하는 일관성 우선 전략을 취한다.
- 스트리밍은 `StreamResponder` + `drive_stream` + `ThrottlePolicy`로 플랫폼 독립 추상화가 잘 되어 있다.

## 3) 개선 기회

### A. 문서-코드 정합성

- README/architecture 문서의 crate 수, 구조 설명이 현재 워크스페이스와 불일치한다.
- 현재는 Discord 전용이 아니라 Telegram/Slack/Teams/Persistence/Provider bridge까지 포함한 다채널 구성이다.

### B. 중복 코드

- 메시지 분할 로직이 `core::message_utils`에 이미 있음에도 Discord/Slack에서 동일 구현이 반복된다.
- `/team` 명령 처리 로직이 Discord/Slack/Telegram에 유사 패턴으로 중복된다.
- `send_message` 처리(bridge 이벤트 반영 → 플랫폼 전송)도 채널별로 구조가 거의 같다.

### C. 경계 재정의 필요

- `Engine::process_message`와 `process_message_streaming`의 공통 전처리(이벤트 emit + user 메시지 영속화)가 반복된다.
- 팀 스트리밍 경로는 아직 토큰 단위가 아닌 전체 응답 1회 방출이어서, API는 스트리밍인데 실제 체감은 제한적이다.

### E. 테스트 전략(미니멀 코어 지향과 연결)

- 핵심 정책 로직(예: team 활성/비활성 결정, `/team` 커맨드 해석)을 I/O 코드에서 분리하면 단위 테스트가 쉬워진다.
- 채널 어댑터 테스트는 “입력 이벤트 -> 공통 서비스 호출 인자”와 “응답 포맷” 검증에 집중하고,
  오케스트레이션 규칙 테스트는 core 계층에서 플랫폼 독립적으로 수행하는 구조가 바람직하다.
- 목표는 “채널 추가 시 기존 core 테스트는 그대로 통과, 채널별 contract 테스트만 추가”가 되도록 만드는 것이다.

### D. 운영/확장 관점

- 현재 `collect_gateways`는 자격증명 보유 여부 기반 정적 분기라, 채널 추가 시 if 블록이 계속 증가한다.
- 플러그인 레지스트리(예: `Vec<Box<dyn GatewayFactory>>`) 형태로 전환하면 새 채널 추가 비용을 더 낮출 수 있다.

## 4) 우선순위 제안

1. 문서 정합성 복구(README + docs/codebase-review-2026-03.md) — 온보딩/유지보수 즉시 효과
2. 메시지 분할 로직 단일화(`opengoose_core::message_utils::split_message` 재사용)
3. `/team` 명령 처리 공통화(플랫폼별 parser + 공통 service 분리)
4. gateway 수집/초기화 플러그인화
5. 팀 스트리밍 실제 토큰 단위 연동

## 4.1) 리팩터링 원칙 (프로젝트 철학 반영)

1. **Goose 우선 재사용**: goose에 있는 기능을 opengoose에서 재구현하지 않는다.
2. **코어 최소화**: core는 정책/추상화만, 채널 SDK/프로토콜 세부사항은 어댑터로 격리.
3. **중복 제거 우선순위**: 공통화로 인한 추상화 비용보다 중복 누적 비용이 큰 지점부터 통합.
4. **테스트 가능성 우선 설계**: 분리 기준을 “단위 테스트 가능한가?”로 판단.
5. **확장 계약 명확화**: 새 채널 추가 시 필요한 최소 인터페이스를 문서화/고정.

## 5) 결론

현재 구조는 **"goose 엔진 + 멀티 채널 오케스트레이터"** 목표에 대체로 부합하며, `GatewayBridge` 중심 설계는 확장성 측면에서 좋은 선택이다. 다만 **문서 드리프트**와 **채널별 반복 코드**가 누적되고 있어, 지금 시점에 정리하면 이후 기능 확장 속도와 안정성이 크게 좋아질 것으로 보인다.

특히 사용자 의도(Goose 기능 최대 활용 + 미니멀 코어 + 모듈화/테스트 용이성)에 맞춰 보면,
다음 단계의 핵심은 “새 기능 추가”보다 “경계 정리와 공통화”다. 이 작업을 먼저 수행하면 이후 확장(새 채널/새 오케스트레이션 패턴)의 난이도가 유의미하게 낮아질 것이다.

## 6) 외부 레퍼런스 비교 분석 (현재 레포 영향 없이 수행)

### 분석 방법

- 외부 레포 분석은 모두 `/tmp/opengoose-benchmark`에서 shallow clone으로 수행했다.
- 현재 레포(`/workspace/opengoose`)에는 문서 파일 외 코드/설정 변경을 하지 않았다.
- 이름 기반 검색 시 동명이인 레포가 존재하므로, 각 이름의 상위 매칭 중 실제 에이전트/오케스트레이션 성격이 강한 레포를 우선 비교했다.
- `openfang`은 동일 맥락의 명확한 대표 레포를 식별하기 어려워 이번 비교표에서는 제외했다.

### 관찰 요약

1. **OpenClaw**
   - 단일 장기 실행 gateway + 다채널 + WS control plane을 매우 명확하게 문서화했다.
   - protocol/schema/codegen/운영 불변조건까지 문서 수준에서 표준화되어 있어 확장/기여 온보딩이 빠르다.

2. **NanoBot**
   - “초경량”과 “연구 친화성(가독성/확장성)”을 전면에 둔 포지셔닝이 분명하다.
   - 다채널 지원을 유지하면서도 코어 라인 수를 관리 대상으로 둔 점이 인상적이다.

3. **NanoClaw**
   - “설정보다 코드 변경”, “기능 추가보다 스킬”이라는 강한 미니멀 철학을 채택한다.
   - 단일 프로세스 + registry/self-registration 구조로, 확장 시 핵심 경계가 단순하다.

4. **ZeroClaw**
   - trait-driven, provider/channel/tool swappable, pluggable everything을 명시적으로 내세운다.
   - 인프라/런타임 계층의 교체 가능성을 일급 개념으로 두어 장기 확장성에 유리하다.

5. **IronClaw**
   - 보안(샌드박스/허용목록/비밀 주입 경계/유출 탐지)을 아키텍처 1급 요구사항으로 정의한다.
   - 채널/도구/런타임 계층 분리와 운영 자동화를 함께 강조한다.

6. **Pi-mono**
   - 모노레포에서 역할별 패키지를 분할(LLM API, agent core, coding agent, TUI/Web UI, 운영 CLI)해 경계를 선명히 유지한다.
   - 개발/검증 루틴(`build -> check -> test`)을 문서에 고정해 협업 품질을 높인다.

### OpenGoose에 적용 가능한 실천 항목 (업데이트)

- **Goose-native 계약 문서화 강화**
  - “Goose가 제공하는 것 vs OpenGoose가 제공하는 것”을 README와 `docs/codebase-review-2026-03.md`에 동기화해 유지.
  - 신규 기능 PR 템플릿에 “goose 재사용 검토 결과” 항목 추가.

- **미니멀 코어 지표 도입**
  - core crate의 public API 수/모듈 수/중복 함수 수를 간단한 스크립트로 추적.
  - 채널별 커맨드 처리/메시지 분할 중복을 우선 제거해 core 경계의 의미를 강화.

- **확장 계약(Extension Contract) 명문화**
  - 새 채널 추가 시 필수 구현 포인트(입력 정규화, 세션키 매핑, 스트리밍 draft 전략, 오류 이벤트 규약)를 체크리스트화.
  - `collect_gateways`의 정적 분기를 팩토리/레지스트리로 전환해 채널 추가의 표준 경로를 만든다.

- **테스트 계층 분리**
  - core: 플랫폼 독립 정책/오케스트레이션 테스트
  - adapter: 입력 이벤트 -> 공통 bridge 호출 계약 테스트
  - e2e: 최소한의 채널 smoke 테스트

이 네 가지를 먼저 적용하면, 사용자가 원하는 방향(Goose 기능 최대 활용 + 미니멀 코어 + 높은 확장성/테스트 용이성)에 더 일관되게 수렴할 수 있다.

## 7) 코드 근거 기반 추가 보완 포인트 (실행 우선)

아래 항목은 문서 의견이 아니라, 현재 코드에서 바로 확인되는 갭을 기준으로 정리했다.

1. **Pairing 트리거가 첫 번째 gateway에만 결합됨**
   - 현재 런타임에서 pairing handler는 `gateways.first()` 기준으로 1개만 등록된다.
   - 멀티채널 환경에서 pairing UX가 채널별로 일관되지 않을 수 있어, 최소한 플랫폼 선택형/브로드캐스트형으로 확장할 필요가 있다.

2. **CLI 정체성 문구와 현재 멀티채널 방향 정합성 유지 필요**
   - CLI 상단 설명은 멀티채널 문구로 정렬했으며, 향후 기능 확장 시 설명 문자열이 다시 드리프트되지 않도록 점검이 필요하다.
   - README/실제 구현/CLI help 텍스트를 같은 변경 단위로 관리한다.

3. **메시지 분할 공통화 진행 필요(진행 상태 반영)**
   - core 공통 `split_message` 재사용 방향이 맞고, adapter 로컬 사본 제거를 우선 추진한다.
   - 상태: `P0-1` 항목에서 완료 기준(DoD)으로 관리한다.

4. **메시지 전처리 중복(동일 정책 반복) 유지**
   - `Engine::process_message`와 `process_message_streaming`은 동일한 전처리 단계(이벤트 emit + user 메시지 기록)를 별도 유지한다.
   - 정책 경계를 더 얇게 유지하려면 공통 전처리 함수로 수렴시키는 것이 바람직하다.

## 8) 실행 백로그(DoD 포함)

### P0-1. 메시지 분할 단일화 ✅ (완료)
- 작업
  - Discord/Slack 로컬 `split_message` 제거
  - `opengoose_core::message_utils::split_message`만 사용
- 완료 근거
  - `crates/opengoose-discord/src/gateway.rs`에서 core `split_message` import/사용
  - `crates/opengoose-slack/src/gateway.rs`에서 core `split_message` import/사용
- DoD
  - adapter 내부 분할 함수 0개
  - UTF-8/개행 경계 테스트는 기존 수준 유지 또는 증가

### P0-2. `/team` 커맨드 처리 공통 서비스화
- 작업
  - 채널별 파싱/응답 포맷은 adapter에 유지
  - 팀 활성/비활성/list/not-found 정책은 core 서비스로 추출
- DoD
  - 팀 정책 분기 로직이 core 1곳에 존재
  - adapter는 입력 추출 + 출력 렌더링 역할만 수행

### P0-3. Pairing 라우팅 구조 개선
- 작업
  - 단일 first-gateway 결합 제거
  - 플랫폼 지정 또는 다중 브리지 fan-out 전략 도입
- DoD
  - 어떤 채널이 먼저 초기화되었는지와 무관하게 pairing 요청이 의도한 대상에 도달
  - 회귀 테스트(또는 최소 integration 시나리오) 추가

### P1-1. gateway 수집/초기화 플러그인화
- 작업
  - `collect_gateways`의 정적 if-chain을 factory/registry로 전환
- DoD
  - 신규 채널 추가 시 기존 함수 수정량 최소화
  - 채널별 자격증명 규칙은 factory 단위로 캡슐화

### P1-2. 메시지 전처리 공통화
- 작업
  - `process_message*`의 공통 전처리 추출
- DoD
  - 동일 전처리 코드 중복 제거
  - 이벤트/영속화 순서 불변 보장 테스트 추가

## 9) 다음 3개 PR 제안 (작게, 빠르게)

1. **PR-1: TeamCommandService 도입(core) + adapter 연결**
2. **PR-2: PairingRouter 도입(first-gateway 결합 제거)**
3. **PR-3: collect_gateways registry/factory 전환**

위 3개를 완료하면, 문서에서 반복적으로 제기된 “중복/경계/확장성” 이슈의 체감도가 크게 개선된다.
