# 참조: 실행 — 프로덕션 시스템, 격리, 비교 참조

> 에이전트 실행 파이프라인, 격리 패턴, 다른 접근법에서 배우는 것.

---

## 1. Stripe Minions

**무엇인가:** 주당 1,300+ PR. Goose 포크 기반.

**핵심 통찰:** "모델이 시스템을 운영하는 게 아니다. 시스템이 모델을 운영한다."

**채택한 패턴:**

- **블루프린트 패턴** — 결정론적 노드 (git 연산, 린팅, PR 생성)와 에이전트 노드 (LLM 추론) 교차. 결정론적 노드는 토큰을 절약하고 예측 가능. **v0.2:** Rig의 `execute()` 메서드가 곧 블루프린트.
- **컨텍스트 사전 수집** — 에이전트 루프 전 결정론적으로 AGENTS.md 읽기, 이슈 fetch, 코드 사전 로드. **v0.2:** `ContextHydrator` 미들웨어.
- **큐레이션된 도구 세트 (Toolshed)** — ~500개 도구 중 에이전트당 ~15개만 노출. **v0.2:** Goose Recipe의 rig별 extension 설정.
- **제한된 CI 루프** — 최대 2라운드, 이후 needs-human-review. **v0.2:** `BoundedRetry` 미들웨어.

**고유 패턴:** 디렉토리별 범위 지정 규칙 파일 (Cursor 포맷), 사전 워밍 Devbox (10초 스핀업).

---

## 2. Ramp Inspect

**무엇인가:** 전체 머지 PR의 ~30%. Modal 샌드박스.

**핵심 통찰:** 전체 컨텍스트 샌드박스 = 에이전트가 인간 엔지니어와 같은 도구를 가짐.

**채택한 패턴:**

- **샌드박스 격리** — 각 세션이 완전 격리된 Modal 샌드박스. 파일시스템 스냅샷 사전 워밍. **v0.2:** rig별 Git worktree (로컬). 향후 `SandboxBackend` 트레잇.

**고유 패턴:** 30분마다 파일시스템 스냅샷 재구축, 사용자 타이핑 시 워밍 프리로딩, 전후 스크린샷 비교.

---

## 3. Coinbase Cloudbot

**무엇인가:** 엔터프라이즈 프레임워크.

**핵심 통찰:** 관찰 가능성과 감사 가능성은 경성 요구사항.

**고유 패턴:** 코드 우선 그래프 아키텍처, 결정론적 노드는 유닛 테스트 / LLM 노드는 eval 하니스, 모든 도구 호출 추적 + diff.

---

## 4. Open SWE (langchain-ai)

**무엇인가:** Stripe/Ramp/Coinbase의 프로덕션 패턴을 재사용 가능한 오픈소스 프레임워크로 체계화.

**채택한 패턴:**

- **미들웨어 훅 (Deep Agents 패턴)** — 결정론적 로직 주입을 위한 수명주기 지점:
  ```
  before_agent(state)           -- 1회 초기화
  wrap_model_call(fn, state)    -- 각 LLM 호출 감싸기
  before_tool_call(call, state) -- 도구 실행 전
  after_tool_call(call, msg, state) -- 도구 실행 후
  ```
  + `@before_model` (메시지 큐 주입) + `@after_agent` (안전망).
  **v0.2:** `on_start()`, `pre_hydrate()`, `post_execute()`가 있는 `Middleware` 트레잇. Deep Agents보다 단순 (Goose가 내부적으로 호출별 래핑을 처리).

- **안전망 PR** — `@after_agent` 훅이 커밋되지 않은 변경사항 감지 → 자동 브랜치 + 커밋 + PR. **v0.2:** `SafetyNet` 미들웨어.

---

## 5. Portless (vercel-labs/portless)

**무엇인가:** 포트 번호를 네임드 URL로 대체하는 로컬 프록시. 단일 바이너리.

**핵심 메커니즘:**

### 포트 할당 (감지가 아닌 제어)
```
portless myapp "npm run dev"
  │
  ├─ 1. 빈 포트 찾기: 4000-4999 범위
  │     랜덤 50회 시도 → 실패 시 순차 스캔
  │     net.Server 임시 바인딩으로 확인
  │     (TOCTOU race 인지 — 랜덤 우선으로 최소화)
  │
  ├─ 2. child process 실행 + 환경변수 주입:
  │     PORT=4237 HOST=127.0.0.1
  │     PORTLESS_URL=http://myapp.localhost:1355
  │     (Vite 등 PORT 무시 프레임워크 → --port --host 자동 주입)
  │
  ├─ 3. routes.json에 등록:
  │     { hostname: "myapp.localhost", port: 4237, pid: 12345 }
  │     (파일 기반 잠금: mkdir로 lock, 20회 재시도, 50ms 간격)
  │     (stale lock: 10초 초과 시 강제 삭제)
  │
  └─ 4. 프록시 라우팅:
       Host 헤더 → 서브도메인 추출 (HTTP/2: :authority, HTTP/1.1: Host)
       1차: 정확한 hostname 매칭
       2차: 와일드카드 suffix (tenant.myapp.localhost → myapp.localhost)
       → 127.0.0.1:{port}로 프록시
```

