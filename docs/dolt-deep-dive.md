# Dolt 심층 분석: 멀티에이전트 AI를 위한 버전 관리 데이터베이스

> **분석일:** 2026-03-11
> **대상 프로젝트:** OpenGoose
> **문서 유형:** 기술 분석서
> **관련 문서:** [database-strategy.md](./database-strategy.md), [opengoose-v2-architecture.md](./opengoose-v2-architecture.md)

---

## 목차

1. [Dolt란 무엇인가](#1-dolt란-무엇인가)
2. [OpenGoose 현재 DB 현황](#2-opengoose-현재-db-현황)
3. [Dolt로 대체 가능한가?](#3-dolt로-대체-가능한가)
4. [Git 대체/보완 도구 비교: Dolt vs Entire.io vs jj vs GitButler](#4-git-대체보완-도구-비교)
5. [Beads란 무엇인가](#5-beads란-무엇인가)
6. [Wasteland는 어떻게 사용하는가](#6-wasteland는-어떻게-사용하는가)
7. [Dolt + PostgreSQL 조합 분석](#7-dolt--postgresql-조합-분석)
8. [OpenGoose에 가장 잘 맞는 선택](#8-opengoose에-가장-잘-맞는-선택)

---

## 1. Dolt란 무엇인가

### 1.1 핵심 정의

Dolt는 **"Git for Data"** — SQL 데이터베이스에 Git 스타일 버전 관리(branch, merge, diff, clone, push/pull)를 적용한 세계 유일의 데이터 머지 DB이다.

```
┌─────────────────────────────────────────────┐
│                   Dolt                       │
│                                             │
│   MySQL 와이어 프로토콜  +  Git 시맨틱스      │
│                                             │
│   ┌─────────┐   ┌─────────┐   ┌──────────┐ │
│   │ branch  │   │  merge  │   │  clone   │ │
│   │ commit  │   │  diff   │   │ push/pull│ │
│   └─────────┘   └─────────┘   └──────────┘ │
│                                             │
│   스토리지: Prolly Tree (content-addressed)   │
└─────────────────────────────────────────────┘
```

### 1.2 Prolly Tree 아키텍처

Dolt의 스토리지 엔진은 **Prolly Tree** (Probabilistic B-Tree)를 사용한다:

- **Content-addressed**: 데이터의 해시가 주소 → 동일 데이터는 자동 중복 제거
- **구조적 공유**: 100개 브랜치를 만들어도 변경된 부분만 추가 저장
- **Cell-level diff**: 행(row) 단위가 아닌 셀(cell) 단위로 변경 추적
- **3-way merge**: 공통 조상 + 양쪽 변경으로 자동 머지

```
main 브랜치:  [A1][B2][C3][D4]
                         │
agent-1 브랜치: [A1][B2][C9][D4]  ← C3→C9만 변경, 나머지 공유
                         │
agent-2 브랜치: [A1][B7][C3][D4]  ← B2→B7만 변경, 나머지 공유
                         │
머지 결과:     [A1][B7][C9][D4]  ← 셀 레벨 자동 머지 (충돌 없음)
```

### 1.3 MySQL 호환성

- **MySQL 와이어 프로토콜**: 기존 MySQL 클라이언트, ORM, 도구 그대로 사용
- **SQL 문법**: 표준 MySQL SQL + Dolt 확장 프로시저
- **Diesel 호환**: `diesel(mysql)` 피처 플래그로 Rust에서 사용 가능

### 1.4 핵심 기능

| 기능 | SQL 명령 | 설명 |
|------|---------|------|
| 브랜치 생성 | `CALL dolt_branch('agent-1')` | 격리된 데이터 공간 생성 |
| 브랜치 전환 | `CALL dolt_checkout('agent-1')` | 에이전트가 자기 브랜치에서 작업 |
| 커밋 | `CALL dolt_commit('-Am', 'task done')` | 스냅샷 저장 |
| 머지 | `CALL dolt_merge('agent-1')` | 변경 사항 통합 |
| 디프 | `SELECT * FROM dolt_diff_work_items` | 변경 내용 확인 |
| 시간 여행 | `SELECT * FROM work_items AS OF 'abc123'` | 과거 상태 조회 |
| 이력 조회 | `SELECT * FROM dolt_history_work_items` | 전체 변경 이력 |
| 충돌 확인 | `SELECT * FROM dolt_conflicts_work_items` | 머지 충돌 목록 |
| 클론 | `dolt clone myorg/mydb` | 전체 DB 히스토리 포함 복제 |
| 푸시/풀 | `dolt push origin main` | 리모트 동기화 |

### 1.5 시스템 테이블

Dolt는 모든 테이블에 대해 자동으로 시스템 테이블을 생성한다:

- `dolt_branches` — 모든 브랜치 목록
- `dolt_log` — 커밋 이력 (Git log와 동일)
- `dolt_diff_<table>` — 두 커밋 간 셀 레벨 변경 사항
- `dolt_history_<table>` — 모든 커밋에 걸친 행의 전체 이력
- `dolt_conflicts_<table>` — 머지 충돌 상세
- `dolt_status` — 현재 작업 디렉토리 상태

### 1.6 MCP 서버

Dolt는 **MCP (Model Context Protocol) 서버**를 제공하여 AI 에이전트가 직접 DB 버전 관리를 수행할 수 있다:

```json
{
  "mcpServers": {
    "dolt": {
      "command": "dolt-mcp-server",
      "args": ["--db-path", "/data/opengoose"]
    }
  }
}
```

에이전트가 사용 가능한 MCP 도구:
- `create_branch`, `checkout_branch`, `merge_branch`
- `commit_changes`, `view_diff`, `view_log`
- `execute_query`, `view_schema`

### 1.7 성능 특성

| 벤치마크 | Dolt vs MySQL | 비고 |
|---------|-------------|------|
| 쓰기 (INSERT/UPDATE) | +10% 빠름 | Prolly Tree 구조 이점 |
| 읽기 (SELECT) | -30% 느림 | 해시 탐색 오버헤드 (개선 중) |
| TPC-C 처리량 | MySQL의 40% | 높은 트랜잭션 부하에서 |
| sysbench (2025) | MySQL과 동등 | 최근 대폭 개선 |

**읽기 성능 개선 추이**: Dolt 초기에는 MySQL 대비 10x 느렸으나, 2025년 sysbench 기준 동등 수준까지 개선되었다.

---

## 1.8 Dolt가 멀티에이전트 AI에 최적인 이유

UC Berkeley CS 논문 "Redesigning Data Systems to be Agent-First" + DoltHub 공식 논거:

### 문제: 에이전트는 인간과 다르게 데이터를 다룬다

- 에이전트는 **환각(hallucination)**으로 수천 행을 한 번에 오염시킬 수 있다
- 인간 대비 **50배 더 많은 롤백**이 필요 (Neon 실측치)
- 여러 에이전트가 **동시에** 같은 테이블을 수정한다
- 에이전트의 행동을 **사후 검증**해야 한다 (diff로)

### Dolt만이 제공하는 해결책

| 문제 | Dolt 해결책 | PostgreSQL 대안 |
|------|-----------|----------------|
| 에이전트 격리 | `dolt_branch('agent-1')` | 트랜잭션 (제한적) |
| 변경 가시성 | `dolt_diff_<table>` (셀 단위) | audit 트리거 (수동 구현) |
| 즉시 롤백 | `dolt_reset('--hard')` | `SAVEPOINT` (세션 내만) |
| 합의 기반 머지 | 1000개 브랜치 중 99% 동일 → 자동 | 불가 |
| 효율적 스토리지 | Prolly Tree (변경분만) | 브랜치 개념 없음 |

### 핵심 패턴: Branch-per-Agent

```
main ─────────────────────────────────────── main
  │                                           ▲
  ├── agent-researcher ──┐                    │
  │   (데이터 수집)       ├── dolt_merge ──────┤
  ├── agent-analyst ─────┘                    │
  │   (데이터 분석)                            │
  ├── agent-writer ────── 검증 실패 → 폐기     │
  │   (환각 발생!)                             │
  └── agent-reviewer ──── 검증 통과 → merge ───┘
```

---

## 1.9 doltgres — PostgreSQL 호환 Dolt

DoltHub이 개발 중인 **Dolt의 PostgreSQL 버전**.

| 항목 | 상태 |
|------|------|
| 출시 | Beta (2025.04) |
| 1.0 목표 | 2026.04 (약 1개월 후) |
| 아키텍처 | Postgres SQL → AST → Dolt 엔진 (동일 스토리지 포맷) |
| 트리거 | 완료 |
| 저장 프로시저 | 거의 완료 (마지막 대형 기능) |
| 확장(Extensions) | Alpha |
| DoltHub/DoltLab 푸시 | 미지원 (커스텀 리모트만) |
| pgdump 호환 | 진행 중 (1.0 기준: 모든 pgdump 임포트 가능) |
| Forward Storage 호환 | 안정화 중 (1.x 내 마이그레이션 불필요 보장) |

**OpenGoose 맥락**: doltgres가 1.0이 되면 Diesel `postgres` + 버전 관리가 동시에 가능해진다. 현재는 Dolt(MySQL) + Diesel `mysql`이 더 안정적인 선택이다.

---

## 2. OpenGoose 현재 DB 현황

### 2.1 기술 스택

```
┌────────────────────────┐
│  10개 Store 구현체       │
│  (SessionStore, etc.)  │
├────────────────────────┤
│  Diesel ORM (Rust)     │
│  diesel(sqlite) 피처    │
├────────────────────────┤
│  SQLite                │
│  WAL 모드              │
│  Arc<Mutex<SqliteConn>>│
│  단일 연결 (풀링 없음)   │
└────────────────────────┘
```

**PRAGMA 설정:**
- `journal_mode = WAL` — 동시 읽기/쓰기 (단, 쓰기는 1개만)
- `foreign_keys = ON` — 참조 무결성
- `busy_timeout = 5000` — 5초 대기
- `synchronous = NORMAL` — WAL과 조합해 빠른 성능
- `cache_size = -8000` — 8MB 페이지 캐시
- `temp_store = MEMORY` — 임시 테이블은 메모리

### 2.2 13개 테이블 (5개 도메인)

#### 세션/대화 (2개)

| 테이블 | 컬럼 수 | 핵심 컬럼 | 역할 |
|--------|---------|----------|------|
| `sessions` | 5 | `session_key` (UNIQUE), `active_team` | 세션 관리 |
| `messages` | 6 | `session_key` (FK), `role`, `content`, `author` | 대화 이력 |

#### 에이전트 통신 (2개)

| 테이블 | 컬럼 수 | 핵심 컬럼 | 역할 |
|--------|---------|----------|------|
| `message_queue` | 13 | `sender`→`recipient`, `status`, `retry_count` | 비동기 메시지 큐 |
| `agent_messages` | 9 | `from_agent`→`to_agent`, `channel`, `status` | 직접/pub-sub 통신 |

#### 작업 추적 (3개)

| 테이블 | 컬럼 수 | 핵심 컬럼 | 역할 |
|--------|---------|----------|------|
| `work_items` | 14 | `parent_id` (자기참조), `assigned_to`, `status` | 계층적 작업 항목 |
| `orchestration_runs` | 12 | `team_run_id`, `workflow`, `current_step/total_steps` | 팀 실행 + 크래시 복구 |
| `workflow_runs` | 9 | `run_id`, `state_json` (전체 상태 스냅샷) | 워크플로우 실행 |

#### 자동화 (4개)

| 테이블 | 컬럼 수 | 핵심 컬럼 | 역할 |
|--------|---------|----------|------|
| `schedules` | 10 | `cron_expression`, `team_name`, `next_run_at` | 크론 스케줄 |
| `triggers` | 11 | `trigger_type`, `condition_json`, `fire_count` | 이벤트 트리거 |
| `alert_rules` | 10 | `metric`, `condition`, `threshold`, `actions` (JSON) | 알림 규칙 |
| `alert_history` | 6 | `rule_id`, `value`, `triggered_at` | 알림 이력 |

#### 시스템 (3개)

| 테이블 | 컬럼 수 | 핵심 컬럼 | 역할 |
|--------|---------|----------|------|
| `event_history` | 6 | `event_kind`, `source_gateway`, `payload` (JSON) | 감사 이력 (30일 보존) |
| `plugins` | 10 | `name`, `capabilities`, `source_path`, `enabled` | 플러그인 레지스트리 |
| `api_keys` | 5 | `key_hash` (bcrypt), `last_used_at` | API 인증 |

### 2.3 Store 구현체 (10개)

모든 Store는 동일 패턴: `Arc<Database>` → `Mutex<SqliteConnection>` → `db.with(|conn| ...)`

| Store | 주요 메서드 | 특이사항 |
|-------|-----------|---------|
| `SessionStore` | `append_user_message`, `get_history`, `list_sessions` | `upsert_session` 트랜잭션 |
| `MessageQueue` | `enqueue`, `dequeue`, `complete`, `fail` | 브로드캐스트 중복 제거 |
| `WorkItemStore` | `create`, `update_status`, `list_children` | 계층 탐색 |
| `OrchestrationStore` | `create_run`, `advance_step`, `mark_completed` | 크래시 복구용 |
| `ScheduleStore` | `create`, `list_due`, `mark_executed` | 크론 기반 |
| `TriggerStore` | `create`, `list_by_type`, `mark_fired` | 이벤트 기반 |
| `PluginStore` | `install`, `by_capability`, `enable/disable` | 능력 기반 검색 |
| `AlertStore` | `create_rule`, `check_alerts`, `record_trigger` | 메트릭 평가 |
| `EventStore` | `record`, `query`, `cleanup_old_events` | 30일 자동 삭제 |
| `AgentMessageStore` | `send_directed`, `publish`, `dequeue_pending` | 직접/채널 모드 |
| `ApiKeyStore` | `generate`, `validate`, `delete` | bcrypt 해시 |

### 2.4 현재 한계

| 한계 | 영향 | 발생 조건 |
|------|------|----------|
| 단일 writer (Mutex) | `SQLITE_BUSY` 오류 | 에이전트 5개 이상 동시 쓰기 |
| 실시간 알림 없음 | 폴링 기반 → 지연 | `agent_messages` 상태 변경 감지 |
| 네트워크 접근 불가 | 원격 에이전트 불가 | 분산 배포 시 |
| 순차 ID 경쟁 | 동시 INSERT 충돌 | `work_items` 병렬 생성 |

---

## 3. Dolt로 대체 가능한가?

### 3.1 완전 대체 가능

**기계적 마이그레이션이 가능한 이유:**

1. **13개 테이블 모두 표준 SQL 타입만 사용**: `TEXT`, `INTEGER`, `REAL` → MySQL 호환
2. **Diesel 전환**: `diesel(sqlite)` → `diesel(mysql)` 피처 플래그 변경
3. **WAL → Dolt MVCC**: server mode에서 자동으로 동시 쓰기 지원
4. **타임스탬프**: `TEXT` 형식의 `datetime('now')` → MySQL `DATETIME` 변환 필요

**마이그레이션 비용 추정:**

| 항목 | 난이도 | 작업량 |
|------|--------|--------|
| Diesel 피처 플래그 교체 | 낮음 | Cargo.toml 1줄 |
| 타입 매핑 변경 | 중간 | schema.rs 전체 |
| `datetime('now')` → `NOW()` | 중간 | 마이그레이션 SQL |
| `AUTOINCREMENT` → `AUTO_INCREMENT` | 낮음 | 마이그레이션 SQL |
| `dolt_*` 프로시저 래퍼 | 높음 | raw SQL + 새 모듈 |
| 연결 관리 (`Mutex` → `Pool`) | 중간 | db.rs 리팩터링 |

### 3.2 Dolt가 추가로 제공하는 것

현재 SQLite에서 불가능하지만 Dolt에서 바로 사용 가능한 기능들:

```
┌──────────────────────────────────────────────────┐
│  현재 SQLite로 불가능한 것들                        │
│                                                  │
│  ① 에이전트별 브랜치 격리                           │
│     CALL dolt_branch('agent-researcher-1')       │
│     → 에이전트가 프로덕션 데이터를 건드리지 않고     │
│       자기 브랜치에서 안전하게 실험                  │
│                                                  │
│  ② Cell-level 3-way Merge                        │
│     CALL dolt_merge('agent-researcher-1')        │
│     → 같은 테이블, 다른 행 수정 → 자동 머지         │
│     → 같은 행, 같은 셀 수정 → 충돌 기록             │
│                                                  │
│  ③ 시간 여행                                      │
│     SELECT * FROM work_items AS OF 'abc123'      │
│     → 아무 커밋 시점의 상태를 즉시 조회              │
│                                                  │
│  ④ 전체 변경 이력                                  │
│     SELECT * FROM dolt_history_work_items         │
│     → 누가, 언제, 무엇을 바꿨는지 (event_history 불필요) │
│                                                  │
│  ⑤ Clone/Push/Pull                               │
│     dolt clone opengoose/sessions                │
│     → 히스토리 포함 전체 DB 복제 (연합에 필수)       │
│                                                  │
│  ⑥ Diff                                         │
│     SELECT * FROM dolt_diff_work_items            │
│     → 에이전트가 1행 vs 1,000행 수정했는지 확인      │
└──────────────────────────────────────────────────┘
```

### 3.3 Dolt의 한계

| 한계 | 영향 | 보완 방법 |
|------|------|----------|
| 읽기 -30% (개선 중) | 대시보드 지연 | 읽기 캐시 (Redis) 또는 doltgres 1.0 대기 |
| 실시간 알림 없음 | 폴링 필요 | EventBus + 폴링 주기 최적화 |
| Rust 타입 안전성 | `dolt_*` 프로시저 raw SQL | 래퍼 모듈 직접 구현 |
| 운영 복잡도 | 별도 서버 프로세스 | Docker Compose로 단순화 |

---

## 4. Git 대체/보완 도구 비교

### 핵심 구분

**Dolt는 데이터 버전 관리, 나머지 3개는 코드 버전 관리 도구.**

이들은 서로 **대체 관계가 아니라 보완 관계**이다. 각각 다른 계층의 문제를 해결한다:

```
┌─────────────────────────────────────────────────┐
│  계층                도구             역할         │
│                                                 │
│  코드 버전 관리    jj (또는 Git)    소스 코드 추적   │
│       ↕           + GitButler     브랜치 정리      │
│                                                 │
│  코드 감사        Entire.io       에이전트 reasoning │
│       ↕           Checkpoints     추적/기록        │
│                                                 │
│  데이터 버전 관리  Dolt            에이전트별       │
│       ↕                          DB 브랜치 격리    │
│                                                 │
│  태스크 그래프     Beads           분산 이슈 트래커  │
│                   (Dolt 위에서)   해시 ID, 머지     │
└─────────────────────────────────────────────────┘
```

---

### 4.1 Entire.io

> Thomas Dohmke (전 GitHub CEO), $60M 시드 (2026.02), $300M 밸류에이션

**정의**: AI 생성 코드의 **관측/감사 레이어**. Git을 대체하지 않고 보완한다.

**Checkpoints** — Git 커밋에 에이전트 reasoning trace를 자동 기록:
- 프롬프트, 도구 호출, 토큰 사용량, 파일 수정 내역, 의사결정 경로
- **Shadow Branch** (`entire/checkpoints/v1`)에 저장 → 메인 브랜치 오염 없음
- Git hooks로 자동 캡처 → 에이전트 코드 변경 없음

**3계층 비전**:
1. Git 호환 데이터베이스
2. 시맨틱 추론 레이어 (왜 이 결정을 했는지)
3. AI-native UI

**멀티에이전트 적합성**: **감사/투명성** 도구. "에이전트가 왜 이 코드를 생성했는가?"에 대한 사후 분석.

**Git Worktree 호환**: 워크트리별 독립 세션 추적 → Gas Town의 에이전트별 worktree와 자연스럽게 조합 가능.

---

### 4.2 Jujutsu (jj-vcs)

> Google 출신 개발자들, Rust 구현, Git 호환 VCS

**정의**: Git 백엔드를 사용하면서 **더 나은 UX와 시맨틱스**를 제공하는 VCS.

**핵심 차별점 3가지:**

#### First-class Conflicts

Git에서 충돌은 "실패 상태"이지만, jj에서 충돌은 **커밋의 일부**이다:

```
Git:    rebase → 충돌 → 작업 중단 → 수동 해결 → --continue
jj:     rebase → 충돌 기록된 채 성공 → 나중에 해결 가능
```

- 충돌이 있어도 커밋, 푸시, 작업 계속 가능
- `git rebase --continue`, `git merge --continue` 같은 특수 명령 불필요
- 단순히 파일을 편집하면 자동으로 충돌 해결

#### Operation Log

모든 작업이 기록되고, 어떤 상태든 복원 가능:

```bash
jj op log             # 모든 작업 이력 보기
jj op restore <id>    # 과거 상태로 복원
jj undo               # 직전 작업 취소
```

#### Anonymous Branches

이름 없는 브랜치로 작업 → 필요할 때만 이름(bookmark) 부여:

```bash
jj new          # 새 변경 시작 (브랜치 이름 불필요)
jj bookmark set feature-x  # 나중에 이름 부여
```

**멀티에이전트 적합성:**
- 충돌을 **허용**하면서 작업 계속 → Gas Town "re-imagine" 패턴과 유사
- Operation log = 에이전트 행동의 완벽한 감사 이력
- `agentic-flow` 프로젝트에서 jj + AgentDB 통합 실험 진행 중
- 동시성 안전: rsync/Dropbox로 공유해도 corrupt 불가 → 최악의 경우 충돌만 노출

---

### 4.3 GitButler

> Scott Chacon (GitHub 공동 창업자), Rust/Svelte 구현

**정의**: Git 클라이언트. **Virtual Branches**로 단일 워킹 디렉토리에서 여러 브랜치 동시 작업.

**Virtual Branches:**

```
기존 Git:    한 번에 한 브랜치에서만 작업
GitButler:   여러 브랜치가 동시에 "적용"됨

┌─ Virtual Branch A ─┐  ┌─ Virtual Branch B ─┐
│  file1.rs (수정)    │  │  file3.rs (수정)    │
│  file2.rs (수정)    │  │  file4.rs (수정)    │
└────────────────────┘  └────────────────────┘
         └──────────┬───────────┘
              단일 워킹 디렉토리
```

- **파일/헝크 단위 브랜치 할당**: 변경 사항을 드래그하여 브랜치에 배정
- **Clean Merge 보장**: 이미 머지된 상태에서 시작 → 브랜치 추출 → 충돌 원천 방지
- **Claude Code 통합**: 라이프사이클 훅으로 에이전트 코드를 자동 브랜치 분류

**멀티에이전트 적합성:**
- 에이전트 출력을 브랜치별로 정리/분류하는 데 유용
- 워크스페이스 규칙으로 파일 → 브랜치 자동 라우팅
- 단, **파일시스템 격리 없음** → Git worktree 대비 격리 수준 낮음

---

### 4.4 비교 매트릭스

| 차원 | Dolt | Entire.io | jj (Jujutsu) | GitButler |
|------|------|-----------|--------------|-----------|
| **대상** | 데이터 (SQL) | 메타데이터 (감사) | 코드 (소스) | 코드 (소스) |
| **브랜칭** | SQL 브랜치 | Shadow 브랜치 | Anonymous + bookmark | Virtual 브랜치 |
| **충돌 해결** | Cell-level 3-way | Git 기본 | First-class (커밋에 기록) | 충돌 방지 설계 |
| **롤백** | 즉시 (`dolt_reset`) | Git 기본 | `op restore` (즉시) | oplog undo |
| **멀티에이전트** | 데이터 격리 (핵심) | 감사/투명성 | 충돌 허용 작업 | 브랜치 자동 분류 |
| **Git 관계** | 독립 (데이터용) | 보완 (메타 레이어) | 대체 (Git 호환) | 보완 (클라이언트) |
| **성숙도** | Production | 초기 (2026.02) | 성숙 중 | 성장 중 |
| **Rust 구현** | Go | - | Rust | Rust |

### 4.5 OpenGoose에서의 조합 전략

이들은 서로 다른 계층을 담당하므로 **동시에 조합 가능**:

| 계층 | 추천 도구 | 이유 |
|------|----------|------|
| 코드 버전 관리 | **jj** (또는 Git) | first-class conflict + operation log |
| 코드 감사 | **Entire.io** (출시 후) | 에이전트 reasoning trace 자동 기록 |
| 데이터 버전 관리 | **Dolt** | 에이전트별 DB 브랜치, cell-level merge |
| 태스크 그래프 | **Beads** (또는 자체 구현) | 해시 ID, 머지 충돌 불가, 메모리 디케이 |
| 브랜치 정리 | **GitButler** (선택) | 에이전트 출력 분류, virtual branches |

---

## 5. Beads란 무엇인가

### 5.1 정의

**Beads**는 Steve Yegge가 만든 **AI 코딩 에이전트를 위한 분산 그래프 이슈 트래커**이다.

- GitHub: `steveyegge/beads`, Go 구현, v0.59.0, 18.7k+ 스타
- **설계 원칙**: "AI 에이전트가 사용하기 편한 이슈 트래커"
- **스토리지**: Git 기반 + Dolt 백엔드 → 충돌 없는 머지

```
┌──────────────────────────────────────────────────┐
│  기존 이슈 트래커 (JIRA, GitHub Issues)            │
│  - 순차 ID (#123) → 머지 충돌                     │
│  - 플랫 구조 → 서브태스크 관리 어려움               │
│  - 인간 중심 UI → AI 에이전트에게 비효율적          │
│  - 중앙 서버 → 오프라인 불가                       │
│                                                  │
│  Beads                                           │
│  - 해시 ID (bd-a1b2) → 머지 충돌 원천 방지         │
│  - 무한 중첩 (bd-a3f8.1.1.2) → 서브태스크의 서브태스크│
│  - AI 최적화 CLI (bd prime, bd ready) → 토큰 절약  │
│  - 분산 (Git/Dolt) → 오프라인 + 크로스 rig 참조    │
└──────────────────────────────────────────────────┘
```

### 5.2 핵심 특성

#### 해시 기반 ID

```
bd-a1b2    ← 콘텐츠 해시, 순차 번호 아님
bd-c3d4    ← 동시에 생성해도 충돌 불가
```

JIRA의 `PROJ-123` 같은 순차 ID는 두 에이전트가 동시에 태스크를 만들면 충돌한다. Beads의 해시 ID는 콘텐츠 기반이므로 **충돌이 구조적으로 불가능**하다.

#### 무한 중첩

```
bd-a3f8          ← 최상위 태스크
  bd-a3f8.1      ← 서브태스크
    bd-a3f8.1.1  ← 서브서브태스크
    bd-a3f8.1.2  ← 서브서브태스크
  bd-a3f8.2      ← 서브태스크
```

OpenGoose의 `work_items.parent_id`는 1단계 부모-자식만 지원하지만, Beads는 무한 깊이의 태스크 분해를 지원한다.

#### 관계 타입

| 관계 | 설명 | 예시 |
|------|------|------|
| `relates_to` | 관련 태스크 참조 | "이 버그는 bd-c3d4와 관련" |
| `duplicates` | 중복 태스크 표시 | "bd-e5f6은 이것의 중복" |
| `supersedes` | 이전 태스크 대체 | "이 새 접근법이 bd-g7h8을 대체" |
| `replies_to` | 대화형 참조 | "bd-i9j0에 대한 응답" |
| `external:prefix:id` | 크로스 rig 참조 | "external:gastown:hq-42" |

#### 메모리 디케이 라이프사이클

```
CREATE → LIVE → CLOSE → DECAY → COMPACT → FLATTEN

CREATE:  태스크 생성, 모든 세부 정보 포함
LIVE:    활성 작업 중, 전체 컨텍스트 유지
CLOSE:   완료됨, 전체 정보 보존
DECAY:   시간이 지나면 중요도 하락
COMPACT: AI가 요약본 생성, 원본 보존
FLATTEN: 최소 참조만 남김
```

이 패턴은 AI 에이전트의 **컨텍스트 윈도우 최적화**를 위한 것이다. 오래된 태스크를 자동 압축하여 토큰 낭비를 방지한다.

### 5.3 핵심 명령어

| 명령 | 역할 | AI 최적화 이유 |
|------|------|--------------|
| `bd prime` | 프로젝트 컨텍스트 생성 | 에이전트 세션 시작 시 `.beads/prime.md`를 로딩하면 프로젝트 이해 |
| `bd ready` | 블로킹 없는 태스크만 표시 | 실행 가능한 것만 보여줘서 토큰 절약 |
| `bd compact` | 오래된 태스크 요약 | 컨텍스트 윈도우에 맞게 압축 |
| `bd create` | 태스크 생성 | 해시 ID 자동 생성, 머지 충돌 불가 |
| `bd update --claim` | 태스크 원자적 할당 | 두 에이전트가 동시에 같은 태스크를 잡지 않음 |
| `bd nest` | 서브태스크 생성 | 무한 깊이 분해 |

### 5.4 Beads + Dolt 통합

Dolt가 Beads의 **스토리지 백엔드** 역할:

```
에이전트 A:  dolt_branch('agent-a') → bd create "분석" → dolt_commit
에이전트 B:  dolt_branch('agent-b') → bd create "구현" → dolt_commit
                                        │
                                        ▼
main:       dolt_merge('agent-a') → dolt_merge('agent-b')
            → 해시 ID이므로 충돌 0 → 자동 통합 완료
```

Gas Town의 2계층 구조:
- **Town-level**: `hq-*` ID — Mayor가 관리하는 상위 태스크
- **Rig-level**: `bd-*` ID — 개별 에이전트(Rig)의 Beads 태스크

### 5.5 OpenGoose WorkItem vs Beads 비교

| 기능 | Beads | OpenGoose WorkItem |
|------|-------|-------------------|
| **ID 방식** | 해시 기반 (`bd-a1b2`) | 순차 INTEGER (AUTOINCREMENT) |
| **중첩 깊이** | 무한 (`bd-a3f8.1.1.2`) | 1단계 (`parent_id`) |
| **관계** | `relates_to`, `supersedes` 등 5종 | 없음 |
| **AI 최적화** | `bd prime`, `bd ready`, `bd compact` | 없음 |
| **머지 충돌** | 불가 (해시 ID) | 가능 (순차 ID 경쟁) |
| **메모리 디케이** | `compact` → 자동 요약 | 없음 |
| **크로스 rig 참조** | `external:prefix:id` | 없음 |
| **분산 복제** | Git/Dolt clone | 불가 (SQLite 파일) |

**진화 경로**: OpenGoose의 `work_items`를 Beads 패턴으로 확장할 수 있다:
1. 순차 ID → 해시 기반 ID
2. `parent_id` → 무한 중첩 경로 (`bd-a3f8.1.1.2`)
3. 관계 테이블 추가
4. `bd prime`/`bd ready`/`bd compact` 동등 기능 구현

---

## 6. Wasteland는 어떻게 사용하는가

### 6.1 Wasteland의 Dolt 활용

Wasteland는 **크로스 조직 AI 에이전트 연합**을 위해 Dolt를 핵심 인프라로 사용한다.

```
┌──────────┐    Dolt push/pull     ┌──────────┐
│ Wasteland │ ◀──────────────────▶ │ DoltHub  │
│ Instance A│    gRPC 청크 전송      │ hop/     │
│ (조직 A)  │    (변경분만)          │ wl-commons│
└──────────┘                      └──────────┘
      ▲                                ▲
      │                                │
      │         Dolt push/pull         │
      │                                │
┌──────────┐                      ┌──────────┐
│ Wasteland │ ◀──────────────────▶ │ Wasteland│
│ Instance B│                      │ Instance C│
│ (조직 B)  │                      │ (조직 C) │
└──────────┘                      └──────────┘
```

### 6.2 Public Commons DB

DoltHub `hop/wl-commons` 레포지토리에 **4개 핵심 테이블**:

| 테이블 | 역할 |
|--------|------|
| **Wanted Board** | 작업 요청 게시판 (bounty 포함) |
| **Completions** | 완료된 작업 기록 |
| **Stamps** | 평판/보증 도장 (신뢰도 지표) |
| **Trust Ladder** | 4단계 신뢰 등급 (Stranger→Trusted→Verified→Core) |

### 6.3 워크플로우

```bash
# 1. 연합 참여
wl join                    # DoltHub에서 wl-commons 포크

# 2. 로컬 클론
# .wasteland/ 디렉토리에 Dolt DB 생성

# 3. 작업 탐색 및 수행
wl browse                  # Wanted Board 탐색
wl claim <task-id>         # 작업 할당 (원자적)
# ... 에이전트가 작업 수행 ...
wl done <task-id>          # 완료 보고

# 4. 동기화
wl sync                    # Dolt push/pull로 연합과 동기화
```

### 6.4 핵심 Dolt 패턴

| 패턴 | 설명 |
|------|------|
| **브랜치별 독립 인스턴스** | 각 Wasteland 인스턴스가 자기 Dolt 브랜치에서 작업 |
| **gRPC 청크 전송** | Dolt의 Prolly Tree 덕분에 변경분만 네트워크 전송 |
| **Append-only 히스토리** | Dolt 커밋 이력은 수정 불가 → 이력 조작 방지 (신뢰 기반) |
| **PR 모드 vs Wild-west** | PR 모드: 머지 전 리뷰 / Wild-west: 자동 머지 |

### 6.5 Gas Town과의 비교

| 차원 | Gas Town | Wasteland |
|------|----------|-----------|
| **범위** | 단일 조직 내 | 크로스 조직 |
| **Dolt 서버** | 단일 (port 3307) | 각 인스턴스별 |
| **DB 수** | 4개 (`hq`, `beads`, `gastown`, `crew`) | 1개 (commons) |
| **동기화** | 브랜치 머지 (로컬) | push/pull (네트워크) |
| **신뢰 모델** | 조직 내 (암묵적) | Stamps + Trust Ladder |
| **충돌 해결** | Refinery "re-imagine" | PR 리뷰 또는 자동 머지 |

**Refinery "re-imagine" 패턴**: 머지 충돌 시 양쪽 버전을 AI에게 보여주고 **새로운 통합 구현을 생성**하게 한다. 단순한 "ours/theirs" 선택이 아닌 창의적 해결.

---

## 7. Dolt + PostgreSQL 조합 분석

### 7.1 왜 조합을 고려하는가?

| 특성 | Dolt 강점 | PostgreSQL 강점 |
|------|----------|----------------|
| 브랜칭/머지 | Cell-level 3-way merge | 없음 |
| 읽기 성능 | -30% (개선 중) | 최적 |
| 실시간 알림 | 없음 (폴링) | LISTEN/NOTIFY |
| 동시 쓰기 | MVCC + branch | MVCC |
| 시간 여행 | AS OF (네이티브) | Temporal Tables (확장) |
| 연합/분산 | clone/push/pull | Logical Replication |
| Diesel 지원 | `mysql` feature | `postgres` feature |
| 운영 성숙도 | 상대적 신생 | 30년+ |

### 7.2 패턴별 비교

#### 패턴 A: Dolt 단독 (Gas Town 방식)

```
장점: 브랜칭/머지 네이티브, 단일 시스템, Gas Town 검증
단점: 읽기 -30%, 실시간 알림 없음
적합: 에이전트 브랜칭이 핵심 기능인 경우
```

#### 패턴 B: PostgreSQL 단독

```
장점: 읽기 최적, LISTEN/NOTIFY, 30년 성숙도, Diesel 최적 지원
단점: 브랜칭 없음, 연합 동기화 어려움
적합: 읽기 성능 + 실시간이 우선인 경우
```

#### 패턴 C: Dolt + PostgreSQL 하이브리드

```
장점: 각각의 강점 활용
단점: 두 시스템 운영/동기화 비용, 데이터 일관성 문제
결론: 비권장 — 복잡도 대비 이점 부족
```

#### 패턴 D: Redis + PostgreSQL

```
장점: 업계 표준 (CrewAI, LangGraph 등), 서브밀리초 메시징
단점: 두 시스템 운영, 브랜칭 없음
적합: 대규모 (50+ 에이전트) 배포
```

#### 패턴 E: doltgres (Dolt + PostgreSQL 통합)

```
장점: Dolt 브랜칭 + PostgreSQL 호환
단점: Beta 단계, 1.0 목표 2026.04
적합: 1.0 출시 후 검토 — 잠재적 최적 해
```

### 7.3 동시 쓰기 비교

| | SQLite (현재) | Dolt | PostgreSQL |
|-|:---:|:---:|:---:|
| 쓰기 모델 | 단일 writer (Mutex) | MVCC + branch | MVCC |
| 동시 쓰기 | `SQLITE_BUSY` | 브랜치별 격리 | 트랜잭션 격리 |
| 충돌 해결 | N/A | 3-way cell merge | 롤백 + 재시도 |
| 실질적 에이전트 한계 | 3-5 | 15-25 | 30+ |

---

## 8. OpenGoose에 가장 잘 맞는 선택

### 8.1 권장: Dolt 단독 (Gas Town 패턴)

OpenGoose가 Gas Town/Wasteland 수준의 멀티에이전트 오케스트레이션을 목표로 한다면, **Dolt가 최적의 선택**이다.

**이유:**

| # | 근거 | 상세 |
|---|------|------|
| 1 | **에이전트 브랜칭이 핵심 차별점** | SQLite/PostgreSQL로는 불가능한 기능 |
| 2 | **Beads 호환** | Dolt 위에 Beads 패턴 구현 가능 |
| 3 | **MCP 서버** | 에이전트가 직접 `dolt_branch/merge/commit` 호출 |
| 4 | **연합 준비** | Wasteland의 clone/push/pull이 Dolt에서만 가능 |
| 5 | **검증된 패턴** | Gas Town이 20-30 에이전트에서 프로덕션 사용 중 |
| 6 | **doltgres 로드맵** | 2026.04 1.0 출시 시 PostgreSQL 생태계도 활용 가능 |

### 8.2 코드 버전 관리 보완

데이터(Dolt)와 별도로, 코드 버전 관리도 강화할 수 있다:

| 도구 | 우선순위 | 이유 |
|------|---------|------|
| **jj** | 높음 | first-class conflict + operation log → 멀티에이전트 코드 작업에 최적 |
| **Entire.io** | 중간 | 에이전트 reasoning 감사 → 출시 후 검토 |
| **GitButler** | 낮음 | 에이전트 출력 정리에 유용하나 필수 아님 |

### 8.3 마이그레이션 경로

```
현재 상태                    목표 상태
┌──────────┐              ┌──────────────────────┐
│ SQLite   │              │ Dolt (MySQL 프로토콜)  │
│ Diesel   │  ──────────▶ │ Diesel(mysql)         │
│ 13 tables│              │ + dolt_* 래퍼 모듈     │
│ 10 stores│              │ + Branch 관리 계층     │
└──────────┘              │ + Beads 패턴 WorkItem  │
                          └──────────────────────┘
```

#### 단계 1: Store trait 추상화 (현재 가능)

```rust
// 현재: SQLite에 직접 의존
impl WorkItemStore {
    pub fn create(&self, ...) -> PersistenceResult<i32> {
        self.db.with(|conn| { /* Diesel SQLite 쿼리 */ })
    }
}

// 목표: trait 기반 추상화
trait WorkItemRepository {
    fn create(&self, ...) -> PersistenceResult<WorkItemId>;
    fn get(&self, id: WorkItemId) -> PersistenceResult<WorkItem>;
    // ...
}

impl WorkItemRepository for SqliteWorkItemStore { ... }
impl WorkItemRepository for DoltWorkItemStore { ... }
```

#### 단계 2: Dolt 서버 로컬 실행 + Dual-write

```bash
# Dolt 서버 시작
dolt sql-server --port 3307 --host 0.0.0.0

# 양쪽에 동시 쓰기, 읽기는 SQLite에서만
```

#### 단계 3: 검증 후 전환

- 데이터 일관성 확인
- 쿼리 성능 벤치마크
- 에이전트 통합 테스트

#### 단계 4: Beads 패턴으로 WorkItem 확장

| 변경 | 상세 |
|------|------|
| 순차 ID → 해시 ID | `INTEGER AUTOINCREMENT` → `TEXT` (해시) |
| 1단계 부모 → 무한 중첩 | `parent_id` → 경로 기반 (`bd-a3f8.1.1`) |
| 관계 추가 | `work_item_relations` 테이블 |
| AI 최적화 | `prime`, `ready`, `compact` 동등 기능 |

### 8.4 database-strategy.md와의 관계

기존 [database-strategy.md](./database-strategy.md)는 "언제 DB를 바꿔야 하는가?"에 대한 ADR로, PostgreSQL을 최우선으로 권장했다.

**이 문서의 보완 의견:**

database-strategy.md의 결론 "대부분의 경우 PostgreSQL"은 **범용적으로 맞다**. 그러나 OpenGoose가 Gas Town/Wasteland 수준을 목표로 한다면, 에이전트 브랜칭은 "있으면 좋은 기능"이 아니라 **핵심 차별화 기능**이 된다. 이 경우 Dolt가 더 적합하다.

```
의사결정 트리:

OpenGoose의 핵심 목표가 무엇인가?
├── "단순히 잘 작동하는 멀티에이전트 시스템"
│   └── PostgreSQL 권장 (database-strategy.md 결론 유지)
│
└── "Gas Town/Wasteland 수준의 에이전트 자율성"
    └── Dolt 권장 (이 문서의 결론)
        ├── 에이전트별 브랜치 격리
        ├── Beads 패턴 태스크 관리
        ├── 연합 동기화 (clone/push/pull)
        └── doltgres 1.0 (2026.04) → PostgreSQL 호환성 추가
```

---

## 9. Beads vs beads_rust: 무엇이 다른가

### 9.1 기본 비교

| 차원 | Beads (원본) | beads_rust |
|------|-------------|------------|
| **언어** | Go | Rust |
| **저자** | Steve Yegge | Dicklesworthstone |
| **크기** | ~130,000줄 | ~20,000줄 |
| **GitHub 스타** | 18.7k+ | ~수백 |
| **버전** | v0.59.0 | 초기 |
| **라이선스** | Apache-2.0 | MIT |
| **백엔드** | Dolt (유일) | SQLite + JSONL |
| **형태** | CLI 도구 | CLI 도구 (lib.rs 없음) |
| **ORM** | 없음 (raw SQL) | rusqlite (직접 SQL) |
| **프로덕션 검증** | Gas Town (20-30 에이전트) | 없음 |

### 9.2 기능 차이

| 기능 | Beads | beads_rust | 비고 |
|------|-------|------------|------|
| **해시 ID** | SHA-256 + base36, 적응형 길이 (4→5→6) | SHA-256 + base36, 적응형 길이 | 동일 알고리즘 |
| **ready()** | Dolt 쿼리 기반 + blocked 캐시 | SQLite 쿼리 기반 + blocked 캐시 | 동일 로직, 다른 DB |
| **prime()** | BriefIssue 포맷 (~97% 토큰 절감) | BriefIssue 포맷 | 동일 |
| **compact()** | AI 요약 + 원본 보존 | AI 요약 + 원본 보존 | 동일 |
| **Wisp** | Dolt `dolt_ignore` 기반 | 없음 | beads_rust에 미구현 |
| **Molecule** | 전체 워크플로 엔진 | 없음 | Gas Town 핵심 기능 |
| **Landing the Plane** | AGENT_INSTRUCTIONS.md 관례 | 없음 | beads_rust에 미구현 |
| **remember/recall** | KV 메모리, prime()에 주입 | KV 메모리 | 동일 |
| **브랜칭** | Dolt `dolt_branch/merge` | 없음 (SQLite 단일 DB) | 핵심 차이 |
| **동기화** | Dolt push/pull | JSONL 내보내기 | Dolt ≫ JSONL |
| **3-way merge** | Dolt cell-level 네이티브 | 커스텀 구현 (SQLite ATTACH) | Dolt가 훨씬 강력 |
| **MCP 서버** | 지원 | 없음 | |
| **콘텐츠 해시 중복 제거** | 없음 | 있음 (동일 내용 탐지) | beads_rust가 추가 |
| **JSONL 이식** | 제거됨 (v0.58) | 핵심 기능 | 방향이 반대 |

### 9.3 아키텍처 차이

```
Beads (원본):
┌─────────┐     ┌──────────────┐
│ bd CLI  │────▶│ Dolt 서버     │  ← 별도 프로세스 (3307 포트)
│ (Go)    │     │ (MySQL 프로토콜)│
└─────────┘     └──────────────┘
  │
  ├── Molecule 엔진 (워크플로)
  ├── Wisp (dolt_ignore 기반)
  ├── Landing the Plane (관례)
  └── MCP 서버

beads_rust:
┌─────────┐     ┌──────────────┐
│ main.rs │────▶│ SQLite       │  ← 임베디드 (파일)
│ (Rust)  │     │ (rusqlite)   │
└─────────┘     └──────────────┘
  │
  ├── JSONL 내보내기 (이식성)
  └── 콘텐츠 해시 중복 제거
```

### 9.4 OpenGoose에서 각각 참조할 것

| 출처 | 참조할 것 | 이유 |
|------|----------|------|
| **Beads** | ready/prime/compact 알고리즘 설계 | 프로덕션 검증됨 |
| **Beads** | Wisp + Landing the Plane 개념 | OpenGoose에서 개선하여 구현 |
| **Beads** | Molecule 워크플로 패턴 | OpenGoose Team/Workflow와 매핑 |
| **Beads** | BriefIssue 포맷 | 토큰 최적화 핵심 |
| **beads_rust** | SHA-256 + base36 해시 ID 구현 코드 | Rust 참조 구현 |
| **beads_rust** | SQLite 기반 ready() 쿼리 | Diesel로 변환하되 로직 참조 |
| **beads_rust** | blocked 캐시 테이블 구조 | 그대로 차용 가능 |
| **beads_rust** | 콘텐츠 해시 중복 제거 | 유용한 추가 기능 |

**핵심 차이 요약**: Beads는 Dolt 위에서 **완전한 멀티에이전트 워크플로 엔진**이고, beads_rust는 **핵심 알고리즘만 SQLite로 포팅한 경량 재구현**이다. OpenGoose는 Beads의 설계 + beads_rust의 구현을 참조하되, 둘 다 의존성으로 사용하지 않고 자체 구현한다.

---

## 10. Dolt의 장점을 SQLite 위에서 가져가기

### 10.1 Dolt 기능별 OpenGoose 대응 전략

Dolt를 서버로 도입하지 않더라도, Dolt의 핵심 장점들을 SQLite 위에 구현할 수 있다:

| Dolt 기능 | 가치 | SQLite 위 구현 방법 | 복잡도 |
|-----------|------|---------------------|--------|
| **Branch-per-Agent** | 에이전트 격리 | `VACUUM INTO` → 에이전트별 DB 파일 복제 | 중간 |
| **Cell-level Diff** | 변경 가시성 | `ATTACH` + `EXCEPT` 쿼리 | 중간 |
| **3-way Merge** | 자동 통합 | 커스텀 머지 엔진 (base, ours, theirs 비교) | 높음 |
| **AS OF 쿼리** | 시간 여행 | Temporal 테이블 (`_history` 접미사) + 트리거 | 중간 |
| **dolt_history_<table>** | 변경 이력 | `work_items_history` 자동 기록 트리거 | 낮음 |
| **dolt_log** | 커밋 이력 | `vcs_commits` 테이블 | 낮음 |
| **dolt_conflicts** | 충돌 관리 | `vcs_conflicts` 테이블 | 중간 |
| **dolt_reset** | 즉시 롤백 | 브랜치 DB 파일 삭제 + main 재복제 | 낮음 |
| **clone/push/pull** | 연합 동기화 | Phase 5: cr-sqlite 또는 커스텀 동기화 | 높음 |
| **MCP 서버** | 에이전트 직접 조작 | OpenGoose API로 동등 기능 노출 | 낮음 |

### 10.2 구현 우선순위

```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Phase 1: 필수 (Beads 핵심)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  ✓ 해시 ID (머지 충돌 방지)
  ✓ 관계 그래프 (blocks/depends_on)
  ✓ Wisp (휘발성 태스크)
  ✓ ready/prime/compact 알고리즘

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Phase 2: Dolt 핵심 가치 (SQLite 위)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  ○ Temporal 테이블 (자동 이력)
    → INSERT/UPDATE/DELETE 트리거로 _history 테이블에 자동 기록
    → SELECT * FROM work_items_history WHERE changed_at > ?
    → Dolt의 AS OF와 동등한 효과

  ○ Branch-per-Agent (격리)
    → VACUUM INTO 'agent-1.db' (스냅샷 생성, <1ms for <10MB)
    → 에이전트가 자기 DB에서 자유롭게 작업
    → 실패 시 브랜치 DB 삭제 = 즉시 롤백

  ○ Cell-level Diff
    → ATTACH 'agent-1.db' AS branch;
    → SELECT main.*, branch.*
       FROM main.work_items main
       JOIN branch.work_items branch ON main.id = branch.id
       WHERE main.status != branch.status
          OR main.title != branch.title
          OR ...;
    → 변경된 셀만 반환

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Phase 3: Dolt 고급 기능 (SQLite 위)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  ○ 3-way Merge
    → base (분기 시점) + ours (main) + theirs (branch) 비교
    → 같은 셀 변경 → 충돌 기록 (vcs_conflicts)
    → 다른 셀 변경 → 자동 머지
    → 구현: ~200줄 Rust (컬럼별 비교 루프)

  ○ Agent Memory (remember/recall)
  ○ Landing the Plane (프로그래밍적)
  ○ Blocked 캐시 + EventBus 연동

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Phase 4: VCS 완성
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  ○ vcs_commits / vcs_branches 테이블
  ○ 커밋 그래프 (부모 해시 체인)
  ○ 충돌 해결 UI/API

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Phase 5 (나중): 연합 동기화
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  ○ cr-sqlite 또는 커스텀 동기화
  ○ 또는: 이 시점에서 Dolt/doltgres 서버 전환
```

### 10.3 Temporal 테이블 구현 예시

```sql
-- 1. 이력 테이블 생성
CREATE TABLE work_items_history (
    history_id INTEGER PRIMARY KEY AUTOINCREMENT,
    operation TEXT NOT NULL,  -- 'INSERT', 'UPDATE', 'DELETE'
    changed_at TEXT NOT NULL DEFAULT (datetime('now')),
    changed_by TEXT,          -- 에이전트 이름
    -- 원본 테이블의 모든 컬럼 복사
    id INTEGER,
    title TEXT,
    status TEXT,
    assigned_to TEXT,
    priority TEXT,
    parent_id INTEGER,
    hash_id TEXT,
    -- ... 기타 컬럼
    -- 변경 전/후 값 (UPDATE용)
    old_status TEXT,
    old_assigned_to TEXT
);

-- 2. 자동 기록 트리거
CREATE TRIGGER work_items_after_update
AFTER UPDATE ON work_items
BEGIN
    INSERT INTO work_items_history (
        operation, changed_by,
        id, title, status, assigned_to, priority, parent_id, hash_id,
        old_status, old_assigned_to
    ) VALUES (
        'UPDATE', NEW.assigned_to,
        NEW.id, NEW.title, NEW.status, NEW.assigned_to, NEW.priority,
        NEW.parent_id, NEW.hash_id,
        OLD.status, OLD.assigned_to
    );
END;

-- 3. 시간 여행 쿼리 (Dolt의 AS OF와 동등)
-- "1시간 전 work_items 상태"
SELECT * FROM work_items_history
WHERE changed_at <= datetime('now', '-1 hour')
ORDER BY history_id DESC;
```

### 10.4 Branch-per-Agent 구현 예시

```rust
use std::path::PathBuf;

/// Dolt의 dolt_branch()와 동등한 기능
pub fn create_branch(main_db_path: &str, branch_name: &str) -> Result<PathBuf> {
    let branch_path = format!("{}.branch.{}", main_db_path, branch_name);

    // VACUUM INTO: 전체 DB를 새 파일로 복제 (atomic, <1ms for small DBs)
    let conn = Connection::open(main_db_path)?;
    conn.execute(&format!("VACUUM INTO '{}'", branch_path), [])?;

    Ok(PathBuf::from(branch_path))
}

/// Dolt의 dolt_diff()와 동등한 기능
pub fn diff_branch(main_path: &str, branch_path: &str, table: &str) -> Result<Vec<CellDiff>> {
    let conn = Connection::open(main_path)?;
    conn.execute(&format!("ATTACH '{}' AS branch", branch_path), [])?;

    // 컬럼별 비교 → 변경된 셀만 반환
    let diffs = conn.prepare(&format!(
        "SELECT m.id, m.*, b.*
         FROM main.{table} m
         JOIN branch.{table} b ON m.id = b.id
         WHERE m.status != b.status
            OR m.title != b.title
            OR m.assigned_to != b.assigned_to"
    ))?.query_map([], |row| {
        // CellDiff 구조체로 변환
        Ok(CellDiff { /* ... */ })
    })?;

    Ok(diffs.collect())
}

/// Dolt의 dolt_merge()와 동등한 기능 (3-way)
pub fn merge_branch(
    main_path: &str,
    branch_path: &str,
    base_snapshot: &str,  // 분기 시점의 스냅샷
) -> Result<MergeResult> {
    let conn = Connection::open(main_path)?;
    conn.execute(&format!("ATTACH '{}' AS branch", branch_path), [])?;
    conn.execute(&format!("ATTACH '{}' AS base", base_snapshot), [])?;

    // 3-way merge: base vs main vs branch
    // 같은 셀이 양쪽에서 변경 → 충돌
    // 한쪽만 변경 → 자동 머지
    let conflicts = Vec::new();
    let merged = Vec::new();

    // ... (컬럼별 비교 루프)

    if conflicts.is_empty() {
        Ok(MergeResult::Clean(merged))
    } else {
        Ok(MergeResult::Conflict(conflicts))
    }
}

/// Dolt의 dolt_reset('--hard')와 동등한 기능
pub fn reset_branch(branch_path: &str) -> Result<()> {
    std::fs::remove_file(branch_path)?;
    Ok(())  // 브랜치 DB 삭제 = 완전 롤백
}
```

### 10.5 정리: Dolt 없이 Dolt의 가치를 얻는 방법

| Dolt 가치 | 단일 바이너리에서 달성 가능? | 방법 |
|-----------|:---:|------|
| 에이전트 격리 | ✓ | VACUUM INTO → DB-per-Agent |
| 변경 추적 | ✓ | Temporal 테이블 + 트리거 |
| 즉시 롤백 | ✓ | 브랜치 DB 파일 삭제 |
| Cell-level diff | ✓ | ATTACH + 컬럼별 비교 |
| 3-way merge | ✓ | 커스텀 머지 엔진 (~200줄) |
| 시간 여행 | ✓ | _history 테이블 쿼리 |
| Clone/Push/Pull | △ | Phase 5 (cr-sqlite 또는 Dolt 전환) |
| 대규모 동시 쓰기 (20+) | ✗ | Dolt/PostgreSQL 서버 전환 필요 |

**결론: Dolt의 핵심 가치 6개 중 5개를 SQLite 위에서 구현 가능. 나머지 2개(연합 동기화, 대규모 쓰기)는 스케일 아웃 시점에서 Dolt 서버로 전환하여 해결.**

---

## 참고 자료

- [DoltHub 공식 사이트](https://www.dolthub.com/)
- [doltgres Beta 블로그](https://www.dolthub.com/blog/2025-04-16-doltgres-goes-beta/)
- [doltgres 현황 (2025.10)](https://www.dolthub.com/blog/2025-10-16-state-of-doltgres/)
- [Beads (steveyegge/beads)](https://github.com/steveyegge/beads)
- [Jujutsu VCS](https://www.jj-vcs.dev/)
- [GitButler](https://gitbutler.com/)
- [Entire.io](https://entire.io/)
- [agentic-flow: jj + AgentDB 통합](https://github.com/ruvnet/agentic-flow/issues/54)
