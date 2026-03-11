# Dolt/Beads → OpenGoose 포팅 가이드

> **작성일:** 2026-03-11
> **목표:** Dolt와 Beads의 핵심 기능을 OpenGoose 단일 바이너리에 임베디드로 포팅하기 위한 상세 분석

---

## Part 1: Dolt 기능 전수 조사

### 1.1 SQL 시스템 테이블 (18개)

| 시스템 테이블 | 용도 | OpenGoose 포팅 필요성 |
|---|---|---|
| `dolt_log` | 커밋 이력 (hash, author, date, message) | **필수** — 변경 이력 추적 |
| `dolt_diff_<table>` | 두 커밋 간 행 단위 diff | **필수** — 에이전트 변경 검토 |
| `dolt_commit_diff_<table>` | 특정 커밋이 도입한 변경 | 유용 — diff의 축약 버전 |
| `dolt_status` | 작업 변경 (staged/unstaged) | **필수** — 머지 전 확인 |
| `dolt_branches` | 브랜치 목록 + head 커밋 | **필수** — 에이전트별 브랜치 |
| `dolt_remote_branches` | 리모트 트래킹 브랜치 | 불필요 (단일 인스턴스) |
| `dolt_remotes` | 리모트 엔드포인트 | 불필요 (Phase 4 연합) |
| `dolt_conflicts` | 머지 충돌 (base/ours/theirs) | **필수** — 에이전트 충돌 해결 |
| `dolt_constraint_violations` | 머지 후 FK/unique 위반 | 유용 — 데이터 무결성 |
| `dolt_tags` | 이름 붙은 커밋 참조 | 선택 — 릴리스 마킹 |
| `dolt_schemas` | 저장 프로시저, 트리거, 뷰 | 불필요 (Diesel이 관리) |
| `dolt_docs` | DB 내 문서 | 불필요 |
| `dolt_procedures` | 사용자 정의 프로시저 | 불필요 |
| `dolt_statistics` | 쿼리 최적화 통계 | 불필요 (SQLite 자체 통계) |
| `dolt_column_diff` | 컬럼 레벨 diff 메타 | 유용 — cell-level diff |
| `dolt_commit_ancestors` | 커밋 DAG 부모-자식 | **필수** — 머지 베이스 계산 |
| `dolt_merge_status` | 머지 진행 중 여부 | 유용 — 상태 표시 |
| `dolt_workspace_<table>` | staged + unstaged 합쳐서 표시 | 선택 — 편의 기능 |

### 1.2 저장 프로시저 (22개)

| 프로시저 | Git 동등 | OpenGoose 포팅 | 우선순위 |
|---|---|---|---|
| `dolt_commit()` | `git commit` | `fn commit()` | **P0** |
| `dolt_checkout()` | `git checkout` | `fn checkout()` | **P0** |
| `dolt_merge()` | `git merge` | `fn merge()` | **P0** |
| `dolt_branch()` | `git branch` | `fn branch()` | **P0** |
| `dolt_reset()` | `git reset` | `fn reset()` | **P0** |
| `dolt_add()` | `git add` | `fn stage()` | **P1** |
| `dolt_diff()` | `git diff` | `fn diff()` | **P0** |
| `dolt_revert()` | `git revert` | `fn revert()` | P2 |
| `dolt_cherry_pick()` | `git cherry-pick` | `fn cherry_pick()` | P2 |
| `dolt_stash()` | `git stash` | `fn stash()` | P3 |
| `dolt_tag()` | `git tag` | `fn tag()` | P3 |
| `dolt_fetch()` | `git fetch` | — | Phase 4 |
| `dolt_pull()` | `git pull` | — | Phase 4 |
| `dolt_push()` | `git push` | — | Phase 4 |
| `dolt_clone()` | `git clone` | — | Phase 4 |
| `dolt_conflicts_resolve()` | 충돌 해결 | `fn resolve_conflict()` | **P0** |
| `dolt_gc()` | `git gc` | `fn gc()` | P2 |
| `dolt_clean()` | `git clean` | `fn clean()` | P3 |
| `dolt_rebase()` | `git rebase` | — | 불필요 |
| `dolt_backup()` | 백업 | `fn backup()` | P2 |
| `dolt_verify_constraints()` | 제약 확인 | `fn verify()` | P2 |
| `dolt_undrop()` | DB 복구 | — | 불필요 |

### 1.3 머지 전략: Cell-Level 3-Way Merge

Dolt의 가장 강력한 기능. 행 단위가 아닌 **셀(행×컬럼) 단위** 머지:

```
merge(base, ours, theirs):
    // 1. base↔ours diff와 base↔theirs diff를 계산
    diff_ours  = diff(base, ours)
    diff_theirs = diff(base, theirs)

    // 2. 각 변경된 키에 대해:
    for key in union(diff_ours.keys, diff_theirs.keys):
        if key only in diff_ours → 적용
        if key only in diff_theirs → 적용
        if same change in both → 적용 (동일 변경)
        if different changes:
            // 3. 셀 단위 비교 (Dolt 핵심 차별점)
            for each column:
                if only ours changed → take ours
                if only theirs changed → take theirs
                if both → same value → take either
                if both → different value → CONFLICT
```

**예시: 두 에이전트가 같은 work_item 수정**

