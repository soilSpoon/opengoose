# 참조: 조율 — Wasteland, Gas Town, Goosetown

> Pull 아키텍처, 신뢰 모델, 운영 패턴의 근거.

---

## 1. Wasteland (steveyegge/wasteland)

**무엇인가:** 분산 에이전트 연합 프로토콜. Gas Town 위의 스케일아웃 레이어.

**채택한 핵심 컨셉:**

### Wanted Board (Pull 기반 작업 분배)
상태 머신: `open → claimed → in_review → completed`
- 누구나 작업 게시 가능 (승인 게이트 없음)
- 에이전트가 자율적으로 탐색하고 claim (push가 아닌 pull)
- 오픈 현상금 모드: 여러 에이전트가 병렬로 작업, 첫 번째 유효한 솔루션이 승리

**v0.2:** 보드가 곧 우리의 Wanted Board. 모든 작업이 이를 통과한다.

### Stamps (다차원 평판)
- 차원: Quality (1-5), Reliability (1-5), Creativity (파생)
- 심각도 가중치: leaf=1pt, branch=3pt, root=5pt
- 모든 stamp은 근거가 있는 특정 완료 작업을 참조
- Append-only 장부 (이력 재작성 불가)

### 신뢰 사다리 (점진적 자율성)
L1 (< 3pt) → L1.5 (≥ 3) → L2 (≥ 10) → L2.5 (≥ 25) → L3 (≥ 50)
- 자연스러운 도제 과정: 좋은 작업 수행 → stamps 누적 → 결국 다른 이를 stamp

### 졸업앨범 규칙 (Yearbook Rule)
"다른 사람의 졸업앨범에는 서명할 수 있지만, 자기 것에는 안 된다."
- DB 레벨에서 `stamped_by != target_rig` 강제
- 평판은 다른 이가 당신에 대해 쓰는 것

### GUPP (추진 원칙)
"Hook에 작업이 있으면 실행해야 한다." 작업이 있을 때 에이전트는 절대 idle하지 않는다.
- 우리의 witness가 GUPP 위반 감지: 보드에 claim 가능한 작업이 있는데 에이전트가 idle

### Federation (Phase 2로 연기)
- 이식 가능한 ID를 위한 HOP URI: `hop://alice@example.com/rig-id/`
- Dolt를 통한 fork 기반 모델: upstream commons → fork → local clone → PR 반환
- 크로스 인스턴스 stamp 동기화

---

## 2. Gas Town (steveyegge/gastown)

**무엇인가:** 75k LOC Go, 17일간 vibecoding. Gas Town 인스턴스는 Wasteland에서 **단일 rig**로 등록되지만, 내부적으로는 Mayor + Witness + Deacon + Polecats + Refinery로 구성된 **사실상 팀**이다.

핵심 교훈: "설계가 병목" — 에이전트가 구현을 처리하면 아키텍처 결정이 제한 요소가 됨.

### 내부 아키텍처 (2단계)

**Town Level (`~/gt/.beads/`)** — 크로스 프로젝트 조율:

| 에이전트 | 역할 | 영속성 |
|----------|------|--------|
| **Mayor** | Chief-of-staff. 작업 분해(MEOW), 컨보이 생성, 사용자 통보 | 영속 |
| **Deacon** | 데몬 비콘. 순찰, 에이전트 생존 감시, 복구, 플러그인 관리 | 영속 |
| **Boot** | 워치독. Deacon 심박 5분마다 체크, 다운 시 재시작 | 일시적 |
| **Dogs (5)** | Deacon의 유지보수 crew (명령형 Go): Doctor, Reaper, Compactor, JSONL Backup, Dolt Backup | goroutines |

**Rig Level (`~/gt/<rig>/`)** — 프로젝트별:

| 에이전트 | 역할 | 영속성 |
|----------|------|--------|
| **Witness** | 핏 보스. Polecat 건강 감시, stuck 감지, MERGE_READY를 Refinery에 전달. 구현 작업 안 함 | 영속 |
| **Refinery** | Bors 스타일 머지 큐. 배치→이등분 전략 | 영속 |
| **Polecats** | 워커 (4-30개). 각각 git worktree 격리. 이름: Furiosa, Nux, Toast, Slit... | 영속 정체성, 일시적 세션 |
| **Crew** | 인간 관리 영속 워크스페이스 (전체 git clone) | 영속 |

에스컬레이션 체인: Agent → Deacon (해결 또는 전달) → Mayor (해결 또는 전달) → Overseer (인간).

### Polecat 3계층 수명주기

| 계층 | 영속성 | 수명 |
|------|--------|------|
| **Identity** | 영구 | 이름, agent beads, CV chain, 작업 이력. 절대 삭제 안 됨 |
| **Sandbox** | 지속 | 세션 간 생존. Worktree가 작업 사이에 main으로 sync |
| **Session** | 일시적 | 스텝마다 순환. Claude 컨텍스트 윈도우 리프레시 |

상태 전이: `IDLE → WORKING → DONE → IDLE` (파괴 단계 없음). 20-30개 병렬 실행 가능.

풀 설정:
```json
{ "polecat_pool_size": 4, "polecat_names": ["furiosa", "nux", "toast", "slit"] }
```

스케줄러: `max_polecats` (-1=직접, 0=비활성, N=용량 제한). `capacity = maxPolecats - activePolecats`. 3분 심박 주기, `spawn_delay`로 잠금 경합 방지.

### 작업 분해 (MEOW + Convoy)

