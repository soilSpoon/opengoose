# dolthub/dolt — 소스 코드 분석

> 분석일: 2026-03-11
> 레포: https://github.com/dolthub/dolt
> 언어: Go | 라이선스: Apache 2.0

---

## 1. 프로젝트 개요

Dolt는 **"데이터를 위한 Git"** — Git 시맨틱을 가진 SQL 데이터베이스다. Fork, branch, merge, pull request를 **구조화된 데이터**에 대해 수행할 수 있다. MySQL 호환 프로토콜을 사용하며, Beads와 Wasteland의 핵심 인프라 레이어로 사용된다.

> "Dolt는 Git 시맨틱을 가진 SQL 데이터베이스다. Fork, branch, merge, pull request — 구조화된 데이터에 대해. 이것이 전체 연합 트릭을 작동하게 한다." — Steve Yegge

## 2. Dolt vs 일반 SQL 데이터베이스

| 기능 | 일반 DB (MySQL, PostgreSQL) | Dolt |
|------|---------------------------|------|
| 데이터 변경 | 변경 가능, 버전 없음 | 모든 변경이 Git 같은 커밋으로 버전 관리 |
| 분기(branching) | 불가 | 완전한 브랜칭, 각 브랜치가 격리된 DB |
| 머지 | 불가 | **셀 레벨** 3-way 머지 (Git의 라인 레벨보다 정밀) |
| 충돌 감지 | N/A | 같은 셀(행, 열 쌍)을 다르게 수정하면 충돌 기록 |
| 이력 | 로그만 (별도 관리 필요) | 전체 커밋 히스토리 내장 |
| clone/fork | 불가 | 전체 DB를 히스토리째 복제 가능 |
| diff | 불가 | diff 크기에 비례하는 시간으로 계산 (전체 데이터 아님) |

## 3. 소스 코드 구조

```
dolthub/dolt/go/
├── cmd/dolt/
│   ├── dolt.go                    ← CLI 진입점
│   ├── commands/
│   │   ├── merge.go               ← CLI merge 커맨드
│   │   ├── push.go, pull.go       ← 리모트 동기화
│   │   └── sqlserver/
│   │       ├── sqlserver.go       ← SQL 서버 진입점 (MySQL 호환)
│   │       └── mcp.go             ← MCP 서버 통합 (2025년 추가)
├── libraries/doltcore/            ← 핵심 DB 로직 (~77,519줄)
│   ├── merge/
│   │   ├── merge.go               ← 3-way 머지 오케스트레이션
│   │   ├── merge_prolly_rows.go   ← Prolly tree 머지 (~2,300줄)
│   │   └── merge_schema.go        ← 스키마 충돌 해결 (~1,800줄)
│   └── sqle/dprocedures/
│       ├── dolt_commit.go         ← CALL dolt_commit() 구현
│       ├── dolt_merge.go          ← CALL dolt_merge() 구현
│       └── dolt_branch.go         ← CALL dolt_branch() 구현
├── store/prolly/                  ← Prolly Tree 데이터 구조
│   ├── artifact_map.go            ← 충돌/위반 아티팩트 저장
│   └── tree/
│       └── three_way_differ.go    ← 8상태 상태 머신 3-way diff
└── proto/                         ← gRPC 리모트 API (ChunkStoreService)
```

## 4. Git 시맨틱 — SQL 프로시저로 버전 관리

```sql
-- 브랜치 생성/전환
CALL dolt_branch('feature-x');
CALL dolt_checkout('feature-x');

-- 에이전트들이 각자 브랜치에서 작업 수행

-- 스테이징 + 커밋
CALL dolt_add('.');
CALL dolt_commit('-m', 'agent work');

-- 머지 (셀 레벨 충돌 감지)
CALL dolt_merge('main', '--no-ff');
```

## 5. 머지 엔진 — 3단계 충돌 감지

### 머지 결과 구조체 (merge.go)

```go
type Result struct {
    Root                  doltdb.RootValue       // 머지된 DB 상태
    SchemaConflicts       []SchemaConflict       // 스키마 충돌
    Stats                 map[TableName]*MergeStats // 테이블별 머지 통계
}
```

### 3-way Differ (three_way_differ.go)

- **8상태 상태 머신**: Init → Compare → NewLeft/NewRight/Match → ...
- base, left, right 3개 버전을 동시 순회하며 **셀 레벨** 충돌 감지
- Git의 라인 레벨보다 정밀한 데이터 충돌 감지

### 충돌 아티팩트 유형 (artifact_map.go)

```go
const (
    ArtifactTypeConflict       // 데이터 충돌
    ArtifactTypeForeignKeyViol // FK 위반
    ArtifactTypeUniqueKeyViol  // 유니크 키 위반
    ArtifactTypeChkConsViol    // CHECK 제약 위반
    ArtifactTypeNullViol       // NULL 위반
)
```

### 충돌 조회 — 시스템 테이블

