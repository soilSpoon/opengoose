# 참조: 기반 — Goose, Dolt, Beads

> OpenGoose v0.2가 직접 빌드하는 기반 기술의 근거.

---

## 1. Goose (block/goose)

**무엇인가:** Block이 만든 Rust 기반 AI 에이전트 프레임워크. MCP-native, 스트림 우선.

**v0.2를 위한 핵심 교훈:**

- **`Agent::reply()`는 `BoxStream<AgentEvent>`를 반환** — 항상 스트리밍, 동기 경로 없음. 우리의 rig가 이 스트림을 직접 소비한다.
- **MCP 우선** — 모든 도구가 MCP 서버. 독자 도구 포맷 없음. 우리의 팀 도구(`board__*`)는 MCP Stdio 서버.
- **Recipe 시스템** — extension, instruction, parameter, sub-recipe가 포함된 YAML 에이전트 설정. v1의 `AgentProfile` + `ProfileStore`를 대체.
- **SessionManager** — SQLite 기반 대화 이력 + fork/export. 전부 재사용.
- **ExtensionManager** — MCP 클라이언트 수명주기, 도구 라우팅, 도구 네이밍. 건드리지 않음.
- **서브에이전트 시스템** — `Agent::new()`가 격리된 인스턴스 생성. 병렬 워커에 적합하지만 부모-자식만 가능. 우리의 보드 기반 pull 모델이 P2P에 더 유연.
- **에러 핸들링** — 에러를 도구 결과로 모델에 전송 (errors-as-prompts). 모델이 자기 수정. 이것을 상속.
- **자동 압축** — 80% 임계값에서 컨텍스트 요약. 이것을 상속.

### Goose 내장 세션/메모리 (v0.2가 무료로 얻는 것)

**자동 압축 (compaction):**
- 기본 임계값: 80% (`GOOSE_AUTO_COMPACT_THRESHOLD` 환경변수)
- 매 `Agent::reply()` 전 `check_if_compaction_needed()` 호출
- LLM이 대화 요약 생성 → 원래 메시지는 `user_visible=true, agent_visible=false`
- 실패 시 progressive tool response 제거 (0% → 10% → 20% → 50% → 100%)
- `ContextLengthExceeded` 에러 시 최대 2회 복구 압축
- 수동: `/compact`, `/summarize`

**세션 재개:**
- `--resume`: 가장 최근 세션 또는 `--session-id`로 특정 세션 재개
- 작업 디렉토리, extension 상태, provider/model 설정 모두 복원
- `--fork`: 세션 복사 후 복사본에서 재개

**메모리 MCP (`goose-memory`):**
- `remember_memory(category, data, tags, is_global)` / `retrieve_memories(category)`
- 글로벌: `~/.config/goose/memory/`, 로컬: `{working_dir}/.goose/memory/`
- 부트 시 글로벌 메모리를 시스템 프롬프트에 주입
- **한계:** 키워드 기반 검색만 (시맨틱 검색 없음)

**세션 간 검색:**
- `search_chat_history(query, limit, after_date, before_date)` — 전 세션 키워드 검색

**재구현하지 않을 것:**

| Goose가 처리 | 우리가 추가 |
|--------------|------------|
| LLM 호출 루프 + 재시도 | Wanted Board (pull 작업 분배) |
| MCP 도구 디스패치 | Board MCP 도구 (`board__*`) |
| 세션 영속성 | 작업 항목 영속성 (CoW 스토어) |
| 컨텍스트 관리 + 압축 | Prime 주입 + pre-compaction flush |
| 퍼미션 모드 | 신뢰 기반 능력 게이팅 |
| 에러 복구 | Witness (stuck/zombie 감지) |
| 메모리 MCP (기본) | 시맨틱 메모리 + per-rig 스코핑 |

**최소 통합:**
```rust
// Goose와 하는 것은 이것이 전부
let agent = Agent::new(recipe, extensions).await;
let stream = agent.reply(message, session_config, cancel_token).await;
// AgentEvent를 위해 스트림 소비
```

---

## 2. Dolt (dolthub/dolt)

**무엇인가:** Git 시맨틱을 가진 MySQL 호환 SQL 데이터베이스. prolly tree로 구축.

**채택한 핵심 컨셉:**

### Prolly Tree
- 모든 노드가 콘텐츠 주소 지정된(SHA-256) B-tree
- 이력 독립: 삽입 순서와 무관하게 같은 데이터 → 같은 트리 구조
- 경계 안정 노드를 위한 콘텐츠 기반 청킹
- **v0.2 적용:** CoW 시맨틱의 `Arc<BTreeMap>` 사용. 진정한 prolly tree는 아니지만 중요한 속성을 보존: O(1) 브랜치, O(d) diff, 콘텐츠 주소 루트 해시.