| | base | agent-A (ours) | agent-B (theirs) | 결과 |
|---|---|---|---|---|
| title | "분석" | "분석" | "데이터 분석" | "데이터 분석" (theirs만 변경) |
| status | "pending" | "in_progress" | "pending" | "in_progress" (ours만 변경) |
| output | null | null | "결과..." | "결과..." (theirs만 변경) |
| assigned_to | null | "agent-A" | "agent-B" | **CONFLICT** (양쪽 다른 값) |

→ 4개 컬럼 중 **1개만 충돌**, 나머지 3개는 자동 머지

### 1.4 Prolly Tree 아키텍처

```
[Root Node] ← 루트 해시 = 커밋이 가리키는 값
    ├── [Internal Node A]
    │   ├── [Leaf: row1, row2, row3]    ← 각 리프 ~4KB
    │   └── [Leaf: row4, row5]
    └── [Internal Node B]
        ├── [Leaf: row6, row7, row8]
        └── [Leaf: row9, row10]

각 노드:
  hash: SHA-512(직렬화된 내용)
  keys: Vec<PrimaryKey>
  values: Vec<RowData> (리프) 또는 Vec<NodeHash> (내부)
  level: u8  (0=리프, >0=내부)
```

**핵심 특성:**
- **역사 독립성**: 동일 데이터 → 동일 트리 구조 (삽입 순서 무관)
- **구조적 공유**: 변경되지 않은 서브트리는 완전히 공유
- **O(변경) diff**: 같은 해시의 서브트리는 건너뜀
- **Rolling hash로 청크 경계 결정**: `hash(content) % (1 << pattern) == pattern`

### 1.5 커밋 그래프 (DAG)

```rust
struct Commit {
    hash: Hash,                  // SHA-512 (내용 주소)
    parents: Vec<Hash>,          // 1개 (일반), 2개 (머지)
    root_value: Hash,            // Prolly Tree 루트
    meta: CommitMeta {
        name: String,
        email: String,
        timestamp: DateTime,
        description: String,
    }
}

// 브랜치 = 커밋을 가리키는 참조 (O(1) 생성)
struct Branch {
    name: String,
    commit_hash: Hash,
    working_set: Hash,           // 스테이징 영역
}
```

### 1.6 Dolt 테스트 커버리지

| 테스트 유형 | 규모 | 내용 |
|---|---|---|
| Go 단위/통합 테스트 | 수천 개 | `_test.go` 파일, 스토리지/머지/SQL |
| BATS 테스트 | ~100+ 파일 | CLI 엔드투엔드 (commit, merge, diff 등) |
| sqllogictest | 수만 개 | MySQL 호환성 (99%+ 목표) |
| sysbench | 정기 실행 | 성능 벤치마크 (MySQL 대비 쓰기 2-5x, 읽기 1.5-2x) |
| CI | GitHub Actions | 매 PR: Go + BATS + SQL correctness |

**머지 테스트 (포팅 시 참고):**
- 양쪽이 같은 행 수정
- 한쪽 추가 + 한쪽 삭제
- 스키마 변경 중 머지
- FK 위반 감지
- PK 없는 테이블 머지
- 다중 공통 조상

### 1.7 Chunk Store 추상화

```go
type ChunkStore interface {
    Get(ctx, hash) → Chunk
    Has(ctx, hash) → bool
    Put(ctx, chunk)
    Root() → hash              // 현재 루트
    Commit(ctx, current, last) → bool  // CAS 커밋
}
```

디스크 레이아웃:
```
.dolt/noms/
├── manifest           # 루트 해시 + 테이블 파일 목록
├── <hash1>.idx        # 테이블 파일 인덱스
├── <hash1>            # 테이블 파일 (청크 묶음)
└── ...
```

---

## Part 2: Beads 기능 전수 조사

### 2.1 Bead 데이터 모델

```rust
struct Bead {
    // 식별
    id: String,              // "bd-k7m2x9" (SHA-256 해시 기반)
    title: String,
    description: Option<String>,

    // 계층
    parent_id: Option<String>,
    path: String,            // "bd-abc.1.2" (중첩 경로)
    depth: u32,

    // 상태
    status: BeadStatus,      // pending, in_progress, completed, failed, blocked, cancelled
    priority: Priority,      // critical, high, medium, low

    // 할당
    assigned_to: Option<String>,
    session_key: Option<String>,

    // 추적
    created_at: DateTime,
    updated_at: DateTime,
    completed_at: Option<DateTime>,

    // 콘텐츠
    input: Option<String>,
    output: Option<String>,
    error: Option<String>,

    // 메타
    tags: Vec<String>,
    estimated_effort: Option<Duration>,
    actual_effort: Option<Duration>,
    is_compacted: bool,

    // 관계
    relationships: Vec<Relationship>,
}
```

### 2.2 해시 ID 생성 알고리즘

```rust
fn generate_bead_id(title: &str, creator: &str) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    let mut hasher = Sha256::new();
    hasher.update(title.as_bytes());
    hasher.update(b"|");
    hasher.update(creator.as_bytes());
    hasher.update(b"|");
    hasher.update(timestamp.to_le_bytes());

    let hash = hasher.finalize();
    let value = u32::from_le_bytes([hash[0], hash[1], hash[2], hash[3]]);
    let encoded = base36_encode(value);

    format!("bd-{encoded}")
}
// 결과: "bd-k7m2x9" (7-9자, 충돌 불가, 머지 안전)
```

**특성:**
- 내용 주소 지정: 같은 입력(같은 시점) → 같은 ID
- 충돌 방지: 나노초 타임스탬프 포함
- 머지 안전: 순차 ID가 아니므로 브랜치 간 충돌 없음

### 2.3 관계 타입 (8가지)

