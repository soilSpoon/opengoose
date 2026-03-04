# opengoose 아키텍처

## 한 줄 정의

opengoose는 **goose-native 오케스트레이터**다.
채널(Discord 등)은 입출력 인터페이스이며, 에이전트 코어는 goose에 둔다.

---

## 핵심 원칙

1. **Goose Native First** — goose의 Gateway trait를 직접 구현하여 일급 시민으로 동작한다.
2. **Channel is Interface, Not Core** — 채널 어댑터는 교체/추가 가능한 모듈이다.
3. **Orchestrator, not Agent Engine** — opengoose는 라우팅/정책/운영만. 지능은 goose.
4. **Minimal Core, Maximum Extensibility** — 코어 타입과 trait은 최소. 확장은 크레이트 추가로.

---

## 연동 방식: Gateway Trait 직접 구현

goose 크레이트를 Cargo 의존성으로 추가하고, `goose::gateway::Gateway` trait를 구현한다.
goose의 TelegramGateway와 동일한 패턴으로 Discord를 지원한다.

### goose가 자동으로 제공하는 것

- 페어링 플로우 (6자 코드, 300초 만료) — `GatewayHandler`
- 세션 생성/복원 (`SessionType::Gateway`) — `SessionManager`
- 에이전트 실행 + 스트리밍 응답 — `Agent::reply()`
- Extension 동기화 — `resolve_extensions_for_new_session()`
- 도구 루프 제한 (`GATEWAY_MAX_TURNS = 5`)
- 보안/권한 검사 — `SecurityInspector`, `PermissionManager`
- 반복 방지 — `RepetitionInspector`
- 재시도/타임아웃 — `RetryManager`

### opengoose가 구현하는 것

- Discord WebSocket 연결 + 이벤트 수신 (Twilight)
- `IncomingMessage` ↔ Discord 메시지 변환
- `OutgoingMessage` → Discord 응답 (2000자 분할, typing)
- Thread 기반 세션 매핑
- Guild/Role ACL
- 설정 로딩 (TOML)

---

## 책임 경계

| 책임 | 담당 | 근거 |
|------|------|------|
| 세션 생성/복원/저장 | goose | `SessionManager`, `SessionType::Gateway` |
| 페어링 | goose | `GatewayHandler::complete_pairing()` |
| 메시지 → 에이전트 전달 | goose | `GatewayHandler::relay_to_session()` |
| 에이전트 루프/도구 실행 | goose | `Agent::reply()` |
| 보안/권한/반복방지 | goose | `SecurityInspector`, `RepetitionInspector` |
| Extension/Recipe/Schedule | goose | `AgentManager`, `SchedulerTrait` |
| Discord 연결/이벤트 수신 | opengoose | `twilight-gateway` Shard |
| Discord 응답 전송/포맷 | opengoose | `twilight-http` Client |
| Thread→Session 매핑 | opengoose | `SessionRouter` |
| Guild/Role ACL | opengoose | `PolicyGuard` |
| 설정 관리 | opengoose | TOML + goose config 참조 |

---

## 워크스페이스 구조

```text
opengoose/
├── Cargo.toml              # workspace 정의
├── opengoose.toml           # opengoose 설정
├── crates/
│   ├── opengoose-types/     # Layer 0: 도메인 타입 (의존성 0)
│   ├── opengoose-core/      # Layer 1: trait + 라우터 + 정책
│   ├── opengoose-goose/     # Layer 2: Goose Gateway trait 구현
│   ├── opengoose-discord/   # Layer 2: Discord 어댑터 (Twilight)
│   └── opengoose-cli/       # Layer 3: 바이너리
```

### 의존성 그래프

```text
opengoose-types            ← leaf (serde, chrono)
       ↑
opengoose-core             ← types만 의존
    ↑         ↑
opengoose-goose  opengoose-discord   ← 서로 독립, core+types 의존
  + goose          + twilight-*
    ↑         ↑
opengoose-cli              ← 전체 조립
```

채널 추가 = 새 크레이트. 엔진 교체 = goose 크레이트 교체. core 변경 불필요.

### 각 크레이트 책임

**opengoose-types** — 도메인 타입만. 비즈니스 로직 없음.
- `ChannelEvent`, `ChannelResponse`, `SessionKey`, `UserIdentity`
- `TransportKind`, `MessageContent`, `PolicyConfig`

**opengoose-core** — 오케스트레이션 trait과 구현.
- `ChannelAdapter` trait (채널 어댑터 인터페이스)
- `EngineBridge` trait (에이전트 엔진 인터페이스)
- `SessionRouter` (thread→session 매핑)
- `PolicyGuard` (ACL 판정)
- `Bridge` (채널↔엔진 연결)