### Worktree 자동 감지
git branch 이름을 서브도메인 접두사로:
- main: `myapp.localhost:1355`
- fix-ui 브랜치: `fix-ui.myapp.localhost:1355`

### Stale 라우트 정리
`loadRoutes()` 시 각 라우트의 PID 생존 확인 (`process.kill(pid, 0)`). 죽은 프로세스 → 라우트 자동 삭제.

### 쿠키/스토리지 격리
서브도메인별 쿠키 저장소 + localStorage 범위. 브랜치 간 세션 누출 없음.

**v0.2 적용:**
- 네임드 URL 철학: `{rig-id}.{project}.localhost`
- Worktree + 서브도메인 자동 매핑
- 포트 충돌 구조적 불가능
- **차이점:** OpenGoose는 child process를 만들지 않으므로 Portless처럼 PORT를 주입하며 spawn할 수 없음. Phase 5에서 Portless 방식 (할당+주입) vs VS Code 방식 (런타임 감지) 중 선택 필요.

**생략하는 것:** Portless 바이너리 직접 사용, Turborepo/Next.js 특화, HTTPS 인증서 자동 생성.

---

## 6. AntFarm (snarktank/antfarm)

**무엇인가:** OpenClaw 위에서 돌아가는 멀티에이전트 파이프라인 오케스트레이터. YAML 워크플로우 정의 → 에이전트 순차 실행. 48시간 연속 실행, 3개 레포에서 33개 스토리 자율 계획+구현.

**v0.2 적용:**
- **`progress.txt` 패턴** → Flight Record 설계의 참조. 에이전트가 자유 형식으로 학습을 기록하는 가장 단순한 형태
- **Ralph 루프 (깨끗한 컨텍스트)** → Gas Town의 세션 순환과 동일 원칙. Rig의 세션 컴팩션 정책 설계 시 참조

**가져오지 않는 것:** 순차 파이프라인 모델 (pull이 아님), OpenClaw 의존성, YAML 워크플로우 정의.

---

## 7. Fractals (TinyAGI/fractals)

**무엇인가:** 재귀적 작업 분해 + 병렬 실행 엔진. ~500줄 TypeScript.

**v0.2 적용:**
- **Classify-before-decompose 게이트** → Rig가 `board__create_task`로 하위 작업을 만들 때 "정말 분해가 필요한가?" 검증. 과도한 분해(토큰 낭비) 방지
- **Lineage-as-context** → 하위 작업의 전체 조상 체인을 `pre_hydrate()`에 포함. 저비용 고효과
- **형제 인식 프롬프트** → "너는 병렬로 작업하는 여러 에이전트 중 하나다. 형제 작업과 중복하지 마라"

**가져오지 않는 것:** Push 모델 (top-down 오케스트레이션), 사전 확정 트리, 머지/의존성/실패 처리 없음.

---

## 8. Agent Orchestrator (ComposioHQ/agent-orchestrator)

**무엇인가:** 병렬 AI 코딩 에이전트 오케스트레이터. 4.6k stars. 8개 플러그인 슬롯, 16-상태 세션 머신.

**v0.2 적용:**
- **에스컬레이션 체인 with 타임아웃** → Witness/BoundedRetry 확장: 이벤트 유형별 자동 처리 횟수, 타임아웃 후 에스컬레이션 규칙
- **Wakeup 병합** → Rig가 바쁠 때 여러 알림을 중복 제거하고 하나로 합침
- **Orchestrator-as-Agent** → L3 rig가 board 관리 도구만 가지고 (코딩 도구 없이) 조율하는 recipe 패턴

**가져오지 않는 것:** 중앙 push 모델, tmux send-keys 통신, 상태 없는 flat files.

---

## 9. Paperclip (paperclipai/paperclip)

**무엇인가:** "제로-인간 회사"를 위한 컨트롤 플레인. 28k stars.

**v0.2 적용:**
- **세션 컴팩션 정책** → Rig의 Goose 세션이 컨텍스트 한계에 도달할 때: 핸드오프 요약 생성 → 새 세션
- **비용 추적 + 빌링 코드** → Rig가 위임할 때 비용이 부모 작업에 롤업. L3의 blast radius 제한에 비용 차원 추가
- **메모리 어댑터 계약** → `write/query/get/forget` 인터페이스 패턴
- **Atomic checkout with 409** → Board의 claim 실패 시 "재시도하지 말고 다른 작업 선택" 프로토콜

**가져오지 않는 것:** 조직도 계층, 서버 중심 아키텍처, 거버넌스 게이트, PostgreSQL 의존성.
