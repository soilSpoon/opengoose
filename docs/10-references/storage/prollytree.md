# Prollytree Reference

Prolly Trees (Probabilistic B-trees) combine the search efficiency of B-trees with the integrity and structural sharing of Merkle trees.

## Core Features

- **History Independence**: The same set of data results in the same tree structure regardless of insertion order.
- **Efficient Diffing**: O(diff) performance for comparing database states.
- **Structural Sharing**: Changes create new nodes while pointing to unchanged existing nodes.
- **3-way Merge**: Built-in support for merging concurrent changes from different agents.

## Implementation in OpenGoose

OpenGoose utilizes the `prollytree` crate (v0.3.2-beta) with the `git` feature enabled.

### Dependency Specification

```toml
# Cargo.toml (workspace)
prollytree = { git = "https://github.com/zhangfengcdt/prollytree.git", default-features = false, features = ["git"] }
```

**주의사항:**
- crates.io v0.3.1은 빌드 실패 (167개 에러, `#[cfg(feature = "agent")]` 게이트 누락)
- GitHub main (v0.3.2-beta, 커밋 `a969843`)에서만 빌드 성공
- `sql` feature는 비활성화 (GlueSQL 미포함 — 현재 불필요)
- `agent` feature는 비활성화 (OpenGoose가 자체 에이전트 관리)

### OpenGoose 래퍼: `opengoose-prolly` (749줄, 24개 테스트)

#### ProllyStore (store.rs, 483줄)

```rust
// 데이터 모델
pub struct ProllyWorkItem {
    pub hash_id: String,
    pub session_key: String,
    pub team_run_id: String,
    pub parent_hash_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub assigned_to: Option<String>,
    pub priority: i32,
    pub is_ephemeral: bool,
    pub created_at: String,
    pub updated_at: String,
}

// CRUD
insert_work_item(&mut self, item: &ProllyWorkItem) -> Result<()>
get_work_item(&self, hash_id: &str) -> Result<Option<ProllyWorkItem>>
update_work_item(&mut self, item: &ProllyWorkItem) -> Result<()>
delete_work_item(&mut self, hash_id: &str) -> Result<()>

// 조회
list_work_items(&self) -> Result<Vec<ProllyWorkItem>>
list_by_status(&self, status: &str) -> Result<Vec<ProllyWorkItem>>
list_for_run(&self, team_run_id: &str) -> Result<Vec<ProllyWorkItem>>

// 관계
insert_relationship(&mut self, from: &str, to: &str, kind: &str) -> Result<()>
is_blocked(&self, hash_id: &str) -> Result<bool>

// Merkle 증명
root_hash(&self) -> Result<Vec<u8>>
diff(&self, other: &Self) -> Result<Vec<DiffEntry>>
stats(&self) -> Result<TreeStats>
```

**스토리지 백엔드:**
- `InMemoryProllyStore`: 테스트용 (인메모리)
- `FileProllyStore`: 프로덕션 (파일 기반, `file_store(dir: PathBuf)`)

#### VersionedWorkItemStore (versioned.rs, 246줄)

```rust
// Git-backed 버전 관리
VersionedWorkItemStore::init(repo_path: &Path) -> Result<Self>
insert(&mut self, key: &str, value: &str) -> Result<()>  // staged
commit(&mut self, message: &str) -> Result<String>         // → commit hash
create_branch(&mut self, name: &str) -> Result<()>
current_branch(&self) -> Result<String>
list_branches(&self) -> Result<Vec<String>>
log(&self) -> Result<Vec<CommitInfo>>
status(&self) -> Result<Vec<StatusEntry>>
```

### Conflict Resolution

OpenGoose implements `WorkItemStatusResolver` for automatic conflict resolution:

```rust
impl ConflictResolver for WorkItemStatusResolver {
    // 작업 상태 우선순위:
    // completed > failed > in_progress > cancelled > pending > compacted
    // 더 "진행된" 상태가 승리
}
```

prollytree 내장 5가지 전략도 사용 가능:
- `TimestampResolver`: Last write wins based on time
- `AgentPriorityResolver`: Higher priority agent's change wins
- `SemanticMergeResolver`: Intelligently merges JSON objects
- 기타 2개 (custom impl 가능)

### 테스트 커버리지 (24개)

**store.rs (16개):**
- 기본 CRUD (insert, get, update, delete, nonexistent)
- 조회 (list, by_status, for_run)
- Merkle 증명 (root_hash_changes, same_data_same_hash)
- Diff (diff_between_stores)
- 관계 (relationship_and_blocking)
- 충돌 해결 (conflict_resolver)
- 파일 지속성 (file_store_persistence)
- 대량 삽입 (batch_insert — 100개)

**versioned.rs (8개):**
- Git 커밋, 브랜칭, 로그, 상태

### 현재 상태 (2026-03-13)

| 항목 | 상태 |
|---|---|
| 빌드 | ✅ GitHub main에서 성공 |
| 래퍼 크레이트 | ✅ 완성 (749줄) |
| 테스트 | ✅ 24개 통과 |
| 프로덕션 적용 | ❌ SQLite와 병행 유지 |
| 벤치마크 | ❌ 미실시 (Phase 2 예정) |
| crates.io stable | ❌ 대기 중 |

### Dolt 대비 트레이드오프

| | Dolt | prollytree |
|---|---|---|
| **셀 레벨 머지** | ✅ (MySQL 프로토콜) | ⚠️ KV 레벨 (JSON 필드별은 SemanticMergeResolver) |
| **SQL 완전 호환** | ✅ MySQL 프로토콜 | ⚠️ GlueSQL (제한적) |
| **단일 바이너리** | ❌ Go 서버 필요 | ✅ |
| **순수 Rust** | ❌ | ✅ |
| **서버 운영** | 필요 (168MB→41MB) | 불필요 |
| **구조적 공유** | ✅ | ✅ |
| **3-way Merge** | ✅ | ✅ |

---

*Source: [github.com/zhangfengcdt/prollytree](https://github.com/zhangfengcdt/prollytree)*
