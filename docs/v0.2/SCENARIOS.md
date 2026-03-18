# OpenGoose v0.2 — 사용 시나리오

> v0.2가 완성되었을 때 사용자가 경험하는 흐름.

---

## 시나리오 1: 간단한 대화

사용자가 질문 하나를 던진다.

```
$ opengoose

  ┌─ OpenGoose v0.2 ──────────────────────────────────────┐
  │  Board: 0 open · 0 claimed · 0 done                   │
  │  Rigs:  main (L1, idle)                                │
  └────────────────────────────────────────────────────────┘

> auth 모듈에서 JWT 토큰 만료 처리가 어떻게 돼 있어?
```

**내부에서 일어나는 일:**

```
사용자 입력
  │
  ▼
┌─────────────────────────────┐
│ CLI: Board.post(work_item)  │  ← 작업 항목으로 변환, 보드에 게시
│   session: cli-001          │
│   content: "auth 모듈에서…" │
└──────────┬──────────────────┘
           │
           ▼
┌─────────────────────────────┐
│ Board: work_item open       │  ← 보드에 등록됨
└──────────┬──────────────────┘
           │
           ▼
┌─────────────────────────────┐
│ Rig "main" (대기 중)        │
│   board.wait_for_claimable()│
│   → 새 항목 발견!           │
│   → board.claim()           │  ← 자동으로 가져감
└──────────┬──────────────────┘
           │
           ▼
┌─────────────────────────────┐
│ Goose Agent::reply()        │  ← LLM 호출 (Goose가 처리)
│   코드 검색, 파일 읽기…     │
│   스트리밍 응답 생성         │
└──────────┬──────────────────┘
           │
           ▼
┌─────────────────────────────┐
│ CLI: 토큰 단위 스트리밍 출력│
└─────────────────────────────┘
```

사용자가 보는 것:

```
> auth 모듈에서 JWT 토큰 만료 처리가 어떻게 돼 있어?

src/auth/jwt.rs의 validate_token() 함수에서 처리합니다.

1. 토큰 디코딩 후 exp 클레임 확인
2. 현재 시간과 비교하여 만료 여부 판단
3. 만료 시 AuthError::TokenExpired 반환
4. refresh_token이 있으면 자동 갱신 시도
  …
```

**핵심: 사용자 입장에서는 그냥 채팅처럼 보인다. 하지만 내부적으로 모든 것이 Board를 통과.**

---

## 시나리오 2: 코드 작업 (단일 rig)

사용자가 구체적인 코드 작업을 요청한다.

```
> /task "rate limiting 미들웨어를 추가해줘. 분당 100 요청 제한."
```

**내부 흐름 — 블루프린트 패턴:**

```
┌─ [결정론적] 컨텍스트 사전 수집 ─────────────────────────┐
│  · AGENTS.md 읽기                                        │
│  · src/middleware/ 디렉토리 스캔                          │
│  · 기존 미들웨어 패턴 파악                               │
│  · Cargo.toml 의존성 확인                                │
│  (LLM 호출 없음 — 도구 실행만, 토큰 절약)               │
└──────────────────────────┬──────────────────────────────┘
                           ▼
┌─ [결정론적] Git worktree 생성 ──────────────────────────┐
│  $ git worktree add /tmp/og-rigs/main/#1           │
│    -b rig/main/#1                                   │
│  · PORT=4237 (랜덤 할당)                                 │
│  · OPENGOOSE_URL=http://main.localhost:4237              │
└──────────────────────────┬──────────────────────────────┘
                           ▼
┌─ [에이전트] 구현 ───────────────────────────────────────┐
│  Goose Agent::reply() — LLM이 자율적으로:               │
│  · rate_limiter.rs 작성                                  │
│  · middleware chain에 등록                               │
│  · 테스트 작성                                           │
│  (worktree 안에서 작업, main에 영향 없음)               │
└──────────────────────────┬──────────────────────────────┘
                           ▼
┌─ [결정론적] 검증 ───────────────────────────────────────┐
│  $ cargo clippy          → 통과 ✓                        │
│  $ cargo test            → 2 실패 ✗                      │
└──────────────────────────┬──────────────────────────────┘
                           ▼
┌─ [에이전트] 수정 (1/2 라운드) ──────────────────────────┐
│  Goose Agent::reply() — 테스트 실패 내용 보고           │
│  · 테스트 수정                                           │
└──────────────────────────┬──────────────────────────────┘
                           ▼
┌─ [결정론적] 재검증 ─────────────────────────────────────┐
│  $ cargo clippy          → 통과 ✓                        │
│  $ cargo test            → 전부 통과 ✓                   │
└──────────────────────────┬──────────────────────────────┘
                           ▼
┌─ [결정론적] 완료 ───────────────────────────────────────┐
│  · git add + commit                                      │
│  · board.submit(result)                                  │
│  · board.merge(branch, main)                             │
│  · git worktree remove                                   │
└─────────────────────────────────────────────────────────┘
```

