# Rust 네이티브 임베디드 대안 분석: Dolt/Beads 없이 단일 바이너리

> **분석일:** 2026-03-11
> **핵심 제약:** 별도 서버 없음, 단일 바이너리, 외부 프로그램 의존 없음
> **관련 문서:** [dolt-deep-dive.md](./dolt-deep-dive.md), [database-strategy.md](./database-strategy.md)

---

## 1. 제약 조건이 바꾸는 것

dolt-deep-dive.md에서는 Dolt + Beads를 권장했지만, **"단일 바이너리, 외부 의존 없음"** 제약을 적용하면 상황이 달라진다:

| 도구 | 언어 | 서버 필요 | 임베딩 가능 | 결론 |
|------|------|----------|------------|------|
| Dolt | Go | 별도 서버 (port 3307) | 불가 | **탈락** |
| Beads | Go | CLI 프로세스 | 불가 | **탈락** |
| PostgreSQL | C | 별도 서버 | 불가 | **탈락** |
| Redis | C | 별도 서버 | 불가 | **탈락** |
| **SQLite** | **C** | **없음** | **임베딩** | **유지** |

**남는 선택지**: SQLite를 기반으로 Dolt/Beads의 핵심 가치를 Rust로 직접 구현한다.

---

## 2. Rust 생태계에서 사용 가능한 것들

### 2.1 데이터 버전 관리 (Dolt 대안)

| 솔루션 | Rust 임베딩 | 브랜칭/머지 | SQL 호환 | 성숙도 | 평가 |
|--------|:---------:|:---------:|:-------:|:-----:|------|
| **SQLite + 커스텀 버저닝** | rusqlite/Diesel | DIY | 완전 호환 | 검증됨 | **최적** |
| cr-sqlite | C 확장 로드 | CRDT 자동 머지만 | SQLite 확장 | 프로덕션 사용 | 동기화엔 좋지만 브랜칭 아님 |
| libSQL (Turso) | C 또는 Rust | 없음 (클라우드만) | SQLite 호환 | 프로덕션 | SQLite 대체로는 좋으나 버저닝 없음 |
| prollytree crate | 순수 Rust | 트리 diff만 | SQL 없음 | 초기 (v0.3) | 빌딩 블록, 완성품 아님 |
| CrepeDB (redb 위) | 순수 Rust | 포크/스냅샷 있음 | SQL 없음 | 매우 초기 (v0.1) | KV만, SQL 없음 |
| sled + sled-snapshots | 순수 Rust | 스냅샷 포레스트 | SQL 없음 | 리라이트 중 (위험) | 유지보수 불확실 |
| SurrealKV | 순수 Rust | 시간여행만, 브랜치 없음 | SQL 없음 | 개발 중 | 선형 이력만 |
| git2-rs / gitoxide | Rust (C 또는 순수) | 완전한 Git 브랜칭 | SQL 없음 | 성숙 | 고빈도 쓰기에 부적합 |
| FrankenSQLite | 순수 Rust | 시간여행만 | SQLite 파일 호환 | 매우 실험적 | 검증 안 됨 |
| LiteTree | C (SQLite 수정) | 네이티브 브랜칭! | SQLite 호환 | **2018년 이후 죽음** | 사망 |

**핵심 발견**: Rust 생태계에 **"임베디드 + SQL + 브랜칭/머지"를 모두 제공하는 라이브러리는 없다.**

### 2.2 태스크 그래프 (Beads 대안)

| 솔루션 | 설명 | 평가 |
|--------|------|------|
| **beads_rust** (Dicklesworthstone) | Beads의 Rust 재구현, 20K줄 CLI | CLI이므로 라이브러리로 임베딩 불가 |
| **petgraph** (v0.8.2) | Rust 표준 그래프 라이브러리, DAG 래퍼 | **핵심 빌딩 블록** |
| daggy (v0.9.0) | petgraph 위의 DAG 전용 래퍼 | petgraph보다 가벼운 API |
| **sha2 + base36** | 해시 ID 생성 | 크레이트 2개로 Beads 스타일 ID 구현 가능 |
| 기존 WorkItem | OpenGoose에 이미 존재 | **확장 기반으로 최적** |

**핵심 발견**: Beads 핵심 기능은 **~500줄 Rust**로 직접 구현 가능하다.

---

## 3. 권장 아키텍처: SQLite + 커스텀 버저닝 + 임베디드 Beads

### 3.1 전체 구조

