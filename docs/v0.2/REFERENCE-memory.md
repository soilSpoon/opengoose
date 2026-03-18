# 참조: 메모리 — OpenClaw, QMD, Letta Code

> 에이전트 메모리 시스템 설계의 근거. Goose 내장 메모리([Foundation](REFERENCE-foundation.md) § 1)를 넘어서 필요한 것.

---

## 1. OpenClaw Memory (openclaw.ai)

**무엇인가:** OpenClaw의 내장 에이전트 메모리 시스템. 마크다운 파일이 진실의 원천, 선택적 벡터 인덱싱.

### 2계층 메모리

| 계층 | 파일 | 감쇠 | 로딩 |
|------|------|------|------|
| **일간 로그** | `memory/YYYY-MM-DD.md` | 30일 반감기 | 세션 시작 시 오늘+어제 자동 로드 |
| **큐레이션** | `MEMORY.md` | Evergreen (감쇠 없음) | 비공개 세션에서만 로드 |

### Pre-compaction flush (핵심 메커니즘)

```json5
compaction: {
  memoryFlush: {
    enabled: true,
    softThresholdTokens: 4000,
    systemPrompt: "Session nearing compaction. Store durable memories now."
  }
}
```

컨텍스트 압축 **전에** silent agentic turn 삽입 → 에이전트가 중요한 것을 디스크에 기록할 기회. 에이전트가 기록할 것이 없으면 `NO_REPLY`. 압축 사이클당 1회만 트리거.

### 시맨틱 검색 (벡터 + BM25 하이브리드)

- Per-agent SQLite: `~/.openclaw/memory/<agentId>.sqlite`
- 청킹: ~400토큰, 80토큰 오버랩
- 하이브리드: `vectorWeight: 0.7, textWeight: 0.3`
- MMR 리랭킹 (다양성): `λ=0.7`
- 시간 감쇠: `decayedScore = score × e^(-λ × ageInDays)`, 반감기 30일
- Evergreen 파일 (`MEMORY.md`, 비날짜 파일) 감쇠 면제

### 에이전트 도구

- **`memory_search`** — 시맨틱 검색. 스니펫 (~700자) + 파일 경로 + 라인 범위 + 점수 반환
- **`memory_get`** — 특정 파일 읽기. 파일 없으면 빈 텍스트 반환 (에러 아님)
- 자동 쓰기 API 없음 — 에이전트가 명시적으로 파일에 기록해야 함

### QMD 백엔드 (선택적)

OpenClaw은 기본 SQLite 대신 QMD를 메모리 백엔드로 사용 가능:
- 사이드카 아키텍처: `~/.openclaw/agents/<agentId>/qmd/`에 격리된 QMD 홈
- 세션 트랜스크립트 인덱싱 (User/Assistant 턴을 마크다운으로 내보내기)
- QMD 실패 시 기본 SQLite로 graceful fallback

### 스코프 제어

```json5
scope: {
  default: "deny",
  rules: [
    { action: "allow", match: { chatType: "direct" } },
    { action: "deny", match: { keyPrefix: "discord:channel:" } }
  ]
}
```

**v0.2 적용:**
- **Pre-compaction flush** → Goose의 자동 압축에 훅을 걸어 에이전트에게 기록 기회 제공. 가장 중요한 단일 메커니즘
- **2계층 (일간 로그 + 큐레이션)** → per-rig 메모리에 동일 구조 적용
- **시간 감쇠** → Wasteland stamps의 30일 반감기와 동일. 일관된 시간 모델
- **에이전트 주도 쓰기** → `board__remember` 도구로 에이전트가 판단하여 기록

---

## 2. QMD (tobi/qmd)

**무엇인가:** Tobi Lütke(Shopify CEO)가 만든 온디바이스 하이브리드 검색 엔진. "내가 만든 모든 도구의 기반." TypeScript, 16k stars. 2025-12-07 공개.

### 3단계 검색 파이프라인

```
쿼리 → Query Expansion (커스텀 파인튜닝 1.7B)
  → lex:/vec:/hyde: 타입별 확장
  → [원본 ×2 가중치] + [확장 1] + [확장 2]
  → BM25 (FTS5) + Vector (cosine) 각각 검색
  → RRF (k=60) 융합 → Top 30
  → LLM Re-ranking (qwen3-reranker 0.6b)
  → Position-Aware Blend:
      Rank 1-3:  75% RRF + 25% reranker
      Rank 4-10: 60% RRF + 40% reranker
      Rank 11+:  40% RRF + 60% reranker
```