사용자가 보는 것:

```
> /task "rate limiting 미들웨어를 추가해줘. 분당 100 요청 제한."

● #1 "rate limiting 미들웨어 추가" — main rig가 작업 중...

  [사전 수집] 기존 미들웨어 패턴 분석 완료
  [worktree] rig/main/#1 생성

  src/middleware/rate_limiter.rs 작성 중...
  src/middleware/mod.rs 수정 중...
  tests/middleware/rate_limiter_test.rs 작성 중...

  [검증] clippy 통과, 테스트 2개 실패
  [수정] 테스트 수정 중...
  [재검증] clippy 통과, 테스트 전부 통과

✓ #1 완료 — 커밋 a3f8e21, worktree 정리됨

>
```

---

## 시나리오 3: 복수 rig 협업

연구자와 개발자 rig가 등록되어 있다.

```
$ opengoose rigs
  main        L1   idle     (기본)
  researcher  L2   idle     (연구 전문)
  developer   L2   idle     (구현 전문)

> /task "Rust에서 쓸 수 있는 rate limiting 라이브러리를 조사하고, 가장 적합한 걸로 구현해줘"
```

**내부 흐름:**

```
┌─ 사용자 ─────────────────────────────────────────────────┐
│  /task "Rust rate limiting 라이브러리 조사 + 구현"        │
└──────────────────────────┬───────────────────────────────┘
                           ▼
┌─ Board ──────────────────────────────────────────────────┐
│  #2 [open] "rate limiting 라이브러리 조사 + 구현"   │
└──────────────────────────┬───────────────────────────────┘
                           │
            ┌──────────────┘
            ▼
┌─ Rig "researcher" ──────────────────────────────────────┐
│  태그 매칭: work_item.tags=[] → 아무 rig나 claim 가능   │
│  researcher가 먼저 board.wait_for_claimable() 반환       │
│  board.claim(#2)                                    │
│                                                          │
│  Goose Agent::reply():                                   │
│  · governor, ratelimit, leaky-bucket 등 조사             │
│  · 비교 분석 작성                                        │
│  · "governor가 가장 적합" 결론                           │
│                                                          │
│  조사 완료 → 구현 하위 작업 생성:                        │
│  board__create_task("governor로 rate limiting 구현")     │
│  board.submit(조사 결과)                                 │
└──────────────────────────┬───────────────────────────────┘
                           ▼
┌─ Board ──────────────────────────────────────────────────┐
│  #2 [done]  "조사 완료 — governor 추천"             │
│  #3 [open]  "governor로 rate limiting 구현"  ← 새로 │
└──────────────────────────┬───────────────────────────────┘
                           │
            ┌──────────────┘
            ▼
┌─ Rig "developer" ───────────────────────────────────────┐
│  #3.tags=[] → 아무 rig나 claim 가능                │
│  researcher가 #2 작업 중 → developer가 먼저 claim  │
│  board.claim(#3)                                    │
│                                                          │
│  [결정론적] worktree 생성                                │
│  [에이전트] governor 의존성 추가 + 구현 + 테스트         │
│  [결정론적] clippy + test → 통과                         │
│  [결정론적] 커밋 + merge                                 │
│                                                          │
│  board.submit(구현 결과)                                 │
└─────────────────────────────────────────────────────────┘
```