```
┌─────────────────────────────────────────────────────┐
│                  OpenGoose 단일 바이너리              │
│                                                     │
│  ┌─────────────────────────────────────────────┐    │
│  │  opengoose-beads (신규 ~500줄)               │    │
│  │  - petgraph::StableGraph (태스크 DAG)        │    │
│  │  - sha2 + base36 (해시 ID)                   │    │
│  │  - ready(), prime(), compact()               │    │
│  └──────────────┬──────────────────────────────┘    │
│                 │                                    │
│  ┌──────────────▼──────────────────────────────┐    │
│  │  opengoose-persistence (기존 + 확장)         │    │
│  │  - Diesel ORM (SQLite)                       │    │
│  │  - 브랜치별 DB 파일 관리                      │    │
│  │  - Temporal 테이블 (시간여행)                 │    │
│  │  - Cell-level diff (변경 추적)                │    │
│  └──────────────┬──────────────────────────────┘    │
│                 │                                    │
│  ┌──────────────▼──────────────────────────────┐    │
│  │  SQLite (임베디드, 서버 없음)                 │    │
│  │  - main.db (메인 브랜치)                     │    │
│  │  - branches/agent-1.db (에이전트 브랜치)      │    │
│  │  - branches/agent-2.db (에이전트 브랜치)      │    │
│  └─────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────┘
```

### 3.2 데이터 브랜칭: DB-per-Branch 패턴

Dolt의 핵심 가치(에이전트별 격리, diff, merge, rollback)를 **SQLite 파일 분리**로 구현한다:

```
~/.opengoose/
├── main.db                    # 메인 브랜치 (프로덕션)
├── branches/
│   ├── agent-researcher.db    # 에이전트 브랜치 (main의 복사본)
│   ├── agent-analyst.db       # 에이전트 브랜치
│   └── _metadata.db           # 브랜치 메타데이터 (커밋 DAG)
```

#### 브랜치 생성 (SQLite VACUUM INTO)

```rust
// 에이전트 브랜치 생성: main.db → branches/agent-1.db 복사
// SQLite의 VACUUM INTO는 일관된 스냅샷을 원자적으로 생성
fn create_branch(main_db: &Path, branch_name: &str) -> Result<PathBuf> {
    let branch_path = branches_dir().join(format!("{branch_name}.db"));
    let conn = Connection::open(main_db)?;
    conn.execute(&format!("VACUUM INTO '{}'", branch_path.display()), [])?;
    // 메타데이터 DB에 브랜치 기록
    record_branch_creation(branch_name, get_current_commit_hash(main_db)?)?;
    Ok(branch_path)
}
```

#### Diff (테이블 비교)

```rust
// 두 브랜치 DB의 차이를 행 단위로 비교
fn diff_tables(base_db: &Path, branch_db: &Path, table: &str) -> Vec<RowDiff> {
    // ATTACH로 두 DB를 동시에 열고 비교
    // SELECT * FROM main.{table} EXCEPT SELECT * FROM branch.{table}
    // → 삭제된 행
    // SELECT * FROM branch.{table} EXCEPT SELECT * FROM main.{table}
    // → 추가/변경된 행
}
```

#### Merge (3-way)

```rust
// base(공통 조상) + ours(main) + theirs(branch) → 머지
fn merge_branch(main_db: &Path, branch_db: &Path, base_snapshot: &Path) -> MergeResult {
    let base_rows = read_all_rows(base_snapshot, table);
    let main_rows = read_all_rows(main_db, table);
    let branch_rows = read_all_rows(branch_db, table);

    // 3-way diff: base↔main, base↔branch
    // 같은 행의 같은 컬럼이 양쪽에서 변경 → 충돌
    // 그 외 → 자동 머지
}
```

#### Rollback (파일 삭제)

```rust
// 에이전트 환각 시 브랜치 즉시 폐기
fn discard_branch(branch_name: &str) -> Result<()> {
    let branch_path = branches_dir().join(format!("{branch_name}.db"));
    std::fs::remove_file(branch_path)?; // 즉시 롤백 완료
    Ok(())
}
```

### 3.3 시간여행: Temporal 테이블

```sql
-- 기존 work_items 테이블에 이력 추적 추가
CREATE TABLE work_items_history (
    history_id   INTEGER PRIMARY KEY AUTOINCREMENT,
    operation    TEXT NOT NULL,  -- 'INSERT', 'UPDATE', 'DELETE'
    changed_at   TEXT NOT NULL DEFAULT (datetime('now')),
    changed_by   TEXT,           -- 에이전트 이름
    -- 원본 컬럼 전체 복사
    id           INTEGER,
    title        TEXT,
    status       TEXT,
    -- ... 나머지 컬럼
);

-- UPDATE 트리거: 변경 전 상태를 이력에 기록
CREATE TRIGGER work_items_update_history
AFTER UPDATE ON work_items
BEGIN
    INSERT INTO work_items_history (operation, changed_by, id, title, status, ...)
    VALUES ('UPDATE', NEW.assigned_to, OLD.id, OLD.title, OLD.status, ...);
END;
```