| 관계 | 의미 | `ready()` 영향 |
|---|---|---|
| `subtask_of` | 부모-자식 | 자식 완료 → 부모 완료 가능 |
| `blocks` | A가 B를 차단 | B는 A 완료까지 시작 불가 |
| `blocked_by` | B가 A에 차단됨 | 위와 동일, 역방향 |
| `depends_on` | A가 B 결과 필요 | A는 B 완료까지 시작 불가 |
| `enables` | A 완료 → B 활성화 | B는 A 완료 시 ready |
| `relates_to` | 정보 링크 | 스케줄링 영향 없음 |
| `supersedes` | A가 B를 대체 | B 자동 취소 |
| `duplicates` | A가 B의 복제 | 하나 취소 |

### 2.4 핵심 알고리즘: `ready()`

```rust
fn ready(&self, session_key: Option<&str>) -> Vec<Bead> {
    let mut candidates: Vec<Bead> = self.all_beads()
        .filter(|b| b.status == Pending)
        // 모든 blockers가 완료되었는지
        .filter(|b| !self.has_unresolved_blockers(b.id))
        // 모든 depends_on이 완료되었는지
        .filter(|b| !self.has_unmet_dependencies(b.id))
        .collect();

    // 세션 필터링
    if let Some(key) = session_key {
        candidates.retain(|b|
            b.session_key.as_deref() == Some(key)
            || b.assigned_to == self.agent_for_session(key)
            || b.assigned_to.is_none()  // 미할당 작업도 포함
        );
    }

    // 우선순위 → 생성일 정렬
    candidates.sort_by(|a, b|
        a.priority.cmp(&b.priority)
            .then(a.created_at.cmp(&b.created_at))
    );

    // 배치 크기 제한 (컨텍스트 오버플로 방지)
    candidates.truncate(MAX_READY_BATCH); // 기본 5-10개
    candidates
}
```

**핵심 설계:**
- 진정으로 실행 가능한 태스크만 반환 (차단/대기 항목 제외)
- 의존성 그래프 존중
- 배치 크기 제한으로 AI 컨텍스트 윈도우 보호
- 세션별 필터링으로 에이전트가 자기 관련 태스크만 조회

### 2.5 핵심 알고리즘: `prime()`

```rust
fn prime(&self, session_key: &str) -> String {
    let mut context = String::new();

    // 1. 프로젝트 개요
    context += "# Project Context\n";
    context += &self.project_description();

    // 2. 진행 중 태스크
    context += "\n# Active Tasks\n";
    for bead in self.get_in_progress(session_key) {
        context += &format_bead_summary(&bead);
    }

    // 3. Ready 태스크
    context += "\n# Ready Tasks\n";
    for bead in self.ready(Some(session_key)) {
        context += &format_bead_summary(&bead);
    }

    // 4. 최근 완료 (컨텍스트)
    context += "\n# Recently Completed\n";
    for bead in self.get_recently_completed(session_key, Duration::hours(24)) {
        context += &format_bead_brief(&bead);
    }

    // 5. 차단된 항목
    context += "\n# Blocked Items\n";
    for bead in self.get_blocked(session_key) {
        context += &format_bead_with_blocker(&bead);
    }

    // 6. 의존성 그래프
    context += "\n# Dependencies\n";
    context += &self.render_dependency_graph(session_key);

    context
}
```

**용도:** AI 에이전트 세션 시작 시 필요한 모든 컨텍스트를 한 번에 생성

### 2.6 핵심 알고리즘: `compact()`

```rust
fn compact(&self, older_than: Duration) -> Vec<CompactedBead> {
    let old_completed = self.all_beads()
        .filter(|b| b.status == Completed)
        .filter(|b| b.completed_at.unwrap() < now() - older_than)
        .filter(|b| !b.is_compacted)
        .collect::<Vec<_>>();

    let groups = group_by_parent_or_tag(&old_completed);

    groups.into_iter().map(|group| {
        let summary = summarize(&group); // 템플릿 또는 AI 요약

        let compacted = CompactedBead {
            original_ids: group.iter().map(|b| b.id.clone()).collect(),
            summary,
            key_outputs: group.iter()
                .filter_map(|b| b.output.clone())
                .take(3)
                .collect(),
            compacted_at: now(),
        };

        // 원본은 삭제하지 않고 숨김
        for bead in &group {
            self.mark_compacted(bead.id);
        }

        compacted
    }).collect()
}
```

**용도:** 컨텍스트 윈도우가 제한된 AI 에이전트에서 오래된 태스크를 요약하여 토큰 절약

### 2.7 Beads 저장 스키마

```sql
CREATE TABLE beads (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    description TEXT,
    parent_id TEXT REFERENCES beads(id),
    path TEXT NOT NULL,
    depth INTEGER DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'pending',
    priority TEXT NOT NULL DEFAULT 'medium',
    assigned_to TEXT,
    session_key TEXT,
    input TEXT,
    output TEXT,
    error TEXT,
    tags TEXT,                    -- JSON array
    estimated_effort_secs INTEGER,
    actual_effort_secs INTEGER,
    is_compacted BOOLEAN DEFAULT FALSE,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    completed_at TEXT
);

CREATE TABLE relationships (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id TEXT NOT NULL REFERENCES beads(id),
    target_id TEXT NOT NULL REFERENCES beads(id),
    relationship_type TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(source_id, target_id, relationship_type)
);

CREATE TABLE sessions (
    key TEXT PRIMARY KEY,
    agent_name TEXT NOT NULL,
    started_at TEXT NOT NULL,
    last_active TEXT NOT NULL,
    context_tokens INTEGER DEFAULT 0
);

CREATE TABLE compacted_beads (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    original_ids TEXT NOT NULL,   -- JSON array
    summary TEXT NOT NULL,
    key_outputs TEXT,             -- JSON array
    compacted_at TEXT NOT NULL
);
```