사용자가 보는 것:

```
> /task "Rust에서 쓸 수 있는 rate limiting 라이브러리를 조사하고, 가장 적합한 걸로 구현해줘"

● #2 "rate limiting 라이브러리 조사" — researcher가 작업 중...

  governor (v0.6) — 토큰 버킷, 매우 활발한 유지보수
  ratelimit (v0.9) — 간단하지만 async 미지원
  leaky-bucket (v1.1) — async 지원, 하지만 기능 적음

  → 추천: governor (async, 유연한 설정, 활발한 커뮤니티)

✓ #2 조사 완료

● #3 "governor로 rate limiting 구현" — developer가 작업 중...

  Cargo.toml에 governor 추가 중...
  src/middleware/rate_limiter.rs 작성 중...
  테스트 작성 중...
  clippy 통과, 테스트 통과

✓ #3 구현 완료 — 커밋 b7c9d32

>
```

---

## 시나리오 4: 헤드리스 모드 (CI/자동화)

```bash
# 단일 작업 실행 후 종료
$ opengoose run "auth.rs의 실패하는 테스트를 고쳐줘"

● #4 "auth.rs 테스트 수정" — main rig 작업 중...
  [사전 수집] cargo test 실행, 실패 내용 수집
  [에이전트] validate_token 테스트 수정
  [검증] cargo test — 전부 통과
  [커밋] fix: validate_token test for expired tokens (a1b2c3d)
✓ 완료
$

# 특정 recipe로 실행
$ opengoose run --recipe researcher "2024년 이후 Rust async 런타임 벤치마크 정리"

● #5 "Rust async 런타임 벤치마크" — researcher 작업 중...
  tokio vs async-std vs smol vs glommio 비교...
  …결과 정리 완료
✓ 완료 — 결과가 stdout에 출력됨
$
```

---

## 시나리오 5: 보드 상태 확인

```
> /board

  ┌─ Wanted Board ────────────────────────────────────────┐
  │                                                        │
  │  Open: 2    Claimed: 1    Done: 12    Rigs: 3          │
  │                                                        │
  │  ── Rigs ──────────────────────────────────────────    │
  │  main        L1.5  idle     —                          │
  │  researcher  L2    working  #6 "API 스펙 조사"    │
  │  developer   L2    idle     —                          │
  │                                                        │
  │  ── Open ──────────────────────────────────────────    │
  │  ○ #7  P1  "에러 핸들링 리팩토링"                 │
  │  ○ #8  P2  "README 업데이트"                      │
  │                                                        │
  │  ── In Progress ───────────────────────────────────    │
  │  ● #6  researcher  "API 스펙 조사"  2m 30s        │
  │                                                        │
  │  ── Recent ────────────────────────────────────────    │
  │  ✓ #1  "rate limiting 구현"          5m ago       │
  │  ✓ #2  "rate limiting 조사"          8m ago       │
  │  ✓ #3  "JWT 만료 처리 수정"          15m ago      │
  └────────────────────────────────────────────────────────┘

> /status

  ┌─ Rig Status ──────────────────────────────────────────┐
  │                                                        │
  │  researcher  L2  trust: ██████████░░ 12pts             │
  │    완료: 7건  stamp 평균: q:0.7 r:0.8 h:0.6           │
  │    현재: #6 "API 스펙 조사" (2m 30s)              │
  │                                                        │
  │  developer   L2  trust: ██████████░░ 15pts             │
  │    완료: 5건  stamp 평균: q:0.8 r:0.9 h:0.7           │
  │    현재: idle                                          │
  │                                                        │
  │  main        L1.5  trust: ███░░░░░░░░ 4pts             │
  │    완료: 3건  stamp 평균: q:0.6 r:0.7 h:0.5           │
  │    현재: idle                                          │
  └────────────────────────────────────────────────────────┘
```

---

## 시나리오 6: 신뢰 시스템 동작

시간이 지나면서 rig의 신뢰가 쌓인다.

