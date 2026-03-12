# Gastown Architecture Summary

Gastown is a multi-agent orchestration paradigm designed to manage dozens of agents in parallel, eliminating the "baby-sitting" overhead often associated with AI agents.

## Core Philosophy

### 1. Research-First, Build-Second
Every complex task begins with parallel research. 에이전트 무리가 독립적으로 조사하여 3개 소스에서 80% 신뢰도를 확보하면 구현으로 전환한다.

### 2. Propulsion Principle (추진 원칙)
에이전트는 즉시 실행한다. 질문하거나 예절을 차리기보다 작업을 우선시하며, 모든 지연은 시스템의 정지로 간주한다.

### 3. Context is Finite
오케스트레이터(Mayor)는 직접 코드를 작성하지 않고 위임만 수행하여 자신의 컨텍스트 윈도우를 보호한다.

### 4. Write as You Go
에이전트는 중간 산출물을 지속적으로 디스크에 남긴다. 세션이 크래시되더라도 작업의 일부가 보존되어 다음 에이전트가 이어받을 수 있다.

## Key Mechanisms

- **Hierarchical Supervision**: Mayor -> Witness -> Polecat 순으로 위임과 감시를 수행한다.
- **Git Worktrees**: 에이전트별 독립된 작업 공간을 제공하여 머지 충돌을 방지한다.
- **Refinery**: Bors 스타일의 머지 큐를 운영하며, 충돌 발생 시 "re-imagine" 에이전트를 통해 통합 구현을 생성한다.
- **Dolt**: Git 시맨틱을 가진 SQL 데이터베이스를 활용하여 구조화된 데이터의 버전 관리와 머지를 수행한다.

---

*For full research details, see [Gastown Archive](../../90-archive/gastown-full-research.md).*
