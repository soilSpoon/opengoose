# 코드 품질 평가 개선 — 설계 문서

> **날짜:** 2026-03-26
> **목표:** 코드 품질 및 아키텍처 평가에서 도출된 7개 개선 항목 해결
> **접근법:** 균형 (Approach B) + evolver 크레이트 추출

---

## 1. 배경

외부 평가에서 7개 개선 포인트가 도출됨:

| # | 이슈 | 심각도 |
|---|------|--------|
| 1 | opengoose 크레이트 비대 (11.4k LOC, 38%) | 낮음 |
| 2 | sandbox 크레이트 고아 — ARCHITECTURE.md 미언급 | 낮음 |
| 3 | 크레이트 수 불일치 — 문서 "4개", 실제 5개 | 낮음 |
| 4 | 통합 테스트 부족 — Board→Worker flow 없음 | 중간 |
| 5 | Doc-tests 0개 — 공개 API에 예제 없음 | 낮음 |
| 6 | Board 이중 책임 — SQLite + CowStore 한 struct | 낮음 |
| 7 | 런타임 에러 핸들링 — Worker 실패 시 전체 실패 | 낮음 |

**결정:**
- 항목 1: evolver를 별도 크레이트로 추출 (11.4k → ~8.9k LOC)
- 항목 6: Board 분리하지 않음 — merge→persist 원자성을 위해 단일 소유자 유지 (ADR-1)
- 나머지: 문서화 + 테스트 + 에러 핸들링

---

## 2. Evolver 크레이트 추출 (항목 1)

### 2.1 동기

`crates/opengoose/src/evolver/` (2,490 LOC)는 와이어링이 아닌 독립된 서브시스템:
- stamp 분석 → LLM 호출 → 스킬 생성/업데이트 파이프라인
- `loop_driver.rs`, `pipeline.rs`, `sweep.rs` — 자체 비즈니스 로직
- Board + Skills + Goose에 의존 — opengoose-rig와 동일한 의존 방향

### 2.2 의존성 해결

| 현재 의존 | 처리 |
|-----------|------|
| `opengoose_board::Board` | 그대로 유지 |
| `opengoose_rig::work_mode::evolve_session_id` | 그대로 유지 |
| `goose::agents::Agent` | 그대로 유지 |
| `crate::runtime::{AgentConfig, create_agent}` | `opengoose-rig`로 이동 (범용 Agent 생성 유틸) |
| `crate::skills::{evolve, load}` | `opengoose-skills` 직접 의존으로 교체 |
| `crate::skills::test_env_lock` | 새 크레이트 내부 test util로 이동 |

### 2.3 추출 후 의존성 그래프

```
opengoose-board           (독립)
       ↑
opengoose-rig             (board, goose)
       ↑
opengoose-evolver  [NEW]  (board, rig, skills, goose)
       ↑
opengoose                 (board, rig, skills, evolver)

opengoose-skills          (독립)
opengoose-sandbox         (독립, 실험적)
```

순환 없음. 바이너리 크레이트에서 `opengoose_evolver::run(board, stamp_notify)` 한 줄 호출.

### 2.4 새 크레이트 구조

```
crates/opengoose-evolver/
├── Cargo.toml
└── src/
    ├── lib.rs              # pub use loop_driver::run; + AgentCaller trait
    ├── loop_driver.rs      # stamp_notify 대기 + lazy Agent init
    ├── pipeline.rs         # stamp → LLM 분석 → 스킬 생성
    └── sweep.rs            # 주기적 미처리 stamp 스캔
```

### 2.5 바이너리 크레이트 변경

- `crates/opengoose/src/evolver/` 디렉토리 삭제
- `runtime.rs`에서 `opengoose_evolver::run(board, stamp_notify)` 호출
- `crates/opengoose/src/skills/` — evolver가 사용하던 re-export 정리

---

## 3. ARCHITECTURE.md 문서 수정 (항목 2, 3, 6)

### 3.1 크레이트 수 수정

`docs/v0.2/ARCHITECTURE.md` 18번 줄:

```
변경 전: 3. **4개 크레이트** — `opengoose`, `opengoose-board`, `opengoose-rig`, `opengoose-skills`.
변경 후: 3. **6개 크레이트** — `opengoose`, `opengoose-board`, `opengoose-rig`, `opengoose-skills`, `opengoose-evolver`, `opengoose-sandbox` (실험적).
```

### 3.2 크레이트 구조 섹션 (§3) — evolver + sandbox 추가

기존 트리에 추가:

```
│   ├── opengoose-evolver/               # Evolver — stamp 기반 스킬 자동 진화
│   │   └── src/
│   │       ├── lib.rs                   # AgentCaller trait, run() 진입점
│   │       ├── loop_driver.rs           # stamp_notify 대기 + lazy Agent init
│   │       ├── pipeline.rs              # stamp → LLM 분석 → 스킬 생성
│   │       └── sweep.rs                 # 주기적 미처리 stamp 스캔
│   │
│   └── opengoose-sandbox/               # 실험적 — microVM 샌드박스
│       └── src/
│           ├── hypervisor/              # HVF (Apple Hypervisor.framework)
│           ├── boot.rs                  # VM 부팅 시퀀스
│           ├── machine.rs              # VM 머신 설정
│           ├── pool.rs                 # VM 풀 관리
│           ├── snapshot.rs             # CoW 스냅샷
│           ├── vm.rs                   # VM 라이프사이클
│           ├── uart.rs                # 시리얼 콘솔
│           ├── virtio.rs             # VirtIO 장치
│           └── initramfs.rs          # initramfs 빌더
```

