# 스토리지 아키텍처: 단일 바이너리 + Prolly Tree

> **최종 결정:** 2026-03-12
> **핵심:** SQLite → **prollytree** 전환, 순수 Rust 단일 바이너리
> **통합된 문서:** dolt-deep-dive.md, dolt-beads-porting-guide.md, database-strategy.md, rust-native-embedded-alternatives.md

---

## 1. 아키텍처 결정 요약

### 제약 조건
- **단일 바이너리**: 외부 서버/프로세스 없음
- **순수 Rust**: C 의존성 최소화 (libsqlite3-sys 제거 목표)
- **Prolly Tree 효율성**: Dolt 수준의 구조적 공유와 O(diff) 시간 복잡도

### 최종 선택: `prollytree` 크레이트

| 기준 | SQLite + 커스텀 | **prollytree** | Dolt |
|------|:--------------:|:--------------:|:----:|
| 단일 바이너리 | ✅ | ✅ | ❌ (Go 서버) |
| 순수 Rust | ❌ (C 의존) | ✅ | ❌ |
| 구조적 공유 | ❌ | ✅ | ✅ |
| O(diff) 시간 | ❌ | ✅ | ✅ |
| 3-way Merge | 커스텀 필요 | ✅ 내장 | ✅ |
| Git 통합 | ❌ | ✅ | ✅ |
| SQL 지원 | ✅ Diesel | ✅ GlueSQL | ✅ MySQL |
| 라이선스 | MIT | Apache-2.0 | Apache-2.0 |

---

## 2. prollytree 선정 근거

### 2.1 Prolly Tree란?

**Probabilistic B-tree** — B-tree의 효율성과 Merkle tree의 무결성 검증을 결합한 자료구조.

```
[Root Node] ← 루트 해시 = 커밋이 가리키는 값
    ├── [Internal Node A]
    │   ├── [Leaf: row1, row2, row3]
    │   └── [Leaf: row4, row5]
    └── [Internal Node B]
        ├── [Leaf: row6, row7, row8]
        └── [Leaf: row9, row10]
```

**핵심 특성:**
- **Content-addressed**: 동일 데이터 → 동일 해시 → 자동 중복 제거
- **구조적 공유**: 100개 브랜치를 만들어도 변경된 부분만 추가 저장
- **역사 독립성**: 삽입 순서 무관하게 동일 데이터 → 동일 트리
- **O(변경) diff**: 같은 해시의 서브트리는 건너뜀

### 2.2 prollytree 크레이트 (v0.3.1)

**저장소:** https://github.com/zhangfengcdt/prollytree  
**라이선스:** Apache-2.0  
**상태:** 활발히 유지보수 중 (2025.08 최신 릴리스)

```toml
[dependencies]
prollytree = { version = "0.3.1", features = ["git", "sql"] }
```

**제공 기능:**
- Prolly Tree 핵심 (구조적 공유, O(diff))
- 3-way Merge + 5가지 Conflict Resolution 전략
- Git-backed 버전 관리 (branch, commit, merge)
- SQL 인터페이스 (GlueSQL)
- AI Agent 메모리 모듈
- RocksDB/File/InMemory 스토리지 백엔드

### 2.3 대안 비교

| 크레이트 | Stars | 라이선스 | 상태 | SQL | 3-way Merge |
|---------|-------|---------|------|:---:|:-----------:|
| **prollytree** | 25 | Apache-2.0 | 활발 | ✅ GlueSQL | ✅ |
| dialog-db | 129 | MPL-2.0 | 실험적 | ❌ Datalog | 불명확 |
| cr-sqlite | 3.7k | MIT | **중단 (2024.01)** | ✅ SQLite | CRDT만 |
| sqlite-sync | 410 | - | 활발 | ✅ SQLite | CRDT |

**cr-sqlite 탈락 이유:**
- 마지막 릴리스 2024.01.17 이후 업데이트 없음
- SQLite Cloud 의존 (외부 서비스)

**dialog-db 보류 이유:**
- "실험적" 명시, 마이그레이션 보장 없음
- crates.io 미게시, 문서 부족
- 17개 크레이트의 복잡한 구조

---

## 3. 아키텍처 설계

### 3.1 전체 구조

