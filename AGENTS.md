# AGENTS.md — OpenGoose v0.2

## 프로젝트 개요

OpenGoose v0.2는 Goose AI 에이전트 프레임워크 위에 pull 기반 멀티에이전트 조율 레이어를 구축한다.

## 아키텍처

- **6개 크레이트:** `opengoose` (CLI), `opengoose-board` (데이터), `opengoose-rig` (에이전트), `opengoose-skills` (스킬), `opengoose-evolver` (스킬 진화), `opengoose-sandbox` (실험적)
- **Pull 아키텍처:** 모든 작업이 Wanted Board를 통과. 에이전트가 자율적으로 claim.
- **Goose-native:** `Agent::reply()`가 유일한 LLM 인터페이스. Goose의 MCP, 세션, 컨텍스트 관리를 100% 재사용.

## 핵심 규칙

1. Goose가 이미 제공하는 것을 재구현하지 않는다.
2. 하위 크레이트는 상위 크레이트에 의존하지 않는다: board → rig → evolver → opengoose. skills는 독립.
3. board 크레이트는 LLM, 세션, 플랫폼에 대해 아무것도 모른다.
4. CLI만 지원. Discord/Slack 등 플랫폼 게이트웨이 없음.

## 설계 문서

- `docs/v0.2/ARCHITECTURE.md` — 전체 아키텍처
- `docs/v0.2/REFERENCE.md` — 참조 프로젝트 분석