### 2.8 Beads 테스트 패턴

| 영역 | 테스트 |
|---|---|
| 해시 ID | 결정성, 유일성, 충돌 내성 |
| 상태 전이 | pending → in_progress → completed 등 |
| 관계 관리 | 추가, 삭제, 순환 검출 |
| DAG 순환 감지 | A→B→C→A 금지 |
| 우선순위 정렬 | critical > high > medium > low |
| 경로 계산 | 부모-자식 중첩 |
| ready() | 차단된 태스크 제외, 의존성 미충족 제외 |
| prime() | 컨텍스트 생성 정확성 |
| compact() | 요약 후 데이터 무결성 |
| 동시 할당 | 경쟁 조건 방지 |

---

## Part 3: OpenGoose 현황 대비 Gap 분석

### 3.1 현재 OpenGoose 데이터 모델

```
opengoose-persistence (13 테이블):
├── sessions (세션 관리)
├── messages (대화 이력)
├── message_queue (에이전트 간 메시징)
├── work_items (태스크 추적 — Beads의 기반)
├── orchestration_runs (워크플로 실행)
├── alert_rules / alert_history (알림)
├── event_history (이벤트 로그)
├── schedules (크론 스케줄)
├── agent_messages (에이전트 간 통신)
├── triggers (이벤트 트리거)
├── plugins (플러그인 레지스트리)
└── api_keys (API 인증)
```

현재 `WorkItem`:
```rust
pub struct WorkItem {
    pub id: i32,                     // ← 순차 ID (머지 시 충돌 가능)
    pub session_key: String,
    pub team_run_id: String,
    pub parent_id: Option<i32>,      // ← 계층 지원
    pub title: String,
    pub description: Option<String>,
    pub status: WorkStatus,          // 5개 상태 (blocked 없음)
    pub assigned_to: Option<String>,
    pub workflow_step: Option<i32>,
    pub input: Option<String>,
    pub output: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
```

### 3.2 Gap 매트릭스

#### Beads 포팅 Gap

| 기능 | OpenGoose WorkItem | Beads | Gap | 구현 난이도 |
|---|---|---|---|---|
| 태스크 생성 | ✅ | ✅ | — | — |
| 부모-자식 계층 | ✅ (`parent_id`) | ✅ (경로 기반) | 경로+깊이 추가 | 쉬움 |
| 상태 추적 | ✅ (5가지) | ✅ (6가지) | `Blocked` 추가 | 쉬움 |
| 에이전트 할당 | ✅ | ✅ | — | — |
| I/O 추적 | ✅ | ✅ | — | — |
| **해시 ID** | ❌ (순차 int) | ✅ | `hash_id` 컬럼 | 쉬움 (~50줄) |
| **관계 타입** | ❌ | ✅ (8가지) | `relationships` 테이블 | 중간 (~100줄) |
| **우선순위** | ❌ | ✅ (4단계) | `priority` 컬럼 | 쉬움 |
| **태그** | ❌ | ✅ | `tags` 컬럼 (JSON) | 쉬움 |
| **ready()** | ❌ | ✅ | 알고리즘 구현 | 중간 (~80줄) |
| **prime()** | ❌ | ✅ | 컨텍스트 생성기 | 중간 (~100줄) |
| **compact()** | ❌ | ✅ | 요약 + 아카이브 | 중간 (~80줄) |
| **순환 감지** | ❌ | ✅ | petgraph DAG | 쉬움 (~30줄) |

**총 예상: ~500-700줄 Rust 추가**

#### Dolt 포팅 Gap

| 기능 | OpenGoose 현재 | Dolt | Gap | 구현 난이도 |
|---|---|---|---|---|
| **브랜치 생성** | ❌ | O(1) ref | `VACUUM INTO` 파일 복사 | 쉬움 |
| **브랜치 전환** | ❌ | ref 변경 | DB 연결 대상 변경 | 쉬움 |
| **Diff** | ❌ | cell-level | `ATTACH` + `EXCEPT` | 중간 (~200줄) |
| **3-way Merge** | ❌ | cell-level 자동 | 행×컬럼 비교 로직 | **어려움** (~500줄) |
| **커밋 이력** | event_history (부분) | 완전한 DAG | 커밋 테이블 + DAG | 중간 (~200줄) |
| **충돌 해결** | ❌ | ours/theirs | 충돌 테이블 + UI | 중간 (~150줄) |
| **Reset/Rollback** | ❌ | soft/hard | 파일 삭제/교체 | 쉬움 |
| **시간여행** | ❌ | `AS OF` 쿼리 | Temporal 테이블 | 중간 (~200줄) |
| **스키마 Diff** | ❌ | 스키마 비교 | `PRAGMA table_info` 비교 | 쉬움 |
| 스토리지 공유 | ❌ | Prolly Tree CoW | 전체 DB 복사 | **포기** (20 에이전트 이하 OK) |
| 연합 sync | ❌ | push/pull | Phase 4: cr-sqlite | 나중 |

**총 예상: ~1,200-1,500줄 Rust 추가 (Beads 제외)**

---

## Part 4: 포팅 전략 및 상세 구현 계획