```
┌─────────────────────────────────────────────────────────────┐
│                    OpenGoose 단일 바이너리                    │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌─────────────────────┐  ┌─────────────────────┐           │
│  │  Beads 알고리즘 레이어  │  │    VCS 레이어        │           │
│  │  (자체 구현 ~500줄)    │  │                     │           │
│  │                     │  │  • branch()         │           │
│  │  • ready()          │  │  • commit()         │           │
│  │  • prime()          │  │  • merge() (3-way)  │           │
│  │  • compact()        │  │  • diff() O(변경)    │           │
│  │  • hash_id()        │  │                     │           │
│  └──────────┬──────────┘  └──────────┬──────────┘           │
│             │                        │                      │
│             └────────────┬───────────┘                      │
│                          │                                  │
│             ┌────────────▼───────────┐                      │
│             │      prollytree        │  ← Apache-2.0        │
│             │  (Prolly Tree 엔진)     │                      │
│             │                        │                      │
│             │  • 구조적 공유           │                      │
│             │  • Content-addressed   │                      │
│             │  • ConflictResolver    │                      │
│             │  • GitVersionedStore   │                      │
│             └────────────┬───────────┘                      │
│                          │                                  │
│             ┌────────────▼───────────┐                      │
│             │   Storage Backends     │                      │
│             │                        │                      │
│             │  • InMemory (테스트)    │                      │
│             │  • File (기본)          │                      │
│             │  • Git (버전 관리)       │                      │
│             └────────────────────────┘                      │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### 3.2 데이터 모델 매핑

**현재 SQLite 테이블 → Prolly Tree Key-Value:**

```
work_items 테이블:
  Key:   "work_item:{hash_id}"
  Value: JSON { title, status, assigned_to, parent_id, ... }

relationships 테이블 (신규):
  Key:   "rel:{child_id}:{parent_id}"
  Value: JSON { kind: "blocks" | "depends_on" | "supersedes" }

agent_memories 테이블 (신규):
  Key:   "memory:{agent_id}:{key}"
  Value: String (기억 내용)
```

### 3.3 Conflict Resolution 전략

prollytree 내장 5가지 + 커스텀:

```rust
use prollytree::diff::{
    ConflictResolver,
    TimestampResolver,      // 타임스탬프 기반
    AgentPriorityResolver,  // 에이전트 우선순위 기반
    SemanticMergeResolver,  // JSON 시맨틱 머지
};

// OpenGoose 커스텀: 작업 상태 기반 해결
pub struct WorkItemStatusResolver;

impl ConflictResolver for WorkItemStatusResolver {
    fn resolve(&self, conflict: &MergeConflict) -> Resolution {
        // completed > in_progress > pending 우선순위
    }
}
```

---

## 4. 마이그레이션 계획

### Phase 1: PoC 및 평가 (1주)

```rust
// prollytree 기본 동작 확인
use prollytree::tree::{ProllyTree, Tree};
use prollytree::storage::InMemoryNodeStorage;

let storage = InMemoryNodeStorage::<32>::new();
let mut tree = ProllyTree::new(storage, Default::default());

// WorkItem 저장
tree.insert(
    b"work_item:bd-k7m2x9".to_vec(),
    serde_json::to_vec(&work_item)?,
);

// 브랜치 생성 및 머지 테스트
```

**평가 기준:**
- [ ] INSERT/UPDATE 성능 (SQLite 대비)
- [ ] 브랜치 생성/머지 성능
- [ ] 메모리 사용량
- [ ] GlueSQL 쿼리 호환성

### Phase 2: 데이터 레이어 전환 (2주)

1. **WorkItem → Prolly Tree 매핑** 구현
2. **관계 그래프** (petgraph 통합)
3. **ready/prime/compact** 알고리즘 구현
4. 기존 테스트 포팅

### Phase 3: VCS 통합 (1주)

1. Git-backed 스토리지 활성화
2. branch/commit/merge API
3. Landing the Plane 프로토콜
4. 통합 테스트

### Phase 4: Dual-Write 마이그레이션 (선택)

기존 SQLite 데이터가 있는 경우:
1. SQLite ↔ ProllyTree 동시 쓰기
2. 데이터 일관성 검증
3. SQLite 제거

---

## 5. Beads 핵심 알고리즘 (~500줄)

prollytree 위에 구현할 Beads 기능:

### 5.1 해시 ID 생성

```rust
use sha2::{Sha256, Digest};

