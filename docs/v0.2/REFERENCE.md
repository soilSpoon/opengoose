# 참조 프로젝트 — v0.2 설계 결정의 근거

> 성격별로 분리됨. 각 문서에서 상세 내용 참조.

## [Foundation](REFERENCE-foundation.md) — 직접 빌드 기반

| 프로젝트 | 영향 |
|----------|------|
| **Goose** (block/goose) | 에이전트 런타임 그 자체 (재구현하지 않음) |
| **Dolt** (dolthub/dolt) | CoW 스토어 + 3-way merge 컨셉 |
| **Beads** (steveyegge/beads) | Wanted Board 데이터 모델 + ready/prime/compact 알고리즘 |

## [Coordination](REFERENCE-coordination.md) — Pull 아키텍처, 신뢰, 운영

| 프로젝트 | 영향 |
|----------|------|
| **Wasteland** (steveyegge/wasteland) | Pull 루프 + stamps + 신뢰 사다리 + 졸업앨범 규칙 |
| **Gas Town** (steveyegge/gastown) | Witness, GUPP, Landing the Plane, 프랙탈 구조, seance |
| **Goosetown** (block/goosetown) | 단순함 철학, 보드를 통한 소통 |

## [Memory](REFERENCE-memory.md) — 에이전트 메모리, 검색, 세션 연속성

| 프로젝트 | 영향 |
|----------|------|
| **OpenClaw Memory** (openclaw.ai) | Pre-compaction flush, 2계층 메모리, 시간 감쇠 |
| **QMD** (tobi/qmd) | 하이브리드 검색 (BM25+벡터+리랭킹), content-addressable |
| **Letta Code** (letta-ai/letta-code) | Progressive disclosure, MemFS, reflection 서브에이전트 |

## [Execution](REFERENCE-execution.md) — 실행 패턴, 격리, 비교 참조

| 프로젝트 | 영향 |
|----------|------|
| **Stripe Minions** | 블루프린트, Toolshed, 컨텍스트 사전 수집, CI 2라운드 |
| **Ramp Inspect** | 샌드박스 격리, 스냅샷 워밍 |
| **Coinbase Cloudbot** | 관찰 가능성, 그래프 아키텍처 |
| **Open SWE** (langchain-ai) | 미들웨어 훅, 안전망 PR |
| **Portless** (vercel-labs/portless) | 포트 할당 + 네임드 URL + worktree 감지 |
| **AntFarm** (snarktank/antfarm) | progress.txt (Flight Record 참조), Ralph 루프 |
| **Fractals** (TinyAGI/fractals) | classify-before-decompose, lineage-as-context |
| **Agent Orchestrator** (ComposioHQ) | 에스컬레이션 체인, wakeup 병합, orchestrator-as-agent |
| **Paperclip** (paperclipai/paperclip) | 세션 컴팩션, 비용 추적, atomic checkout |

---

## 설계 계보

```
핵심 인프라 (우리가 직접 만드는 것):
  Dolt (prolly tree, branch/merge, 셀 레벨 diff)
    → CoW 스토어 + 3-way merge
  Beads (ready/prime/compact, wisps, 해시 ID, 의존성 그래프)
    → Wanted Board 데이터 모델 + 알고리즘
  Goose (Agent::reply(), MCP, Recipes, Sessions)
    → 에이전트 런타임 그 자체 (재구현하지 않음)

조율 모델 (우리의 핵심 차별점):
  Wasteland (pull 아키텍처, stamps, 신뢰, yearbook)
    → pull 루프 + 신뢰 모델 + 보드 설계
  Gas Town (프랙탈 팀, MEOW/Convoy, Wave 실행, Refinery, seance)
    → 운영 패턴 + 세션 연속성 참조

격리 + 실행:
  Portless (포트 할당, 네임드 URL, worktree 감지)
    → rig 격리 (포트 충돌 없음)
  Stripe/Ramp/Coinbase/Open SWE
    → 미들웨어 훅, 블루프린트 패턴, 컨텍스트 사전 수집, 안전망 PR

메모리 + 검색:
  OpenClaw (pre-compaction flush, 2계층 메모리, 시간 감쇠)
    → Goose 압축 전 기억 기록 기회, 에이전트 주도 메모리
  QMD (3단계 하이브리드 검색, content-addressable, GGUF 로컬)
    → board__recall 시맨틱 검색 백엔드
  Letta Code (MemFS, progressive disclosure, reflection 서브에이전트)
    → 메모리 트리 "목차", Core vs Progressive 2계층

비교 참조 (다른 접근법에서 배우는 것):
  AntFarm (progress.txt, Ralph 루프)
    → 세션 간 학습 영속화의 가장 단순한 형태
  Fractals (classify-before-decompose, lineage-as-context)
    → 과도한 분해 방지 게이트, 저비용 컨텍스트 주입
  Agent Orchestrator (에스컬레이션 체인, wakeup 병합, orchestrator-as-agent)
    → Witness 확장, L3 조율 rig 패턴
  Paperclip (세션 컴팩션, 비용 추적, 메모리 어댑터 계약)
    → 세션 연속성, blast radius에 비용 차원 추가
```