**MEOW = Molecular Expression of Work.** 파이프라인:
```
Formula (TOML 템플릿)
  → Protomolecule (인스턴스화 가능 템플릿)
    → Molecule (활성 워크플로우 인스턴스)
      → Bead (원자 작업)
```

**Convoy** = 종속성 있는 작업 그룹. 3가지 생성 방법:
1. **Batch Sling**: `gt sling <bead1> <bead2>` → 자동 컨보이
2. **Explicit**: `gt convoy create "Auth" task1 task2`
3. **Stage-Launch (권장)**: `gt convoy stage <epic-id>` (BFS + Kahn's 알고리즘으로 DAG 구축) → `gt convoy launch` (Wave 1 디스패치)

**Wave 실행 모델** (병렬 작업의 핵심):
```
Wave 1: 의존성 없는 작업 → Polecat 병렬 실행
           ↓ (각 완료 → integration branch에 머지)
Wave 2: Wave 1 결과에 의존하는 작업 → integration branch에서 분기
           ↓
Wave N: ...
           ↓
전체 완료 → integration branch를 main에 atomic 머지
```

리프 작업 유형만 디스패치 (task, bug, feature, chore). Epic/decision/convoy는 스킵. `blocks`, `conditional-blocks`, `waits-for` 의존성 존중. parent-child는 **절대 블로킹 아님**.

### Refinery (Bors 스타일 머지)

1. MR A-D를 main 위에 스택으로 리베이스
2. 스택 팁에서 테스트
3. 통과 → 전부 fast-forward 머지
4. 실패 → 이등분 탐색으로 실패 MR 격리

정리 파이프라인: Polecat `gt done` (푸시+MR) → Witness POLECAT_DONE (검증+MERGE_READY) → Refinery (품질 게이트+squash-merge) → Witness MERGED (main 확인).

### Push vs Pull 공존 (프랙탈 구조)

Gas Town은 자기 자신을 **프랙탈**로 설명한다:

| 스케일 | 단위 | 메커니즘 |
|--------|------|----------|
| Federation | Wasteland rig | Pull (reputation stamps) |
| Town | Mayor | 결정론적 디스패치 (GUPP) |
| Crew | Specialist | 도메인 전문성 + 하위 에이전트 |
| Worker | Polecat | 자율 실행 |
| Tool | CLI 명령 | 결정론적 출력 |

> "각 에이전트는 미니-타운이다. 위로 능력을 광고하고, 아래로 위임하고, 내부적으로 로컬 지식에 기반해 할당을 결정한다."

Gas Town 내부는 **순수 push가 아니라 "결정론적 디스패치 + 자율 실행"**:
- 외부 (Wasteland): Gas Town이 Wanted Board에서 자율적으로 claim (pull)
- 내부 (Gas Town): Mayor가 분해 → Convoy → Wave → Polecat hook에 배치
- 실행: GUPP에 의해 자율 실행 (Witness는 관찰만, 완료를 게이트하지 않음)

### 세션 연속성: seance

```
세션 A (컨텍스트 가득 참) → /handoff → Landing the Plane → 세션 B 시작
→ gt seance: 세션 A의 대화에서 관련 부분 검색 → 세션 B에 주입
```

에이전트 자체는 기억 안 함. tmux 세션 로그가 남아있고, seance가 검색하는 구조. **메모리가 인프라(tmux + Dolt)에 있다.**

### Landing the Plane (세션 종료 프로토콜)

1. **FILE** — 미완료 작업을 태스크로 기록
2. **GATE** — 품질 검사 실행 (lint, test)
3. **UPDATE** — 완료 항목 닫기, 진행 중 항목 주석
4. **SYNC** — git push (비협상)
5. **VERIFY** — 깨끗한 작업 트리 확인
6. **HANDOFF** — `ready()`로 다음 작업 선택 (Beads 알고리즘 사용)

### 샌드박스 격리 (3단계)

| 모드 | 격리 | 특징 |
|------|------|------|
| **Local** (현재) | tmux 세션, 풀 파일시스템 접근 | 기본값, 보안 한계 인정 |
| **exitbox** (로컬 샌드박스) | 파일시스템/네트워크 정책 | worktree만 읽기/쓰기, Dolt만 네트워크 |
| **daytona** (원격 컨테이너) | 클라우드 Linux, mTLS | 인터넷 차단, polecat별 인증서 |

브랜치 스코프 인가: polecat은 `polecat/<cn-name>-*` refs만 푸시 가능.

**v0.2 적용:**
- Wave 실행 + integration branch → 우리의 CoW branch + 3-way merge로 같은 효과
- Polecat 3계층 수명주기 → Rig의 영속 정체성 + 일시적 Goose 세션과 동일
- 프랙탈 구조 → v0.2의 Rig가 하위 작업을 만들면 미니-타운 효과
- seance → v0.2에서 세션 컴팩션 + Flight Record로 대체 검토

**가져오지 않는 것:** Tmux 기반 프로세스 격리 (Goose가 처리), Go 코드베이스, Mail 시스템, Convoy TOML (보드 relations로 대체).

---

## 3. Goosetown (block/goosetown)

Block의 미니멀 Gas Town 변형. Conductor → Instruments.
- gtwall: bash 파일 기반 append-only 브로드캐스트 (~400줄)
- Village Map: 에이전트 애니메이션이 있는 시각적 대시보드

**가져오는 것:** 단순함 철학 (개념당 파일 하나), 보드를 통한 소통 영감.
**가져오지 않는 것:** Push 모델 (Conductor가 작업 할당).