**Strong signal bypass:** BM25 ≥0.85 + 2위와 ≥0.15 격차 → 확장 스킵 (~4초 절약).

### Content-addressable 저장소

- SQLite 단일 파일 (`~/.cache/qmd/index.sqlite`)
- 벡터는 **콘텐츠 해시로 키잉** — 파일 이동 시 재임베딩 없음
- 동일 내용의 다른 파일은 임베딩 공유 (자동 중복 제거)
- CoW 스토어의 SHA-256 해싱과 철학 일치

### 3개 로컬 GGUF 모델 (~2GB 총합)

| 모델 | 용도 | 크기 |
|------|------|------|
| embeddinggemma-300M | 임베딩 (768차원) | ~300MB |
| qwen3-reranker-0.6b | 교차 인코더 리랭킹 | ~640MB |
| qmd-query-expansion-1.7B | 쿼리 확장 (커스텀 SFT+GRPO) | ~1.1GB |

전부 in-process 실행 (Ollama 불필요). 자동 다운로드.

### Context 시스템

계층적 메타데이터 (global → collection → path prefix → document)가 검색 결과와 함께 반환. `intent` 파라미터로 동일 쿼리의 다른 의미 구분 (예: "performance" = 시스템 벤치마크 vs 직원 평가).

### MCP + SDK

- `qmd mcp` — `query`, `get`, `multi_get`, `status` 도구
- HTTP 트랜스포트 (`--http --daemon`) — 모델을 VRAM에 웜 유지
- SDK: `import { createStore } from '@tobilu/qmd'` — 앱 내장 가능

### 스마트 청킹

~900토큰, 15% 오버랩. 마크다운 구조 인식 (H1: 100점, H2: 90점, 코드 펜스: 80점). 코드 블록 내부 분할 금지. `finalScore = baseScore × (1 - (distance/window)² × 0.7)`.

**v0.2 적용:**
- **하이브리드 검색** → `board__recall`의 백엔드로 QMD 사이드카 실행. Goose의 키워드 검색을 시맨틱으로 업그레이드
- **컬렉션 스코핑** → per-rig = 컬렉션, per-project = 기본 컬렉션. `includeByDefault`로 크로스-rig 검색 제어
- **Content-addressable** → CoW 스토어의 SHA-256과 철학 일치. 중복 임베딩 자동 제거
- **Strong signal bypass** → 에이전트가 빈번 검색할 때 불필요한 LLM 호출 절약
- **Intent** → 같은 쿼리를 다른 rig가 다른 의미로 검색 가능

**가져오지 않는 것:** Node.js 의존성 (사이드카로 실행하거나 검색 알고리즘만 Rust 포팅).

---

## 3. Letta Code (letta-ai/letta-code)

**무엇인가:** MemGPT 논문에서 발전한 메모리 우선 코딩 에이전트. "세션이 아닌 관계. 기억하는 코워커." TypeScript, 1.9k stars. Letta API 기반.

### 3계층 메모리 (MemGPT 아키텍처)

OS 가상 메모리 비유:

| OS 개념 | Letta 개념 | 설명 |
|---------|-----------|------|
| RAM | **Core/System Memory** | 항상 컨텍스트에. 매 턴마다 에이전트가 봄 |
| Disk | **Progressive/Archival** | 저장되어 있지만 컨텍스트에 없음. 도구로 명시적 조회 |
| Event Log | **Recall Memory** | 전체 대화 이력. 컨텍스트에서 빠져도 검색 가능 |

### Memory Filesystem (MemFS)

```
~/.letta/agents/{agent-id}/memory/
├── system/                    ← Core (항상 프롬프트에 핀)
│   ├── persona.mdx            ← "나는 누구인가" (에이전트가 진화시킴)
│   ├── human.mdx              ← "이 사용자에 대해 배운 것"
│   ├── project.mdx            ← "이 코드베이스 이해"
│   ├── style.mdx              ← "사용자의 코딩 선호"
│   └── memory_filesystem.mdx  ← 전체 트리 뷰 (read-only, 자동)
├── notes.md                   ← Progressive (필요 시 로드)
└── archive/
    └── old-decisions.md
```

**`memory_filesystem.mdx`** = 모든 메모리 파일의 "목차". 내용은 없고 트리만. 에이전트가 이걸 보고 필요한 것만 로드 (progressive disclosure). `prime()`의 1-2K 제한 극복.

### Git 기반 영속화

- 메모리 디렉토리 = git repo. 버전 관리, 충돌 해결, 감사 추적
- Pre-commit hook: frontmatter 검증 (`description`, `limit` 필수, `read_only` 보호)
- 서브에이전트가 worktree로 병렬 메모리 편집