```rust
// "3시간 전 상태" 조회
fn as_of(table: &str, timestamp: &str) -> Vec<Row> {
    // work_items_history에서 해당 시점의 상태를 재구성
    // 또는 valid_from/valid_to 패턴으로 직접 조회
}
```

### 3.4 임베디드 Beads: opengoose-beads 모듈

기존 `WorkItem`을 확장하여 Beads 핵심 기능을 ~500줄로 구현:

#### 해시 ID 생성

```rust
use sha2::{Sha256, Digest};

fn generate_bead_id(title: &str, creator: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(title.as_bytes());
    hasher.update(creator.as_bytes());
    hasher.update(SystemTime::now().duration_since(UNIX_EPOCH).unwrap()
        .as_nanos().to_le_bytes());
    let hash = hasher.finalize();
    let short = base36_encode(&hash[..4]); // 4바이트 → ~6자리
    format!("bd-{short}")
}
// 결과: "bd-k7m2x9" 같은 충돌 불가능한 ID
```

#### 중첩 경로

```rust
fn create_child(parent_id: &str, child_counter: &AtomicU32) -> String {
    let n = child_counter.fetch_add(1, Ordering::SeqCst);
    format!("{parent_id}.{}", n + 1)
}
// "bd-k7m2x9" → "bd-k7m2x9.1" → "bd-k7m2x9.1.1"
```

#### 핵심 API

```rust
pub struct BeadStore {
    graph: StableGraph<Bead, Relationship>,  // petgraph
    db: Arc<Database>,                        // 기존 SQLite
}

impl BeadStore {
    /// 블로킹 없는 실행 가능 태스크만 반환 (토큰 절약)
    pub fn ready(&self, session_key: &str) -> Vec<Bead>;

    /// 에이전트 세션 시작용 프로젝트 컨텍스트 생성
    pub fn prime(&self, session_key: &str) -> String;

    /// 오래된 태스크 요약으로 압축 (컨텍스트 윈도우 최적화)
    pub fn compact(&self, older_than: Duration) -> Vec<CompactedBead>;

    /// 태스크 생성 (해시 ID, 머지 충돌 불가)
    pub fn create(&self, title: &str, creator: &str) -> Bead;

    /// 원자적 할당 (경쟁 조건 방지)
    pub fn claim(&self, bead_id: &str, agent: &str) -> Result<()>;
}
```

### 3.5 Dolt 대비 트레이드오프

| 기능 | Dolt | SQLite + 커스텀 | 차이 |
|------|------|----------------|------|
| 브랜치 생성 | `CALL dolt_branch()` | `VACUUM INTO` (파일 복사) | Dolt: 즉시 (CoW), SQLite: DB 크기 비례 |
| 브랜치 스토리지 | Prolly Tree (중복 0) | 전체 DB 복사 | **SQLite가 브랜치당 DB 크기만큼 사용** |
| Merge | Cell-level 3-way 자동 | 커스텀 로직 구현 필요 | 구현 비용 있으나 충분히 가능 |
| Diff | `dolt_diff_<table>` 자동 | ATTACH + EXCEPT 쿼리 | 동일 결과, 약간 더 많은 코드 |
| 시간여행 | `AS OF` 쿼리 | Temporal 테이블 | 동일한 가치, 다른 구현 |
| 연합 동기화 | `dolt push/pull` | 없음 (추후 구현 필요) | **가장 큰 차이점** |
| 서버 필요 | Go 서버 프로세스 | **없음** | **SQLite의 핵심 장점** |
| 바이너리 크기 | +Dolt 바이너리 (~100MB) | +0 (이미 포함) | SQLite 완승 |
| Diesel 호환 | `mysql` feature | `sqlite` feature (현재) | 변경 없음 |

### 3.6 스토리지 효율성 보완

Dolt의 Prolly Tree는 브랜치 간 중복 데이터를 공유하지만, SQLite는 전체 DB를 복사한다.

**완화 전략:**

```
현실적 크기 추정:
- OpenGoose DB ≈ 10-50MB (13개 테이블, 수천 행)
- 에이전트 5개 동시 브랜치 = 50-250MB
- 에이전트 20개 동시 브랜치 = 200MB-1GB

→ 에이전트 20개 이하에서는 스토리지 문제 없음
→ 20개 이상에서는 경량 브랜치 전략 필요:
   - 변경된 테이블만 복사 (ATTACH + CREATE TABLE AS SELECT)
   - 또는 브랜치 수명 제한 (작업 완료 후 즉시 삭제)
```