```
시간 흐름:

Day 1:  researcher 생성 (L1, 0pts)
        ├─ 작업 3개 완료, developer가 /stamp (q:0.8 r:0.7 h:0.5 leaf 등)
        └─ 3pts → L1.5 승급 (하위 작업 생성 가능)

Day 3:  researcher (L1.5, 5pts)
        ├─ 작업 4개 더 완료, stamp 누적
        └─ 12pts → L2 승급 (동료에게 위임 가능)

Day 7:  researcher (L2, 28pts)
        ├─ 고난도 작업(root) 2개 완료
        └─ 38pts → L2.5 (최상위 작업 생성 가능)

Day 14: researcher (L2.5, 55pts)
        └─ 55pts → L3 승급 (다른 rig의 작업을 stamp 가능!)

졸업앨범 규칙: researcher는 자기 작업을 stamp할 수 없다.
               developer가 researcher의 작업을 평가하고,
               researcher가 developer의 작업을 평가한다.
```

---

## 전체 그림: 한 장으로

```
┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│    사용자                                                       │
│      │                                                          │
│      │ CLI (대화/명령/헤드리스)                                  │
│      ▼                                                          │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │                    Wanted Board                           │   │
│  │                                                           │   │
│  │   ○ open    ● claimed    ✓ done                          │   │
│  │                                                           │   │
│  │   게시 ─────────→ claim ──────→ submit ──→ merge         │   │
│  │     ↑                ↑              │                     │   │
│  │     │                │              ▼                     │   │
│  │     │           ┌────┴────┐    ┌─────────┐               │   │
│  │     │           │ Branch  │    │ 3-way   │               │   │
│  │     │           │ (CoW)   │    │ merge   │               │   │
│  │     │           └─────────┘    └─────────┘               │   │
│  │     │                                                     │   │
│  │   Stamps ←── 완료 시 다른 rig가 평가                     │   │
│  │   Trust  ←── stamp 누적 → 자동 승급 (L1→L3)             │   │
│  └──────────────────────────────────────────────────────────┘   │
│           ↕              ↕              ↕                        │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐            │
│  │ Rig: main    │ │ Rig: research│ │ Rig: develop │            │
│  │              │ │              │ │              │            │
│  │ Goose Agent  │ │ Goose Agent  │ │ Goose Agent  │            │
│  │ (reply+MCP)  │ │ (reply+MCP)  │ │ (reply+MCP)  │            │
│  │              │ │              │ │              │            │
│  │ board__*     │ │ board__*     │ │ board__*     │            │
│  │ (내장 도구)  │ │ (내장 도구)  │ │ (내장 도구)  │            │
│  │              │ │              │ │  ┌────────┐  │            │
│  │              │ │              │ │  │worktree│  │            │
│  │              │ │              │ │  │(격리)  │  │            │
│  │              │ │              │ │  └────────┘  │            │
│  └──────────────┘ └──────────────┘ └──────────────┘            │
│                                                                 │
│              모두 단일 바이너리, 단일 프로세스                   │
└─────────────────────────────────────────────────────────────────┘
```

---

## 시나리오 7: 실패 및 복구

### 7.1 CI 실패 (최대 2라운드)

```
> /task "deprecated API 마이그레이션"

● #9 "deprecated API 마이그레이션" — developer 작업 중...

  src/api/legacy.rs → src/api/v2.rs 마이그레이션 중...
  
  [검증] cargo test — 3 실패
  [수정 1/2] 테스트 수정 중...
  [재검증] cargo test — 1 실패
  [수정 2/2] 추가 수정 중...
  [재검증] cargo test — 1 실패 (동일)

⚠ #9 needs-human-review — CI 2라운드 초과
  실패 내용: test_legacy_compat assertion failed
  worktree 유지됨: /tmp/og-rigs/developer/#9
  
  다음 단계:
  · `cd /tmp/og-rigs/developer/#9` 후 수동 수정
  · 또는 `/retry #9` (추가 2라운드 허용)
  · 또는 `/abandon #9` (worktree 삭제)

>
```

### 7.2 Rig Stuck/Zombie

```
$ opengoose

  ┌─ OpenGoose v0.2 ──────────────────────────────────────┐
  │  Board: 1 open · 1 claimed · 5 done                   │
  │  Rigs:  main (L1, idle)                                │
  │         researcher (L2, stuck ⚠)                       │
  │         developer (L2, idle)                           │
  └────────────────────────────────────────────────────────┘