**opengoose-goose** — Goose 연동.
- `goose::gateway::Gateway` trait 구현
- `EngineBridge` 구현 (GooseEngineBridge)
- thread→goose session ID 매핑 관리

**opengoose-discord** — Discord 어댑터.
- Twilight Shard로 Discord WebSocket 연결
- Discord 이벤트 → `ChannelEvent` 변환
- `ChannelResponse` → Discord 메시지 전송
- 2000자 분할, typing indicator, 스레드 생성

**opengoose-cli** — 바이너리 진입점.
- `opengoose.toml` + goose config 로딩
- 크레이트 조립 + 실행

---

## 상호작용 모델

OpenFang 패턴 채택:

- **평문 메시지**: 채널/스레드 내 메시지를 수신 (MESSAGE_CONTENT intent)
- **`/` 커맨드**: `/pair`, `/recipe`, `/status` 파싱
- **스레드 자동 생성**: 채널에서 봇 멘션 시 새 스레드 생성 → 스레드 내 대화

```text
채널에서 @bot 멘션 → 새 스레드 생성 → Goose 세션 생성 → 스레드 내 대화
기존 스레드에서 메시지 → 기존 Goose 세션으로 전달
DM → 사용자 전용 세션
```

---

## 세션 매핑 (Thread 기반)

```text
Discord Thread ID → SessionRouter → Goose Session ID
```

- 새 스레드 → 새 Goose 세션 (`SessionType::Gateway`)
- 기존 스레드 메시지 → 기존 세션으로 relay
- 스레드 아카이브 → 세션 유휴 (Goose LRU 관리)

PlatformUser.user_id에 thread_id를 인코딩하여 Goose의 user→session 매핑을 thread→session으로 활용:
```
PlatformUser { platform: "discord", user_id: "{guild_id}:{thread_id}" }
```

---

## 페어링

Goose의 코드 기반 페어링 활용:

1. 사용자가 봇에 `/pair` 입력
2. 봇: "CLI에서 `goose gateway pair` 실행 후 코드를 입력하세요"
3. CLI에서 6자 코드 출력 (300초 만료)
4. 사용자가 Discord에 코드 입력
5. `GatewayHandler::complete_pairing()` → 인증 완료
6. 이후 스레드 생성 시 자동 세션 생성

---

## 설정

```toml
# opengoose.toml

[goose]
config_dir = "~/.config/goose"

[discord]
bot_token_env = "DISCORD_BOT_TOKEN"
guild_ids = []
intents = 33280  # GUILD_MESSAGES | MESSAGE_CONTENT

[discord.acl]
allowed_roles = []
allowed_channels = []

[gateway]
max_turns = 5
typing_interval_secs = 4
message_split_limit = 2000
```

goose 설정(`~/.config/goose/config.yaml`)에서 provider/model/extension 설정을 그대로 활용.

---

## 기술 스택

- **언어**: Rust (Edition 2024)
- **Discord**: twilight-gateway, twilight-http, twilight-model (0.16)
- **Goose**: goose crate (git dependency, tag pinning)
- **Async**: tokio
- **설정**: toml + serde
- **CLI**: clap

---

## MVP 범위

### 포함

- Discord 메시지 수신 (Twilight Shard)
- 봇 멘션 시 스레드 자동 생성
- Thread→Goose 세션 매핑
- Goose Gateway trait 구현 (메시지 relay + 응답 전송)
- 코드 페어링
- 2000자 메시지 분할
- Typing indicator
- Guild ACL
- TOML 설정

### 제외

- 다채널 동시 지원 (Slack/Telegram)
- 자체 에이전트 루프/메모리
- 고급 스케줄러/워크플로
- Slash command 등록 (Discord Application Commands)
- Embed/Component 등 리치 포맷

---

## 실행 순서

```text
Phase 1: 워크스페이스 + 타입
  ① 워크스페이스 전환 + goose 빌드 확인
  ② opengoose-types 도메인 타입
  ③ opengoose-core trait 정의

Phase 2: Goose 연동
  ④ opengoose-goose Gateway trait 구현
  ⑤ 세션 생성 + 메시지 relay + 스트리밍

Phase 3: Discord 연동
  ⑥ opengoose-discord Twilight Shard 연결
  ⑦ 이벤트 변환 + 스레드 생성 + 응답 전송

Phase 4: 통합
  ⑧ opengoose-cli 조립 + 설정 로딩
  ⑨ 페어링 + ACL
```

---

## 가드레일

1. **"오케스트레이션인가, 엔진 기능인가?"** — 엔진이면 goose 측.
2. transport 의존 타입을 core/types로 가져오지 않는다.
3. discord와 goose 크레이트는 서로 의존하지 않는다.
4. 채널 추가는 core 변경 없이 새 크레이트로.