### Branch/Merge 시맨틱
- `dolt branch` = O(1) 포인터 생성 → 우리의 `board.branch()` = Arc clone
- `dolt commit` = 루트 해시 스냅샷 → 우리의 `board.commit()` = 해시 + 로그 항목
- `dolt merge` = 3-way merge (base vs source vs dest) → 같은 알고리즘
- `dolt diff` = 변경된 경로만 비교 → 우리의 diff는 변경된 키를 비교

### 셀 레벨 충돌 해결
- Dolt는 행 레벨이 아닌 셀 레벨(행 + 열)로 머지
- 에이전트 A가 열 X를, 에이전트 B가 같은 행의 열 Y를 변경 → 충돌 아님
- 같은 (primary_key, column)을 다른 값으로 변경한 경우만 → 충돌
- **v0.2 필드 전략:** SourceWins, DestWins, HigherStatusWins, LatestTimestamp, Immutable, Union

### 생략하는 것
- 확률적 청킹 (대규모 온디스크 트리에 필요, 인메모리에는 불필요)
- 머클 경로 증명 (연합 검증에 필요, Phase 2로 연기)
- SQL 쿼리 엔진 (직접 키-값 접근 사용)
- 온디스크 포맷 (내구성은 SQLite WAL)

---

## 3. Beads (steveyegge/beads)

**무엇인가:** AI 에이전트를 위한 분산 그래프 이슈 트래커 및 영속 메모리 시스템. Dolt 위에 구축.

**채택한 핵심 컨셉:**

### 정수 ID (AUTO INCREMENT)
Board가 중앙에서 작업을 생성하므로 `INTEGER PRIMARY KEY AUTOINCREMENT`로 충분.
Beads의 content-hash(`SHA256[:6]`)는 분산 생성을 전제한 설계 — 단일 SQLite에서는 불필요.
CLI 참조: `#42`, `/stamp 42`, `/retry 7`.

### 3대 핵심 알고리즘

**`ready()`** — 열린 블로킹 의존성이 없는 작업, 우선순위 정렬.
- 사전 계산된 `blocked_cache`로 O(1) 준비성 확인
- 이행적 블로킹: A가 B를 막고 B가 C를 막으면 A가 이행적으로 C를 막음

**`prime()`** — 세션 시작을 위한 1-2K 토큰 컨텍스트 요약.
- 우선순위 분포, 블로킹 이슈, 준비된 작업, 최근 완료
- `BriefIssue`/`BriefDep` 모델 사용: 전체 객체 대비 97% 토큰 절감
- 세션 시작 시 시스템 프롬프트에 주입, 매 턴 갱신

**`compact()`** — 에이전트 메모리 감쇠.
- 30일 이상 된 닫힌 항목 → AI 생성 요약이 전체 내용 대체
- 보존: id, title, status, relationships, stamps
- 삭제: 상세 설명, 수락 기준, 로그

### Wisps (임시 작업)
세 가지 상태: Proto (고체, 동결 템플릿) → Molecule (액체, 활성, 영구) → Wisp (기체, 휘발, 동기화 없음).
- Wisp는 burn (하드 삭제), squash (요약), promote (영구화) 가능
- 영구 기록을 오염시키지 않는 에이전트 스크래치 작업

### 의존성 그래프
6가지 관계 유형: blocks, parent-child, waits-for, relates-to, duplicates, supersedes.
- 순환 감지 포함 이행적 블로킹 계산
- `waits-for` 게이트: FanOut 완료 (모든/일부 자식 완료)

### 3-way Merge 드라이버 (필드 레벨)
- 스칼라: 타임스탬프 기준 last-write-wins
- 배열 (labels, deps): 중복 제거 union
- 상태: 우선순위 규칙 (closed > in_progress > open)
- 우선순위: 수치적 최대 (P0 > P1)

### Beads가 SQLite에서 Dolt로 이동한 이유
1. 쓰기 경합 — SQLite의 단일 작성자 잠금이 5+ 동시 에이전트에서 실패
2. 셀 레벨 머지 — SQLite는 전체 행 덮어쓰기; Dolt는 열 단위 머지
3. 이력 — 즉시 롤백을 위한 `SELECT ... AS OF`
4. 동기화 — 자동 머지가 포함된 `dolt push/pull`

**v0.2 참고:** 단일 바이너리 Rust를 원하기 때문에 CoW BTreeMap (Dolt 아님)을 유지. 스케일링이 필요하면 Board API 뒤에서 Dolt로 저장소 백엔드 교체 가능.