### 4.1 새 DB 스키마 (마이그레이션)

```sql
-- Migration: 2024-01-11-000000_add_versioning_and_beads

-- 1. work_items 확장 (Beads 기능)
ALTER TABLE work_items ADD COLUMN hash_id TEXT;
ALTER TABLE work_items ADD COLUMN path TEXT;
ALTER TABLE work_items ADD COLUMN depth INTEGER DEFAULT 0;
ALTER TABLE work_items ADD COLUMN priority TEXT DEFAULT 'medium';
ALTER TABLE work_items ADD COLUMN tags TEXT DEFAULT '[]';
ALTER TABLE work_items ADD COLUMN is_compacted BOOLEAN DEFAULT 0;
ALTER TABLE work_items ADD COLUMN completed_at TEXT;

CREATE UNIQUE INDEX idx_work_items_hash_id ON work_items(hash_id);
CREATE INDEX idx_work_items_path ON work_items(path);
CREATE INDEX idx_work_items_priority ON work_items(priority);
CREATE INDEX idx_work_items_status_session ON work_items(status, session_key);

-- 2. 태스크 관계 테이블 (Beads 관계)
CREATE TABLE work_item_relationships (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id INTEGER NOT NULL REFERENCES work_items(id),
    target_id INTEGER NOT NULL REFERENCES work_items(id),
    relationship_type TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(source_id, target_id, relationship_type)
);

CREATE INDEX idx_wir_source ON work_item_relationships(source_id);
CREATE INDEX idx_wir_target ON work_item_relationships(target_id);

-- 3. 압축 태스크 (Beads compact)
CREATE TABLE compacted_work_items (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    original_ids TEXT NOT NULL,           -- JSON array of work_item IDs
    summary TEXT NOT NULL,
    key_outputs TEXT,                     -- JSON array
    session_key TEXT,
    compacted_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 4. 커밋 테이블 (Dolt 버저닝)
CREATE TABLE vcs_commits (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    hash TEXT NOT NULL UNIQUE,           -- SHA-256 of content
    parent_hash TEXT,                     -- NULL for initial commit
    parent_hash_2 TEXT,                  -- Non-NULL for merge commits
    branch TEXT NOT NULL,
    author TEXT NOT NULL,
    message TEXT NOT NULL,
    snapshot_path TEXT,                   -- DB 스냅샷 경로 (선택)
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (parent_hash) REFERENCES vcs_commits(hash),
    FOREIGN KEY (parent_hash_2) REFERENCES vcs_commits(hash)
);

CREATE INDEX idx_vcs_commits_branch ON vcs_commits(branch);
CREATE INDEX idx_vcs_commits_hash ON vcs_commits(hash);

-- 5. 브랜치 테이블 (Dolt 브랜칭)
CREATE TABLE vcs_branches (
    name TEXT PRIMARY KEY,
    head_commit_hash TEXT NOT NULL REFERENCES vcs_commits(hash),
    db_path TEXT NOT NULL,               -- SQLite 파일 경로
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 6. 충돌 테이블 (Dolt 머지 충돌)
CREATE TABLE vcs_conflicts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    merge_commit_hash TEXT NOT NULL,     -- 머지 시도 시점
    table_name TEXT NOT NULL,
    row_key TEXT NOT NULL,               -- 충돌 행의 PK
    column_name TEXT NOT NULL,           -- 충돌 컬럼
    base_value TEXT,
    ours_value TEXT,
    theirs_value TEXT,
    resolution TEXT,                     -- 'ours', 'theirs', 'manual', NULL (미해결)
    resolved_at TEXT,
    UNIQUE(merge_commit_hash, table_name, row_key, column_name)
);

-- 7. 변경 이력 테이블 (Dolt 시간여행)
-- 각 테이블마다 _history 테이블 생성 (예: work_items)
CREATE TABLE work_items_history (
    history_id INTEGER PRIMARY KEY AUTOINCREMENT,
    operation TEXT NOT NULL,             -- 'INSERT', 'UPDATE', 'DELETE'
    commit_hash TEXT,                    -- 관련 커밋
    changed_at TEXT NOT NULL DEFAULT (datetime('now')),
    changed_by TEXT,                     -- 에이전트 이름
    -- 원본 work_items 컬럼 전체 복제
    row_id INTEGER,
    session_key TEXT,
    team_run_id TEXT,
    parent_id INTEGER,
    title TEXT,
    description TEXT,
    status TEXT,
    assigned_to TEXT,
    workflow_step INTEGER,
    input TEXT,
    output TEXT,
    error TEXT,
    hash_id TEXT,
    path TEXT,
    depth INTEGER,
    priority TEXT,
    tags TEXT
);

CREATE INDEX idx_wih_commit ON work_items_history(commit_hash);
CREATE INDEX idx_wih_row_id ON work_items_history(row_id);
CREATE INDEX idx_wih_changed_at ON work_items_history(changed_at);

-- 8. 자동 이력 기록 트리거
CREATE TRIGGER work_items_after_insert
AFTER INSERT ON work_items
BEGIN
    INSERT INTO work_items_history
        (operation, changed_by, row_id, session_key, team_run_id, parent_id,
         title, description, status, assigned_to, workflow_step, input, output,
         error, hash_id, path, depth, priority, tags)
    VALUES
        ('INSERT', NEW.assigned_to, NEW.id, NEW.session_key, NEW.team_run_id,
         NEW.parent_id, NEW.title, NEW.description, NEW.status, NEW.assigned_to,
         NEW.workflow_step, NEW.input, NEW.output, NEW.error, NEW.hash_id,
         NEW.path, NEW.depth, NEW.priority, NEW.tags);
END;

CREATE TRIGGER work_items_after_update
AFTER UPDATE ON work_items
BEGIN
    INSERT INTO work_items_history
        (operation, changed_by, row_id, session_key, team_run_id, parent_id,
         title, description, status, assigned_to, workflow_step, input, output,
         error, hash_id, path, depth, priority, tags)
    VALUES
        ('UPDATE', NEW.assigned_to, OLD.id, OLD.session_key, OLD.team_run_id,
         OLD.parent_id, OLD.title, OLD.description, OLD.status, OLD.assigned_to,
         OLD.workflow_step, OLD.input, OLD.output, OLD.error, OLD.hash_id,
         OLD.path, OLD.depth, OLD.priority, OLD.tags);
END;

CREATE TRIGGER work_items_after_delete
AFTER DELETE ON work_items
BEGIN
    INSERT INTO work_items_history
        (operation, changed_by, row_id, session_key, team_run_id, parent_id,
         title, description, status, assigned_to, workflow_step, input, output,
         error, hash_id, path, depth, priority, tags)
    VALUES
        ('DELETE', OLD.assigned_to, OLD.id, OLD.session_key, OLD.team_run_id,
         OLD.parent_id, OLD.title, OLD.description, OLD.status, OLD.assigned_to,
         OLD.workflow_step, OLD.input, OLD.output, OLD.error, OLD.hash_id,
         OLD.path, OLD.depth, OLD.priority, OLD.tags);
END;
```