### 에이전트 자기 편집

`persona.mdx`가 "I'm a coding assistant, ready to be shaped"로 시작 → 상호작용하며 진화. 에이전트가 **스스로** 정체성, 사용자 이해, 프로젝트 지식을 편집.

### Sleep-Time 메모리 통합

Reflection 서브에이전트가 백그라운드에서:
- 대화 트랜스크립트 리뷰
- 실수, 사용자 피드백, 비효율 식별
- 메모리 파일 업데이트 (git worktree에서 격리 편집)
- 생물학적 수면 시 기억 통합과 유사

### Skill 시스템 (절차적 메모리)

- `.skills/` 파일에 학습된 절차 저장 (프로젝트 → 에이전트 → 글로벌 계층)
- 궤적(trajectory) + 피드백에서 학습 → 스킬 `.md` 생성
- Terminal Bench 2.0에서 36.8% 상대 개선

**v0.2 적용:**
- **Progressive disclosure** → 메모리 트리 "목차"를 `prime()`에 포함. 에이전트가 필요한 것만 `board__recall`로 로드. 1-2K 토큰 제한 극복
- **Core vs Progressive 2계층** → 항상 주입 (project context, rig identity) vs 필요 시 검색 (과거 기록, 암묵지)
- **Frontmatter 검증** → 메모리 파일 스키마 강제. 에이전트 쓰레기 기록 방지
- **Reflection 서브에이전트** → 작업 완료 후 별도 에이전트가 학습 정리. Flight Record의 자동화 버전
- **에이전트 자기 편집** → "메모리는 에이전트 밖에" 원칙과 양립: 파일은 디스크에, 에이전트가 도구로 편집

**가져오지 않는 것:** Letta Cloud 서버, TypeScript 런타임, MemFS git 백엔드 (CoW 스토어와 중복).

---

## 종합: v0.2 메모리 설계에 미치는 영향

### Goose가 이미 제공하는 것

| 기능 | 상태 | 한계 |
|------|------|------|
| 자동 압축 (80%) | ✅ 내장 | pre-compaction flush 없음 |
| 세션 재개 (`--resume`) | ✅ 내장 | 단일 에이전트만 |
| 메모리 MCP (카테고리/키워드) | ✅ 내장 | 시맨틱 검색 없음, 스코핑 없음 |
| 세션 간 키워드 검색 | ✅ 내장 | 벡터 검색 없음 |

### v0.2가 추가해야 하는 것

| 기능 | 출처 | Phase | 복잡도 |
|------|------|-------|--------|
| **Pre-compaction flush** | OpenClaw | 2 (Rig) | 중간 — Goose 압축에 훅 |
| **`board__remember` / `board__recall`** | OpenClaw + Letta | 2 (Rig) | 낮음 — MCP 도구 |
| **Per-rig + per-project 스코핑** | Letta MemFS | 2 (Rig) | 낮음 — 디렉토리 구조 |
| **Progressive disclosure (메모리 트리)** | Letta | 2 (Rig) | 낮음 — 트리 렌더링 |
| **하이브리드 검색 (BM25 + 벡터)** | QMD | 후반 | 높음 — QMD 사이드카 또는 Rust 포팅 |
| **시간 감쇠 (30일 반감기)** | OpenClaw + Wasteland | 4 (Trust) | 낮음 — 점수 계산 |
| **Reflection 서브에이전트** | Letta | 후반 | 높음 — 별도 에이전트 수명주기 |

### 최소 실행 가능 메모리 (Phase 2)

```
Goose 내장 (무료):
  ├── 자동 압축 (80%)
  ├── 세션 재개
  └── 기본 메모리 MCP

v0.2 추가 (Phase 2):
  ├── Pre-compaction flush (Goose 압축 전 board__remember 트리거)
  ├── board__remember(content, scope) — 마크다운 파일 쓰기
  ├── board__recall(query, scope) — BM25 텍스트 검색 (벡터는 나중)
  ├── Per-rig 디렉토리: ~/.opengoose/rigs/{id}/memory/
  ├── Per-project 디렉토리: ~/.opengoose/projects/{project}/memory/
  └── 메모리 트리 (memory_filesystem) → prime()에 "목차" 포함

나중에 추가:
  ├── QMD 사이드카 → board__recall을 시맨틱 검색으로 업그레이드
  ├── 시간 감쇠 (일간 로그에만)
  └── Reflection 서브에이전트 (작업 후 학습 자동 정리)
```