### 3.3 의존성 그래프 (§3.1) — evolver + sandbox 추가

```
opengoose-board           (OpenGoose 의존성 없음)
       ↑
opengoose-rig             (의존: board, goose)
       ↑
opengoose-evolver         (의존: board, rig, skills, goose)
       ↑
opengoose                 (의존: board, rig, skills, evolver — 바이너리)

opengoose-skills          (독립. board, rig, goose 의존 없음)
opengoose-sandbox         (독립. macOS 전용, HVF 의존)
```

### 3.4 "하지 않는 것" 테이블 (§3.2) — evolver + sandbox 행 추가

| 크레이트 | 하지 않는 것 |
|----------|-------------|
| **evolver** | Board CRUD, 세션 관리, CLI/TUI, 직접 스킬 파일 I/O (opengoose-skills에 위임) |
| **sandbox** | LLM 호출, Board 접근, 네트워크, 플랫폼 추상화 (macOS HVF 전용) |

### 3.5 설계 결정 기록 (새 섹션)

**ADR-1: 왜 Board가 SQLite + CowStore를 함께 관리하는가**

Board struct는 SQLite(영속성)와 CowStore(인메모리 브랜치/머지) 두 저장소를 소유한다.
분리하지 않는 이유:
- `merge()` 메서드에서 staged clone → merge → persist → swap 4단계가 하나의 Mutex lock 안에서 실행
- persist 실패 시 swap이 안 일어남 → CowStore와 SQLite 일관성 자동 보장
- 분리하면 이 원자성을 외부 호출자가 보장해야 함 → 동기화 버그 표면적 증가
- 현재 board.rs는 200줄, 실제 복잡도는 모듈(work_items/, store/, stamp_ops 등)에 분산
- 재검토 시점: board.rs가 500줄을 넘거나, 저장소 백엔드를 교체할 필요가 생길 때

---

## 4. Board→Worker 통합 테스트 (항목 4)

### 4.1 위치

`crates/opengoose-rig/tests/worker_integration.rs`

opengoose-rig가 Board에 의존하므로 여기에 배치. 바이너리 크레이트(opengoose)는 Goose 런타임 의존이 무거워서 부적합.

### 4.2 테스트 시나리오

| 테스트 | 검증 내용 |
|--------|----------|
| `post_claim_submit_lifecycle` | Board.post → claim → Status::Claimed → submit → Status::Done |
| `worker_skips_blocked_items` | 블로킹 의존성이 있는 항목은 claim 대상에서 제외 |
| `worker_retries_then_stuck` | 실행 실패 → 재시도 2회 → Status::Stuck 마킹 |
| `concurrent_workers_no_double_claim` | 2개 Worker가 동시에 pull해도 같은 항목을 중복 claim하지 않음 |

### 4.3 접근

- `Board::in_memory()` 사용 — 외부 의존 없음
- Goose `Agent::reply()`를 모킹하지 않음 (Goose-native 원칙)
- Worker의 claim/submit/retry 로직을 Board API 레벨에서 검증
- 실제 LLM 호출 제외 — 순수 Board 상태 전이 + 경쟁 조건에 집중
- 동시성 테스트는 `tokio::spawn` + `Arc<Board>` 사용

---

## 5. Doc-tests (항목 5)

### 5.1 범위

핵심 공개 API에만 `/// # Examples` 추가. 내부(`pub(crate)`) 함수 제외.

### 5.2 대상

| 크레이트 | 대상 |
|----------|------|
| **board** | `Board::connect`, `Board::in_memory`, `Board::branch`, `Board::merge` |
| **board** | `WorkItem`, `Status`, `Priority`, `RigId` |
| **board** | `filter_ready`, `prime_summary`, `find_compactable` (beads) |
| **rig** | `Worker::new`, `Operator::new` |
| **rig** | `WorkMode` trait |
| **skills** | `SkillCatalog::load`, `SkillMetadata` |

### 5.3 규칙

- `Board::in_memory().await` 사용하여 외부 의존 없이 실행 가능한 예제
- async 함수는 `# tokio::main` 래퍼 사용
- 실행 불가능한 경우 `no_run` 표시
- 대략 15~20개 doc-test 추가

---

## 6. 런타임 에러 핸들링 (항목 7)

### 6.1 변경

`crates/opengoose/src/runtime.rs`:

```
현재: Board → Web → Evolver → Worker → Ok(Runtime)
                                  ↑ 실패하면 전체 실패

변경: Board → Web → Evolver → Worker 시도
                                  ├─ 성공 → Runtime { worker: Some(worker) }
                                  └─ 실패 → tracing::warn! 로깅
                                            Runtime { worker: None }
```

### 6.2 구체적 변경

- `Runtime.worker` 타입: `Arc<Worker>` → `Option<Arc<Worker>>`
- Worker 생성(`create_worker_agent`) 실패 시 `warn!` 로깅 후 `None`으로 계속
- TUI Board 탭에서 worker가 None이면 "Worker offline" 상태 표시
- `create_agent`의 `unwrap_or_else(|_| ".".into())` — cwd 실패 폴백은 합리적이므로 유지

### 6.3 영향 범위

Runtime.worker를 사용하는 모든 곳에서 `Option` 처리 필요. 현재 runtime.rs 외에 worker를 직접 참조하는 곳을 확인하여 수정.

---

## 7. 범위 밖

- Board struct 분리 — ADR-1 참조
- Federation, 멀티 Worker UX — ARCHITECTURE.md 열린 질문으로 유지