### 4.2 Rust 모듈 구조

```
opengoose-persistence/src/
├── lib.rs                          # 기존
├── db.rs                           # 기존
├── schema.rs                       # Diesel 스키마 (자동 생성)
├── models.rs                       # 기존 + 새 모델
├── work_items.rs                   # 기존 확장
├── ...기존 파일들...
│
├── beads/                          # 신규: Beads 기능
│   ├── mod.rs                      # BeadStore 공개 API
│   ├── hash_id.rs                  # SHA-256 + base36 해시 ID 생성
│   ├── relationships.rs            # 관계 CRUD + 순환 감지
│   ├── ready.rs                    # ready() 알고리즘
│   ├── prime.rs                    # prime() 컨텍스트 생성
│   └── compact.rs                  # compact() 요약/아카이브
│
└── vcs/                            # 신규: 버전 관리 기능
    ├── mod.rs                      # VcsStore 공개 API
    ├── branch.rs                   # 브랜치 생성/삭제/전환
    ├── commit.rs                   # 커밋 생성 + DAG 관리
    ├── diff.rs                     # 테이블 diff (ATTACH + EXCEPT)
    ├── merge.rs                    # 3-way cell-level 머지
    ├── conflict.rs                 # 충돌 기록/해결
    └── history.rs                  # 시간여행 쿼리
```

### 4.3 핵심 Trait 설계

```rust
// === beads/mod.rs ===

/// Beads 태스크 그래프 관리
pub struct BeadStore {
    db: Arc<Database>,
    graph: Mutex<StableGraph<i32, RelationType>>,  // petgraph
}

impl BeadStore {
    pub fn new(db: Arc<Database>) -> Self;

    // 태스크 관리
    pub fn create(&self, title: &str, creator: &str, parent_id: Option<i32>)
        -> PersistenceResult<WorkItem>;
    pub fn claim(&self, work_item_id: i32, agent: &str)
        -> PersistenceResult<()>;

    // 관계
    pub fn add_relationship(&self, source: i32, target: i32, rel_type: RelationType)
        -> PersistenceResult<()>;
    pub fn remove_relationship(&self, source: i32, target: i32, rel_type: RelationType)
        -> PersistenceResult<()>;

    // 핵심 알고리즘
    pub fn ready(&self, session_key: Option<&str>) -> PersistenceResult<Vec<WorkItem>>;
    pub fn prime(&self, session_key: &str) -> PersistenceResult<String>;
    pub fn compact(&self, older_than: Duration) -> PersistenceResult<Vec<CompactedWorkItem>>;
}

// === vcs/mod.rs ===

/// Git 스타일 버전 관리
pub struct VcsStore {
    db: Arc<Database>,
    branches_dir: PathBuf,  // ~/.opengoose/branches/
}

impl VcsStore {
    pub fn new(db: Arc<Database>, data_dir: PathBuf) -> Self;

    // 브랜치
    pub fn create_branch(&self, name: &str) -> PersistenceResult<Branch>;
    pub fn delete_branch(&self, name: &str) -> PersistenceResult<()>;
    pub fn list_branches(&self) -> PersistenceResult<Vec<Branch>>;
    pub fn checkout(&self, branch_name: &str) -> PersistenceResult<()>;

    // 커밋
    pub fn commit(&self, message: &str, author: &str) -> PersistenceResult<Commit>;
    pub fn log(&self, branch: &str, limit: usize) -> PersistenceResult<Vec<Commit>>;

    // Diff & Merge
    pub fn diff(&self, from: &str, to: &str) -> PersistenceResult<Vec<TableDiff>>;
    pub fn merge(&self, branch: &str) -> PersistenceResult<MergeResult>;
    pub fn resolve_conflict(&self, conflict_id: i32, resolution: Resolution)
        -> PersistenceResult<()>;

    // 시간여행
    pub fn as_of(&self, table: &str, timestamp: &str) -> PersistenceResult<Vec<Row>>;

    // 롤백
    pub fn reset(&self, branch: &str, mode: ResetMode) -> PersistenceResult<()>;
}
```