---

## 4. 추가 고려: cr-sqlite로 연합 동기화

Dolt의 `push/pull`을 대체할 수 있는 유일한 임베디드 옵션이 **cr-sqlite**이다:

```
┌──────────────┐    cr-sqlite sync    ┌──────────────┐
│ OpenGoose A  │ ◀────────────────▶  │ OpenGoose B  │
│ (SQLite +    │    변경분만 전송      │ (SQLite +    │
│  cr-sqlite)  │    CRDT 자동 머지     │  cr-sqlite)  │
└──────────────┘                     └──────────────┘
```

- SQLite 확장으로 로드 (`sqlite3_crsqlite_init`)
- CRDT 기반 자동 충돌 해결 (Last-Write-Wins per column)
- Fly.io Corrosion에서 프로덕션 사용
- **단, Git 스타일 브랜칭이 아닌 eventual consistency**

**연합이 필요한 시점에 cr-sqlite를 추가하면 별도 서버 없이 분산 동기화 가능.**

---

## 5. 구현 우선순위

### Phase 1: 기반 (현재 가능)

1. **Store trait 추상화** — 현재 SQLite 직접 의존을 trait 기반으로
2. **Temporal 테이블** — `_history` 테이블 + 트리거 (시간여행)
3. **WorkItem 해시 ID** — `hash_id: TEXT` 컬럼 추가 (순차 ID와 병행)

### Phase 2: 브랜칭 (에이전트 5개+ 시)

4. **DB-per-Branch** — `VACUUM INTO`로 브랜치 생성/삭제
5. **Diff 엔진** — `ATTACH` + `EXCEPT` 기반 테이블 비교
6. **3-way Merge** — base/ours/theirs 행 단위 머지 로직

### Phase 3: Beads (태스크 관리 고도화 시)

7. **opengoose-beads 모듈** — petgraph + sha2 기반 태스크 그래프
8. **ready/prime/compact** — AI 에이전트 컨텍스트 최적화
9. **관계 타입** — `relates_to`, `supersedes`, `duplicates`

### Phase 4: 연합 (분산 필요 시)

10. **cr-sqlite 통합** — CRDT 기반 멀티 인스턴스 동기화

---

## 6. 최종 결론

```
질문: Dolt를 사용해야 하는가?
답변: 아니오. 단일 바이너리 제약에서는 SQLite + 커스텀 버저닝이 최적.
      Dolt의 핵심 가치 대부분을 SQLite 위에서 구현 가능.

질문: Beads를 사용해야 하는가?
답변: 아니오. Go CLI이므로 임베딩 불가.
      ~500줄 Rust로 핵심 기능 직접 구현 가능.
      beads_rust (Rust 재구현)은 CLI이므로 라이브러리로 사용 불가.

질문: 무엇을 사용하는가?
답변: SQLite (현재) + 3개 계층을 점진적 추가:
      ① Temporal 테이블 (시간여행)
      ② DB-per-Branch (에이전트 격리)
      ③ opengoose-beads (태스크 그래프)

질문: Dolt 대비 포기하는 것은?
답변: Prolly Tree 스토리지 효율성 (브랜치당 전체 DB 복사)
      → 에이전트 20개 이하에서는 문제 없음
      → 20개 이상에서는 경량 브랜치 전략으로 보완

      내장 push/pull 연합 (Wasteland 패턴)
      → cr-sqlite로 나중에 보완 가능

질문: Dolt 대비 얻는 것은?
답변: 별도 서버 불필요 (가장 큰 이점)
      바이너리 크기 -100MB
      Diesel SQLite 호환 유지 (마이그레이션 비용 0)
      기존 테스트 코드 전부 유지
      운영 복잡도 제로
```

---

## 참고: 주요 Rust 크레이트

| 크레이트 | 용도 | 비고 |
|---------|------|------|
| `diesel` (sqlite) | ORM, 현재 사용 중 | 변경 없음 |
| `rusqlite` | ATTACH/VACUUM INTO 등 Diesel 미지원 기능 | 선택적 추가 |
| `petgraph` | 태스크 DAG (Beads 그래프) | StableGraph 사용 |
| `sha2` | 해시 ID 생성 | 이미 의존성일 가능성 |
| `cr-sqlite` | 연합 동기화 (Phase 4) | C 확장, 필요 시 추가 |
| `prollytree` | 미래에 스토리지 효율 개선 시 | v0.3, 실험적 |
