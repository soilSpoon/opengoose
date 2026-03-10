# Goose & Goosetown 아키텍처 분석

## 1. Goose 개요

[block/goose](https://github.com/block/goose)는 Block(Square)이 개발한 **Rust 기반 AI 에이전트 프레임워크**다. LLM을 통해 도구(tool)를 사용하며 소프트웨어 엔지니어링 작업을 자동화한다. MCP(Model Context Protocol)를 핵심 통신 프로토콜로 사용한다.

### 1.1 크레이트 구조

```
goose (workspace)
├── crates/
│   ├── goose/          # 핵심 라이브러리 (Agent, Session, Recipe, Provider 등)
│   ├── goose-cli/      # CLI 바이너리 (REPL, 레시피 실행, 인터랙티브 세션)
│   ├── goose-server/   # HTTP 서버 (Axum 기반, Agent를 API로 노출)
│   ├── goose-mcp/      # MCP 통합 (도구 서버 연동)
│   └── goose-acp/      # Agent Control Protocol 구현 + proc macros
```

### 1.2 핵심 컴포넌트 관계

```
                    ┌──────────────────┐
                    │   goose-cli /    │
                    │   goose-server   │
                    │  (진입점)         │
                    └────────┬─────────┘
                             │ uses
                    ┌────────▼─────────┐
                    │      Agent       │
                    │  (오케스트레이터) │
                    └──┬────┬────┬─────┘
          ┌────────────┘    │    └────────────┐
          ▼                 ▼                  ▼
   ┌─────────────┐  ┌────────────┐  ┌─────────────────┐
   │  Provider    │  │ Extension  │  │  Conversation   │
   │ (LLM 호출)  │  │  Manager   │  │  (메시지 이력)  │
   └─────────────┘  │ (MCP 도구) │  └────────┬────────┘
                     └────────────┘           │
                                     ┌───────▼────────┐
                                     │ SessionManager │
                                     │   (SQLite)     │
                                     └────────────────┘
```

### 1.3 동작 원리

1. **Provider**가 LLM에 메시지를 전송 (OpenAI, Anthropic, Databricks, AWS Bedrock 등 지원)
2. **Agent**가 Provider를 감싸고 Extension(도구)들을 관리
3. `Agent::reply()` 메서드가 세션과 메시지를 받아 **AgentEvent** 비동기 스트림을 반환
4. CLI의 **CliSession** 또는 서버 엔드포인트가 **Conversation** (`Vec<Message>`)을 유지하며 매 턴마다 `agent.reply()`를 호출
5. 스트림된 이벤트에는 텍스트 응답, 도구 요청, 도구 응답, 사용자 확인 요청 등이 포함

**핵심 루프**: LLM에 대화 전송 → 도구 호출 처리 → 결과를 대화에 추가 → 반복 (에이전트가 완료할 때까지)

---

## 2. Goosetown 개요

[block/goosetown](https://github.com/block/goosetown)은 **다중 에이전트 조정(multi-agent coordination) 프레임워크**다. 여러 Goose 인스턴스("delegate")를 병렬로 오케스트레이션한다.

### 2.1 핵심 개념

- **Goose** = 단일 에이전트 AI 도구
- **Goosetown** = Goose 위에 올라가는 조정 계층 → 여러 Goose 에이전트가 협업

### 2.2 워크플로우 (5단계)

```
Research → Process Results → Plan & Dispatch → Review → Synthesize
```

1. **Research**: 리서처 delegate들이 병렬로 정보 수집
2. **Process Results**: 결과 통합 및 모순 해결
3. **Plan & Dispatch**: 작업 분해 후 워커 delegate에 배분
4. **Review**: 산출물 검토
5. **Synthesize**: 최종 결과 합성

### 2.3 주요 컴포넌트

| 컴포넌트 | 설명 |
|---|---|
| **Orchestrator** | 중앙 조정자. delegate 생성, 결과 통합, 모순 해결. 직접 산출물을 만들지 않음 |
| **12가지 Delegate 역할** | 8종 리서처(로컬, GitHub, Reddit, StackOverflow, ArXiv, Jira, Slack, Beads) + 워커, 라이터, 리뷰어 |
| **Gtwall ("Town Wall")** | 파일 기반 append-only 브로드캐스트 로그. delegate 간 유일한 통신 채널 |
| **Telepathy** | 오케스트레이터→delegate 단방향 푸시 채널 (긴급 에스컬레이션용) |
| **Flocks** | 3명 이상 delegate가 gtwall을 통해 명시적으로 협업하는 단위 |
| **Dashboard** | 웹 UI (포트 4242-4300), 실시간 에이전트 활동 모니터링 |
| **Beads (`bd` CLI)** | 통합 이슈 트래커 |

### 2.4 안전장치

- 리서처 타임아웃: 5-10분, 워커 타임아웃: 10-15분
- 태스크당 최대 ~8-10 delegate
- 그레이스풀 디그레이데이션: 4명 중 3명만 완료해도 진행
- 증분 파일 쓰기 필수 (delegate 취소 시에도 작업 보존)

---

## 3. Recipe (레시피) 상세

### 3.1 레시피란?

레시피는 **재사용 가능하고 공유 가능한 AI 워크플로우 정의**다. YAML 또는 JSON 파일로 저장되며, 지시사항, 필요한 Extension/도구, 파라미터, 실행 설정을 하나의 포터블한 구성으로 패키징한다.

### 3.2 레시피 구조

```yaml
version: 1.0.0
title: "My Recipe"
description: "유용한 작업을 수행"
instructions: "{{param1}}을(를) 사용해서 X를 수행"
prompt: "시작 프롬프트"
extensions:
  - type: builtin
    name: developer
  - type: stdio
    name: custom-tool
    cmd: "npx"
    args: ["-y", "some-mcp-server"]
parameters:
  - name: param1
    type: string       # string 또는 file
    required: true
    default: "기본값"
    description: "입력 파라미터"
settings:
  provider: openai
  model: gpt-4
  temperature: 0.7
  max_turns: 10
sub_recipes:
  - path: ./sub-recipe.yaml
author:
  name: "작성자"
```

#### 주요 필드 설명

| 필드 | 설명 |
|---|---|
| `version` | 스키마 버전 |
| `title` / `description` | 사람이 읽을 수 있는 이름과 설명 |
| `instructions` | 에이전트에게 주는 상세 작업 지시 (Jinja2 템플릿, MiniJinja로 렌더링) |
| `prompt` | 초기 사용자 프롬프트 |
| `extensions` | 로드할 MCP 서버/빌트인 Extension 목록 (Stdio, Builtin, Platform, StreamableHttp, Frontend, InlinePython) |
| `settings` | 프로바이더, 모델, 온도, 최대 턴 수 등 |
| `parameters` | 파라미터 목록 (이름, 타입, 필수 여부, 기본값, 설명) |
| `sub_recipes` | 하위 레시피 참조 (복합 워크플로우 구성용) |

### 3.3 실행 흐름

```
1. Loading (로딩)
   └─ load_recipe_file()
      └─ CWD, 환경변수 경로, 라이브러리 디렉토리에서 .yaml/.json 검색

2. Building (빌드)
   └─ build_recipe_from_template()
      ├─ 템플릿 검증
      ├─ 파라미터 값 적용 (CLI 인수, 인터랙티브 프롬프트, 기본값)
      ├─ Jinja2 템플릿 렌더링 ({{param_name}})
      └─ 하위 레시피 경로 해석

3. Secret Discovery (시크릿 탐색)
   └─ discover_recipe_secrets()
      ├─ 필요한 자격증명 식별
      └─ 누락 시 인터랙티브 프롬프트 → 시스템 키링에 저장

4. Extension Setup (Extension 설정)
   └─ 레시피의 extensions → ExtensionConfig로 역직렬화
      └─ Stdio, Builtin, Platform, StreamableHttp, Frontend, InlinePython 지원

5. Execution (실행)
   ├─ instructions/prompt → 초기 사용자 메시지로 변환
   ├─ extensions → Agent에 로드
   ├─ settings → Provider 구성
   └─ agent.reply()를 통한 일반 세션 실행과 동일하게 동작
```

### 3.4 고급 기능

#### Sub-Recipes (하위 레시피)
- 다른 레시피 파일을 참조하여 워크플로우 합성
- 컨텍스트 기반 파라미터로 하위 레시피 간 데이터 전달
- 각 하위 레시피는 독립적으로 테스트 가능
- `subagent_execution_tool`과 `TaskConfig`를 통해 자식 Agent로 실행

#### Retry Logic (재시도 로직)
- `RetryConfig` + `SuccessCheck` (셸 명령 검증)
- 성공 조건 충족까지 자동 재시도
- 테스트 스위트, 시스템 운영에 유용

#### Scheduling (스케줄링)
- Cron 기반 예약 실행
- `goose schedule add --cron "0 0 9 * * *" --recipe-source ./recipe.yaml`
- 5, 6, 7자리 cron 표현식 지원

#### 세션에서 레시피 생성
- `agent.create_recipe()`로 현재 세션을 재사용 가능한 레시피로 변환

### 3.5 CLI 명령어

```bash
goose run --recipe <FILE>           # 레시피 실행
goose recipe open <FILE>            # 데스크톱 앱에서 열기
goose schedule add --recipe-source  # 스케줄 등록
```

---

## 4. Rust 라이브러리로서의 멀티턴 세션 사용

### 4.1 가능 여부: **Yes**

`goose` 크레이트는 명시적으로 라이브러리로 설계되어 있다. `lib.rs`에서 27개의 public 모듈을 export하며, `/crates/goose/examples/agent.rs`에 프로그래밍적 사용 예제가 있다.

### 4.2 기본 사용 패턴

```rust
use goose::agents::Agent;
use goose::agents::AgentConfig;
use goose::providers::ProviderFactory;
use goose::conversation::Message;
use goose::session::SessionManager;
use futures::StreamExt;

// 1. Provider 생성
let provider = ProviderFactory::create_with_named_model("openai", "gpt-4").await?;

// 2. Agent 인스턴스 생성
let config = AgentConfig::default();
let mut agent = Agent::new(config);
agent.update_provider(provider);

// 3. Extension 추가 (선택)
agent.add_extension(ExtensionConfig::stdio("developer", "goose-mcp", vec![])).await?;

// 4. 세션 생성
let session_manager = SessionManager::new().await?;
let session = session_manager.create_session().await?;

// 5. 첫 번째 턴
let msg1 = Message::user().with_text("파일 구조를 분석해줘");
let mut stream = agent.reply(&session.id, vec![msg1]).await;
while let Some(event) = stream.next().await {
    // AgentEvent 처리 (텍스트, 도구 호출 등)
}

// 6. 두 번째 턴 (멀티턴!)
let msg2 = Message::user().with_text("이제 테스트를 작성해줘");
let mut stream = agent.reply(&session.id, vec![msg2]).await;
while let Some(event) = stream.next().await {
    // AgentEvent 처리
}

// N번째 턴까지 반복 가능
```

### 4.3 핵심 타입

| 타입 | 설명 |
|---|---|
| `Agent` + `AgentConfig` | 메인 오케스트레이터 |
| `AgentEvent` | 스트리밍 응답 이벤트 (Message, 도구 호출 등) |
| `Conversation` / `Message` / `MessageContent` | 메시지 타입 (역할: User/Assistant, 내용: Text, ToolRequest, ToolResponse, Image, Thinking) |
| `SessionManager` / `Session` | 영속성 계층 (SQLite) |
| `ExtensionConfig` | 도구/MCP 서버 설정 |
| `ProviderFactory` | LLM Provider 생성 |

### 4.4 세션 영속성

**SessionManager**가 SQLite 기반 영속성을 제공한다:

```rust
// 메시지 개별 저장
session_manager.add_message(session_id, message).await?;

// 전체 대화 이력 로드
let session = session_manager.get_session(id, true /* include_messages */).await?;

// 빌더 패턴으로 세션 메타데이터 업데이트
session_manager.update(session_id)
    .total_tokens(Some(100))
    .apply()
    .await?;
```

세션 타입: User, Scheduled, SubAgent, Hidden, Terminal, Gateway

### 4.5 레시피를 멀티턴으로 확장 가능한가?

레시피 자체는 **단일 실행 단위**로 설계되어 있다 (instructions + prompt → agent 실행 → 완료). 하지만:

1. **레시피로 Agent를 구성한 뒤, 추가 턴을 프로그래밍적으로 보내는 것은 가능하다**: 레시피는 결국 Agent + Extension + Settings를 설정하는 선언적 구성이므로, 레시피로 초기화한 Agent에 대해 `reply()`를 반복 호출하면 멀티턴 세션이 된다.

2. **Sub-Recipe를 통한 다단계 실행**: 하나의 레시피가 여러 하위 레시피를 순차/병렬로 실행할 수 있으며, 각 단계 간 컨텍스트 전달이 가능하다.

3. **Retry 메커니즘**: `SuccessCheck`와 결합하면 성공할 때까지 반복 실행하는 패턴도 가능하다.

---

## 5. 주요 의존성

| 라이브러리 | 용도 |
|---|---|
| Tokio | 비동기 런타임 |
| Axum | 웹 서버 (goose-server) |
| SQLx + SQLite | 세션/메시지 영속성 |
| MiniJinja | 레시피 템플릿 렌더링 |
| tree-sitter | 코드 파싱 (9개 언어 지원) |
| tiktoken-rs | 토큰화 |
| keyring | 시크릿 저장 |
| Candle (optional) | CUDA/Metal 로컬 모델 추론 |

---

## 6. 요약

| 항목 | Goose | Goosetown |
|---|---|---|
| **목적** | 단일 AI 에이전트 프레임워크 | 다중 Goose 에이전트 조정 |
| **언어** | Rust (57%) + TypeScript (34%) | Shell + Python + JS |
| **핵심** | Agent + MCP + Provider | Orchestrator + Delegates + Gtwall |
| **실행** | CLI / HTTP API / Rust 라이브러리 | 셸 스크립트 + 대시보드 |
| **확장** | Extension (MCP 서버) | Delegate 역할 (12종) |
| **레시피** | YAML/JSON 워크플로우 정의 | N/A (Goose 레시피를 활용) |