### 4.4 테스트 계획

Dolt와 Beads의 테스트 패턴을 참고한 OpenGoose 테스트 목록:

#### Beads 테스트 (beads/ 모듈)

```rust
// hash_id 테스트
#[test] fn hash_id_format()           // "bd-" 접두사 + base36
#[test] fn hash_id_uniqueness()       // 같은 제목이라도 다른 ID (타임스탬프)
#[test] fn hash_id_determinism()      // 같은 입력+시간 → 같은 ID

// 관계 테스트
#[test] fn add_blocks_relationship()
#[test] fn remove_relationship()
#[test] fn detect_cycle()             // A→B→C→A 금지
#[test] fn supersedes_auto_cancel()   // A supersedes B → B.status = cancelled

// ready() 테스트
#[test] fn ready_excludes_blocked()         // 차단된 태스크 제외
#[test] fn ready_excludes_unmet_deps()      // 미충족 의존성 제외
#[test] fn ready_respects_priority()        // critical > high > medium > low
#[test] fn ready_filters_by_session()       // 세션별 필터링
#[test] fn ready_limits_batch_size()        // 최대 반환 개수
#[test] fn ready_includes_unassigned()      // 미할당 태스크 포함
#[test] fn ready_after_blocker_completes()  // 차단자 완료 → 태스크 활성화

// prime() 테스트
#[test] fn prime_includes_active_tasks()
#[test] fn prime_includes_ready_tasks()
#[test] fn prime_includes_recent_completions()
#[test] fn prime_includes_blocked_items()

// compact() 테스트
#[test] fn compact_groups_by_parent()
#[test] fn compact_preserves_key_outputs()
#[test] fn compact_marks_originals()
#[test] fn compact_ignores_recent()          // older_than 미만은 무시
#[test] fn compacted_excluded_from_ready()
```

#### VCS 테스트 (vcs/ 모듈)

```rust
// 브랜치 테스트
#[test] fn create_branch_copies_db()
#[test] fn create_branch_records_metadata()
#[test] fn delete_branch_removes_file()
#[test] fn list_branches_includes_main()
#[test] fn checkout_switches_connection()

// 커밋 테스트
#[test] fn commit_creates_hash()
#[test] fn commit_records_parent()
#[test] fn commit_history_is_dag()
#[test] fn merge_commit_has_two_parents()

// Diff 테스트 (Dolt BATS 참고)
#[test] fn diff_detects_added_rows()
#[test] fn diff_detects_deleted_rows()
#[test] fn diff_detects_modified_rows()
#[test] fn diff_detects_modified_columns()    // cell-level
#[test] fn diff_empty_when_identical()
#[test] fn diff_across_branches()

// 머지 테스트 (Dolt 핵심 — 가장 중요)
#[test] fn merge_no_conflict_different_rows()       // 서로 다른 행 수정
#[test] fn merge_no_conflict_different_columns()     // 같은 행, 다른 컬럼
#[test] fn merge_no_conflict_same_change()           // 같은 행, 같은 컬럼, 같은 값
#[test] fn merge_conflict_different_values()         // 같은 행, 같은 컬럼, 다른 값
#[test] fn merge_one_add_one_delete()                // 한쪽 추가, 한쪽 삭제
#[test] fn merge_both_add_same_pk()                  // 양쪽 같은 PK 추가
#[test] fn merge_both_add_different_pk()             // 양쪽 다른 PK 추가
#[test] fn merge_cascade_fk_violation()              // FK 위반 감지

// 충돌 해결 테스트
#[test] fn resolve_conflict_ours()
#[test] fn resolve_conflict_theirs()
#[test] fn resolve_conflict_manual_value()
#[test] fn all_conflicts_resolved_then_commit()

// 시간여행 테스트
#[test] fn as_of_returns_past_state()
#[test] fn as_of_after_update()
#[test] fn as_of_after_delete()

// 롤백 테스트
#[test] fn reset_hard_discards_changes()
#[test] fn reset_soft_keeps_changes()
```

### 4.5 구현 순서 (Phase별)

```
Phase 1: Beads 기반 (예상 ~500줄, 테스트 ~300줄)
├── 1a. 마이그레이션 (work_items 확장 + relationships 테이블)
├── 1b. hash_id.rs (SHA-256 + base36)
├── 1c. relationships.rs (CRUD + 순환 감지)
├── 1d. ready.rs (실행 가능 태스크 필터)
├── 1e. prime.rs (에이전트 컨텍스트 생성)
└── 1f. compact.rs (오래된 태스크 요약)

Phase 2: VCS 브랜칭 (예상 ~800줄, 테스트 ~400줄)
├── 2a. 마이그레이션 (vcs_commits, vcs_branches, vcs_conflicts)
├── 2b. branch.rs (VACUUM INTO 기반)
├── 2c. commit.rs (해시 + DAG)
├── 2d. diff.rs (ATTACH + EXCEPT)
└── 2e. history.rs (temporal 테이블 + 트리거)

Phase 3: VCS 머지 (예상 ~500줄, 테스트 ~500줄)
├── 3a. merge.rs (3-way cell-level)
├── 3b. conflict.rs (충돌 기록/해결)
└── 3c. 통합 테스트 (Dolt BATS 패턴 참고)

Phase 4: 연합 (나중)
├── 4a. cr-sqlite 통합
└── 4b. 분산 동기화
```