- `dolt_conflicts_<table>` — 충돌 행 목록
- `dolt_merge_conflicts_<table>` — left/right/base 셀 값
- `dolt_constraint_violations` — 제약 위반 목록
- `dolt_history_<table>` — 커밋 히스토리 조회

## 6. Prolly Trees — 효율적 저장 (store/prolly/)

- **영속적 B+ 트리**: 불변 노드, 콘텐츠 해시로 식별
- **Content-addressed 저장**: 유사한 데이터는 한 번만 저장, 자동 중복 제거
- 브랜칭의 저장 오버헤드 최소
- 적당한 하드웨어에서 100+ 브랜치 지원
- diff 계산이 diff 크기에 비례 (전체 데이터 크기가 아님)

## 7. 리모트 동기화 — gRPC 청크 전송

```protobuf
// chunkstore.proto — 변경된 청크만 전송 (전체 DB 아님)
service ChunkStoreService {
  rpc HasChunks(...)          // 리모트에 있는 청크 확인
  rpc GetDownloadLocations()  // 누락 청크 다운로드
  rpc GetUploadLocations()    // 새 청크 업로드
  rpc Commit(...)             // 커밋 완료
}
```

- Git의 packfile과 유사한 효율적 전송
- 전체 DB가 아닌 **변경된 청크만** 전송

## 8. MCP 통합 — AI 에이전트 직접 연결 (2025년 추가)

```go
// mcp.go — MCP 서버 설정
type MCPConfig struct {
    Port     *int     // MCP HTTP 서버 포트
    User     *string  // DB 사용자
    Password *string  // DB 비밀번호
    Database *string  // 기본 DB
}
// 의존성: github.com/dolthub/dolt-mcp v0.3.4
//         github.com/mark3labs/mcp-go v0.34.0
```

MCP를 통해 AI 에이전트가 직접:
- DB 쿼리 실행, 스토어드 프로시저로 버전 관리
- 머지 수행 및 충돌 해결
- diff/커밋 히스토리 조회

## 9. Gas Town에서의 사용

```
포트 3307 (MySQL 프로토콜)
단일 서버 per Gas Town
├── hq DB (본부 — 글로벌 상태)
├── beads DB (에이전트 메모리/작업 추적)
├── gastown DB (타운 메타데이터)
└── crew DB (인간 워크스페이스)

다중 Polecat이 동시에 쓰기 (server_mode: true)
각 Polecat은 자체 Git worktree에서 격리
```

- **"Re-imagine" 메커니즘**: 충돌 시 "ours or theirs" 대신 새 코드베이스에 맞게 구현을 재설계
- Dolt가 SQLite/JSONL 백엔드의 모든 잔버그 제거

## 10. Wasteland에서의 사용

```
DoltHub: hop/wl-commons (공개 레퍼런스 DB)
├── Wanted Board (작업 게시)
├── Completions (작업 증거)
├── Stamps (다차원 증명)
└── Trust Ladder (평판 레벨)

연합 워크플로우:
1. wl-commons 포크 → 자체 DoltHub 조직에
2. 로컬 클론 (.wasteland/)
3. 작업 → wl sync로 동기화
```

## 11. 왜 Dolt가 멀티 에이전트 시스템에 이상적인가?

| 기능 | 멀티 에이전트 이점 |
|------|------------------|
| **브랜치 격리** | 각 에이전트가 자체 브랜치에서 작업 → 락 대기 없는 진정한 병렬성 |
| **셀 레벨 충돌 감지** | 라인 레벨(Git)보다 정밀한 데이터 충돌 감지 |
| **Append-only 이력** | 데이터를 되돌릴 수 없음 → 완전한 감사 추적 + 평판 시스템에 이상적 |
| **Clone 지원** | 유일한 clone 가능 SQL DB → 에이전트별 격리된 전체 DB 복제 |
| **에이전트 친화적** | 모델들이 Git을 잘 알므로 dolt branch/merge/commit도 빠르게 학습 |
| **서버 모드 동시성** | MySQL 프로토콜로 다수 에이전트가 동시 쓰기 (ACID 보장) |
| **효율적 diff** | diff 크기에 비례하는 계산 → 대규모 데이터에서도 빠른 비교 |

## 12. DoltHub — 데이터 협업 플랫폼

- 공개 Dolt DB 무료 호스팅
- 데이터에 대한 Pull Request (코드가 아닌 데이터!)
- Hosted Dolt: AWS/GCP에 완전 관리형 배포
- **연합 가능**: 각 조직이 동일 스키마의 독립 DB를 포크 → push/pull로 동기화

## 13. DoltHub 팀의 Gas Town 지원

> "Tim Sehn, DoltHub 창업자이자 CEO는 Beads와 Gas Town의 기능 및 버그 수정을 놀랍도록 빠르게 지원했고, 그의 팀은 Gas Town Discord에서 매일 활동한다."

주요 DoltHub 블로그:
- "Agentic Systems Need Version Control" (2025.10)
- "A Day in Gas Town" (2026.01)
- "Connect Agents to Hosted Dolt via MCP" (2026.02)
- "Agents Need Branches: UC Berkeley CS Edition" (2025.09)