> /status

  ┌─ Rig Status ──────────────────────────────────────────┐
  │                                                        │
  │  researcher  L2  ⚠ STUCK                               │
  │    작업: #10 "대규모 리팩토링" (45분 경과)          │
  │    사유: 타임아웃 (30분 초과)                          │
  │    대응: needs-human-review로 표시, respawn 예정       │
  │                                                        │
  │  developer   L2  idle                                  │
  │  main        L1  idle                                  │
  └────────────────────────────────────────────────────────┘

[Witness] researcher stuck 감지 → respawn 중...
[Witness] #10 → needs-human-review

>
```

### 7.3 프로세스 충돌 후 복구

```bash
$ opengoose
# ... 사용 중 갑자기 충돌 ...
$ opengoose   # 재시작

  [복구] SQLite WAL에서 상태 복원 중...
  [복구] claimed 상태 작업 1건 → open으로 롤백
    · #11 "API 문서화" (developer가 작업 중이었음)
  [복구] 완료 — 정상 시작

  ┌─ OpenGoose v0.2 ──────────────────────────────────────┐
  │  Board: 2 open · 0 claimed · 5 done                   │
  │  Rigs:  main (L1, idle), researcher (L2, idle), ...   │
  └────────────────────────────────────────────────────────┘

>
```

---

## 시나리오 8: 세션 인계 (Rig 전환)

Rig A가 대화 중인데 코드 작업이 들어와서 busy 상태가 됨. 후속 메시지를 Rig B가 이어받는 경우.

```
> JWT 만료 처리가 어떻게 돼 있어?
  (main rig — session: cli-42, seq: 1 → claim, 응답)

> /task "refresh token 자동화 구현"
  (main rig — session: cli-42, seq: 2 → claim, worktree 작업 시작… busy)

> 그런데 방금 말한 validate_token 함수 좀 더 자세히 설명해줘
  (main rig가 busy → developer rig가 session: cli-42, seq: 3를 claim)
```

**내부에서 일어나는 일:**

```
┌─ Board ────────────────────────────────────────────────┐
│  seq: 3 도착                                            │
│  세션 친화성: main이 cli-42를 소유하지만 → Working 상태 │
│  → 다른 rig에게 열림                                    │
└──────────────────────────┬─────────────────────────────┘
                           ▼
┌─ Rig "developer" ──────────────────────────────────────┐
│  claim(seq: 3)                                          │
│                                                         │
│  ⚠ 대화 이력 단절 — Goose 세션이 다름                   │
│  → prime() 컨텍스트에 이전 대화 요약 포함:              │
│    "사용자가 JWT 만료 처리를 질문함.                     │
│     validate_token() 함수에 대해 논의 중."              │
│                                                         │
│  Goose Agent::reply() — 요약 기반으로 연속 답변 생성    │
└────────────────────────────────────────────────────────┘
```

사용자가 보는 것:

```
> 그런데 방금 말한 validate_token 함수 좀 더 자세히 설명해줘

  [developer rig가 응답 중 — main은 refresh token 작업 중]

  validate_token()은 src/auth/jwt.rs:42에 있습니다.
  …(이전 맥락을 바탕으로 답변)

  ℹ 참고: main rig가 작업 중이어서 developer가 대화를 이어받았습니다.
    맥락이 불완전할 수 있습니다.
```

**제약사항:** Goose 세션이 rig 인스턴스에 바인딩되므로 완벽한 이력 공유 불가. Phase 2에서 Goose 세션 fork/export API 사용 시 해결 가능.

---

## 시나리오 9: Stamp 거절 — 신뢰 하락과 제재

Rig의 결과물이 계속 나빠서 제재되는 경우.

```
시간 흐름:

Day 1:  developer-02 생성 (L1, 0pts)
        ├─ #1 완료 → /stamp q:0.8 r:0.7 h:0.5 leaf → +0.8 (quality)
        ├─ #2 완료 → /stamp q:0.6 r:0.8 h:0.6 leaf → +0.6 (quality)
        └─ 누적: 1.4pts → 아직 L1