pub fn generate_bead_id(title: &str, creator: &str) -> String {
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
    format!("bd-{}", base36_encode(value))
}
```

### 5.2 ready() — 실행 가능한 태스크

```rust
pub fn ready(&self, session_key: Option<&str>) -> Vec<WorkItem> {
    self.tree.iter()
        .filter(|(k, _)| k.starts_with(b"work_item:"))
        .map(|(_, v)| serde_json::from_slice::<WorkItem>(v).unwrap())
        .filter(|item| {
            item.status == WorkStatus::Pending
            && !item.is_ephemeral
            && !self.is_blocked(&item.hash_id)
            && session_key.map_or(true, |s| 
                item.session_key.as_deref() == Some(s) || item.session_key.is_none()
            )
        })
        .sorted_by_key(|item| item.priority)
        .take(self.config.batch_size)
        .collect()
}
```

### 5.3 prime() — AI 컨텍스트 생성

```rust
pub fn prime(&self, session_key: &str) -> String {
    let active = self.list_by_status(WorkStatus::InProgress);
    let ready = self.ready(Some(session_key));
    let recent = self.recent_completions(Duration::hours(24));
    let blocked = self.list_blocked();
    let memories = self.agent_memories(session_key);
    let last_landing = self.last_landing_report(session_key);

    // BriefIssue 포맷 (97% 토큰 절감)
    format!(
        "# Active ({}):\n{}\n\n# Ready ({}):\n{}\n\n# Recent:\n{}\n\n# Blocked:\n{}\n\n# Memory:\n{}\n\n# Last Landing:\n{}",
        active.len(), format_brief(&active),
        ready.len(), format_brief(&ready),
        format_brief(&recent),
        format_brief(&blocked),
        format_memories(&memories),
        last_landing.unwrap_or_default()
    )
}
```

### 5.4 compact() — 오래된 태스크 요약

```rust
pub fn compact(&self, older_than: Duration) -> Vec<CompactedBead> {
    let cutoff = Utc::now() - older_than;
    
    self.list_by_status(WorkStatus::Completed)
        .into_iter()
        .filter(|item| item.completed_at.map_or(false, |t| t < cutoff))
        .filter(|item| !item.is_compacted)
        .group_by(|item| item.parent_id.clone())
        .map(|(parent, items)| {
            let summary = summarize_completions(&items);
            CompactedBead {
                parent_id: parent,
                summary,
                original_count: items.len(),
                compacted_at: Utc::now(),
            }
        })
        .collect()
}
```

---

## 6. 이전 분석 문서 참조

이 문서에 통합된 원본 분석:

| 원본 문서 | 핵심 내용 | 현재 상태 |
|----------|---------|----------|
| dolt-deep-dive.md | Dolt Prolly Tree, 멀티에이전트 패턴 | 삭제 예정 |
| dolt-beads-porting-guide.md | Beads 알고리즘, 테스트 계획 | 삭제 예정 |
| database-strategy.md | DB 옵션 비교, 마이그레이션 전략 | 삭제 예정 |
| rust-native-embedded-alternatives.md | 임베디드 대안 분석 | 삭제 예정 |

---

## 7. 평가 결과 (2026-03-12 실행)

### 7.1 prollytree v0.3.1 빌드 테스트 결과

```
❌ 빌드 실패 — 167개 컴파일 에러
원인: gluesql-core v0.15.0의 bigdecimal 버전 비호환
      + prollytree 자체 serde/타입 에러 다수
테스트 설정:
  - Rust edition 2024
  - default-features = false, features = ["git"]
  - gluesql-core가 비-optional 의존성으로 항상 포함됨
```

### 7.2 현재 결정: SQLite/Diesel 유지 + 추상화 준비

prollytree가 컴파일 불가하므로, **현재 SQLite/Diesel을 유지**하되:

1. **Beads 핵심 4기능은 이미 구현 완료:**
   - `hash_id()` — SHA-256 + base36, 적응형 길이 (`crates/opengoose-persistence/src/hash_id.rs`)
   - `ready()` — 의존성 인식 실행 가능 태스크 필터 (`ready.rs`)
   - `prime()` — 에이전트 컨텍스트 생성 (`prime.rs`)
   - `compact()` — 완료 태스크 요약 + 다이제스트 (`compact.rs`)

2. **petgraph v0.7 추가 완료** — 향후 관계 그래프 고도화용

3. **prollytree는 Cargo.toml에 주석 처리** — upstream 수정 시 재평가

### 7.3 대안 경로

| 대안 | 상태 | 비고 |
|------|------|------|
| prollytree upstream PR | 대기 | gluesql-core 의존성 optional화 필요 |
| SQLite + 자체 Prolly Tree 구현 | 보류 | 현재 SQLite 성능 충분, 필요 시 검토 |
| redb (순수 Rust 임베디드 KV) | 후보 | Prolly Tree 아님, 단순 KV |
| fjall (LSM-tree, 순수 Rust) | 후보 | 성숙도 확인 필요 |

---

## 8. 결론

```
질문: 왜 prollytree인가?
답변: 순수 Rust + Prolly Tree 효율성 + 3-way Merge 내장 + 단일 크레이트
      → 단, v0.3.1이 컴파일 불가. upstream 수정 대기.

질문: SQLite/Diesel은?
답변: 현재 유지. Beads 4기능(hash_id/ready/prime/compact) 이미 구현 완료.
      prollytree 안정화 시 점진적 전환.

질문: Dolt 대비 포기하는 것?
답변: MySQL 프로토콜 호환성 (불필요)
      DoltHub/DoltLab 동기화 (불필요)

질문: Dolt 대비 얻는 것?
답변: 순수 Rust (C 의존성 제거) — prollytree 안정화 시
      단일 바이너리 (외부 서버 불필요)
      바이너리 크기 -100MB
      운영 복잡도 제로

질문: beads_rust는?
답변: 사용 불가 (Anthropic Rider 라이선스로 Anthropic/OpenAI 사용 금지)
      알고리즘 아이디어만 참조하여 자체 구현 → 완료
```