### 4.6 의존성 추가

```toml
# Cargo.toml (opengoose-persistence)
[dependencies]
# 기존 유지
diesel = { version = "2.2", features = ["sqlite"] }

# 신규 추가
petgraph = "0.8"           # DAG (Beads 관계 그래프)
sha2 = "0.10"              # SHA-256 (해시 ID + 커밋 해시)
# base36 인코딩은 직접 구현 (~15줄, 크레이트 불필요)

# 선택 (Phase 4)
# rusqlite = "0.32"        # ATTACH/VACUUM INTO (Diesel 미지원 기능)
```

---

## Part 5: Dolt vs SQLite+커스텀 최종 비교

### 5.1 기능 커버리지

| Dolt 기능 (22개 프로시저) | SQLite+커스텀으로 포팅 가능? | 난이도 |
|---|---|---|
| `dolt_branch` | ✅ `VACUUM INTO` | 쉬움 |
| `dolt_checkout` | ✅ DB 연결 전환 | 쉬움 |
| `dolt_commit` | ✅ 해시 생성 + DAG 기록 | 중간 |
| `dolt_merge` | ✅ 3-way cell-level (커스텀) | **어려움** |
| `dolt_diff` | ✅ `ATTACH` + `EXCEPT` | 중간 |
| `dolt_reset` | ✅ 파일 교체/삭제 | 쉬움 |
| `dolt_log` | ✅ `vcs_commits` 조회 | 쉬움 |
| `dolt_status` | ✅ 현재 DB vs 마지막 커밋 diff | 중간 |
| `dolt_conflicts_resolve` | ✅ `vcs_conflicts` 업데이트 | 쉬움 |
| `dolt_revert` | ✅ 역방향 diff 적용 | 중간 |
| `dolt_cherry_pick` | ✅ 특정 커밋 diff 적용 | 중간 |
| `dolt_stash` | ✅ 임시 파일 복사 | 쉬움 |
| `dolt_tag` | ✅ 커밋 해시에 이름 부여 | 쉬움 |
| `dolt_gc` | ✅ 참조 없는 스냅샷 삭제 | 중간 |
| `dolt_add` | ⚠️ 스테이징이 필요하면 구현 | 선택 |
| `dolt_fetch/pull/push/clone` | ❌ → Phase 4: cr-sqlite | 나중 |
| `dolt_rebase` | ❌ 불필요 (에이전트 워크플로) | 불필요 |

**22개 중 14개 포팅 가능, 4개 Phase 4, 4개 불필요**

### 5.2 포기하는 것

1. **Prolly Tree 스토리지 효율성**: 브랜치당 전체 DB 복사 (에이전트 20개 이하: ~200MB-1GB, 관리 가능)
2. **O(변경) diff**: SQLite `EXCEPT`는 O(전체 테이블) — 하지만 OpenGoose 테이블 크기가 작으므로 실용적 문제 없음
3. **내장 연합**: push/pull 없음 → cr-sqlite로 보완 가능

### 5.3 얻는 것

1. **별도 서버 불필요** (가장 큰 이점)
2. **기존 Diesel/SQLite 코드 100% 유지**
3. **바이너리 크기 증가 최소** (petgraph + sha2 ≈ ~500KB)
4. **테스트 인프라 그대로** (`Database::open_in_memory()` 패턴)
5. **운영 복잡도 제로**
6. **점진적 도입 가능** (Phase별 독립 배포)

---

## 참고: Dolt 테스트에서 배울 점

### BATS 스타일 통합 테스트

Dolt의 BATS 테스트는 실제 CLI를 실행하는 엔드투엔드 테스트. OpenGoose에서는 기존 `opengoose-cli/tests/` 패턴을 확장:

```rust
// tests/vcs_integration.rs
#[test]
fn branch_create_modify_merge() {
    let env = test_env();
    let db = Database::open_in_memory().unwrap();
    let vcs = VcsStore::new(Arc::new(db), env.branches_dir());

    // 1. main에 데이터 삽입
    create_work_item(&vcs, "task-1");

    // 2. 브랜치 생성
    vcs.create_branch("agent-1").unwrap();

    // 3. 브랜치에서 수정
    vcs.checkout("agent-1").unwrap();
    update_work_item(&vcs, "task-1", status: "completed");

    // 4. main으로 돌아와서 머지
    vcs.checkout("main").unwrap();
    let result = vcs.merge("agent-1").unwrap();
    assert!(result.conflicts.is_empty());
    assert_eq!(get_work_item(&vcs, "task-1").status, "completed");
}
```

### sqllogictest 스타일 정확성 테스트

Dolt가 MySQL 호환성을 검증하는 것처럼, OpenGoose VCS는 diff/merge 정확성을 검증:

```rust
// tests/merge_correctness.rs
// 모든 가능한 2-에이전트 충돌 시나리오를 테이블 기반으로 테스트
#[test_case(
    base: {title: "A", status: "pending"},
    ours: {title: "A", status: "done"},
    theirs: {title: "B", status: "pending"},
    expected: {title: "B", status: "done"} ; // 다른 컬럼 → 자동 머지
    "different columns no conflict"
)]
#[test_case(
    base: {title: "A"},
    ours: {title: "B"},
    theirs: {title: "C"},
    expected: Conflict{column: "title", ours: "B", theirs: "C"} ;
    "same column different values"
)]
fn merge_scenario(base: Row, ours: Row, theirs: Row, expected: MergeExpectation) {
    // ... 테스트 구현
}
```