Day 3:  developer-02 (L1, 1.6pts)
        ├─ #3 완료 → /stamp q:-0.5 r:-0.3 h:0.0 branch → -1.0
        ├─ #4 완료 → /retry "테스트 미작성" → 재작업
        │   └─ #4 재시도 완료 → /stamp q:-0.3 r:-0.2 h:0.1 branch → -0.6
        └─ 누적: 0.0pts

Day 5:  developer-02 (L1, -1.2pts — 감쇠 반영)
        ├─ #5 → 타임아웃 → needs-human-review
        ├─ /stamp q:-0.5 r:-1.0 h:-0.3 root → -4.0
        └─ 누적: -5.2pts → ⚠ 제재 발동!
```

사용자가 보는 것:

```
> /status

  ┌─ Rig Status ──────────────────────────────────────────┐
  │                                                        │
  │  developer-02  L1  ⚠ READ-ONLY (제재)                  │
  │    가중 점수: -5.2  (임계값: -5.0)                      │
  │    최근 stamp: reliability:-1.0 root (#5)          │
  │    대응: 쓰기 도구 사용 불가, 읽기/조사만 가능          │
  │                                                        │
  │  developer-01  L2  idle                                │
  │  main          L1  idle                                │
  └────────────────────────────────────────────────────────┘

  💡 developer-02를 복구하려면:
     · /stamp 으로 양호한 작업에 긍정 stamp을 누적
     · 또는 opengoose rigs remove developer-02 로 삭제
```

---

## 시나리오 10: 프로젝트 전환

대화 중 다른 프로젝트로 컨텍스트를 바꾸는 경우.

```
$ cd ~/dev/myapp && opengoose

> 현재 인증 구조 설명해줘
  (project: myapp — src/auth/ 읽고 답변)

> /project ~/dev/backend

  ℹ 프로젝트 전환: myapp → backend
    Board: 3 open · 0 claimed · 8 done

> /task "에러 핸들링 리팩토링"
  (project: backend — ~/dev/backend 에서 worktree 생성)
```

**내부 흐름:**

```
/project ~/dev/backend
  │
  ▼
CLI가 현재 세션의 프로젝트 컨텍스트를 갱신
  → 이후 생성되는 WorkItem.project = Some("backend")
  → Board: project="backend" 으로 필터링 조회
  → Rig의 working_dir = ~/dev/backend
```

**대화 세션은 유지된다.** 프로젝트만 바뀌고 Goose 세션은 동일. LLM이 이전 프로젝트(myapp)의 맥락도 기억하고 있으므로 교차 참조 질문도 가능:

```
> backend의 에러 핸들링을 myapp처럼 바꿀 수 있어?
  (LLM이 양쪽 맥락을 가지고 있음 — 다만 myapp 코드 접근은 별도 요청 필요)
```

---

## 요약: v0.2의 핵심 경험

| 관점 | 경험 |
|------|------|
| **사용자** | 그냥 채팅하면 된다. 복잡한 작업은 `/task`로. 나머지는 rig들이 알아서. |
| **rig** | 보드에서 자기에게 맞는 작업을 가져가서 실행. 다른 rig와 직접 대화 안 함 — 보드가 중개. |
| **보드** | 모든 것의 중심. 작업 게시, 분배, 격리(브랜치), 통합(머지), 신뢰(stamp). |
| **Goose** | 에이전트 루프만 담당. LLM 호출, 도구 실행, 세션, 에러 복구. 조율은 모름. |
| **운영** | 단일 바이너리 `opengoose` 하나로 전부 동작. 설치 → 실행 → 끝. |
| **실패** | CI 2라운드 제한, Witness가 stuck/zombie 감지, 충돌 시 WAL 복구. |
| **인계** | Rig busy 시 다른 rig가 세션 이어받음. 대화 요약으로 연속성 근사. |
| **제재** | 누적 stamp이 -5.0 이하 → read-only. 긍정 stamp으로 복구 가능. |
| **전환** | `/project`로 대화 중 프로젝트 전환. 세션 유지, 컨텍스트만 변경. |
