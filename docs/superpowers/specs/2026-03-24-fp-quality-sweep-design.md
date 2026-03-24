# FP Quality Sweep — Design Spec

## Overview

OpenGoose 코드베이스(4 crates, ~20.7K lines)의 전반적인 코드 품질 개선.
실용적 함수형 프로그래밍 패턴 적용, 대형 파일 분해, 에러 핸들링 강화, 테스트 커버리지 확대.

현재 사용자 없음, 배포 없음 — 공격적 변경 허용.

## Approach: Risk-First Hybrid

에러 인프라를 먼저 구축하고, 가장 크고 복잡한 파일부터 분해+개선. 각 파일을 한 번만 만져서 효율적으로 처리.

### Phase 순서

1. **에러 인프라** — unwrap() → `?` + `.context()` 전환. 호출자가 실제로 match해서 복구하는 경로만 도메인 에러 enum으로 승격.
2. **고위험 파일 개선** — 500줄+ & 다중 책임 파일을 분해 + unwrap 제거 + FP 패턴 + 테스트를 한 파일 단위로 완결.
3. **나머지 정리** — 남은 500줄+ 파일 동일 처리.
4. **테스트 커버리지 갭** — 변경되지 않은 모듈의 커버리지 보완.

## Error Handling

### 원칙: 발견 기반 설계

에러 타입을 미리 설계하지 않는다. unwrap 제거 과정에서 발견한다.

1. **1차** — unwrap() → `?` + `.context("설명")`. anyhow 유지.
2. **2차** — 호출자가 match해서 복구해야 하는 에러 경로 식별.
3. **3차** — 해당 경로만 도메인 에러 enum으로 추출.

### 크레이트별 구조

- `opengoose-board` — `BoardError` 기존 유지. 필요시 variant 추가.
- `opengoose-rig` — `RigError` 필요시 신규 생성.
- `opengoose-skills` — `SkillError` 필요시 신규 생성.
- `opengoose` (bin) — anyhow로 최종 수집.

## File Decomposition

### 대상: ~500줄+ & 다중 책임 (경계 부근 파일 포함)

| 파일 | 줄수 | 분해 방향 |
|---|---|---|
| `evolver.rs` | 1,744 | stamp listener / skill generation loop / orchestration |
| `web/api.rs` | 1,485 | 리소스별 핸들러 모듈 (board, rigs, stamps, skills) |
| `tui/event.rs` | 1,261 | 이벤트 종류별 핸들러 (key, board, rig) |
| `work_items.rs` | 926 | 쿼리 vs 상태전이 vs CRUD |
| `main.rs` | 829 | CLI 파싱 / 부트스트랩 / 서브커맨드 |
| `skills/evolve.rs` | 689 | 진화 로직 vs 파일 생성 vs 검증 |
| `skills/load.rs` | 669 | 파일시스템 탐색 vs 파싱 vs 캐싱 |
| `rig.rs` | 590 | 스트림 소비 vs 미들웨어 vs 모드별 실행 |
| `tui/ui.rs` | 542 | 탭별 렌더링 (chat, board, logs) |
| `mcp_tools.rs` | 528 | 도구 정의 vs 실행 로직 |
| `manage/discover.rs` | 524 | 탐색 로직 vs 필터링 vs 결과 매핑 |
| `tui/app.rs` | 510 | 상태 관리 vs 액션 디스패치 |
| `conversation_log.rs` | 486 | 읽기 vs 쓰기 vs 필터링 |
| `store.rs` | 474 | CowStore 핵심 vs 브랜치 관리 vs merge |

### 원칙

- 분해 후 각 파일은 단일 책임.
- `mod.rs` 또는 디렉토리 모듈로 re-export — 외부 인터페이스 유지.
- 분해 전에 안전망 테스트 확인/추가.
- 실제 분해 경계는 구현 시 코드 분석 후 확정.

## FP Patterns

### 적용 패턴

**1. 순수 함수 추출**
- 부수효과(DB, 파일IO, 네트워크) 함수에서 순수 로직 분리.
- 분리된 순수 함수는 단위 테스트 가능.

**2. Iterator chain 일관화**
- for loop + mut accumulator → `.iter().filter().map().collect()`.

**3. Option/Result combinator**
- `.map()`, `.unwrap_or_else()`, `.and_then()` 상황에 맞게.
- 가독성이 떨어지면 if let 유지.

**4. 조기 반환으로 중첩 줄이기**
- 깊은 match/if → guard clause + `?` 평탄화.

**5. 명명된 추상화**
- 복잡한 클로저/체인에 의미 있는 이름 부여.

### 적용 기준

가독성이 나빠지면 적용하지 않는다. FP는 수단이지 목적이 아님.

## Testing Strategy

### 1. 리팩터링 안전망 (먼저)

- 분해/변경할 파일의 기존 동작 캡처 테스트 먼저 추가.
- 리팩터링 후 동일 테스트 통과 = 동작 보존 증명.

### 2. 기존 테스트 품질 개선

- 테스트 내 unwrap() → `.expect("설명")` 또는 assert_matches.
- 반복 setup → 테스트 헬퍼/fixture 추출.
- 동작 설명 테스트 이름 (`claim_open_item_succeeds`).

### 3. 커버리지 갭

- 순수 함수 추출 → 단위 테스트 자연 확대.
- 분해된 모듈마다 테스트 포함.
- 에러 경로 테스트 — happy path + 실패 케이스.

### 4. 구조

- `#[cfg(test)] mod tests` 패턴 유지.
- 통합 테스트 필요 시에만 `tests/` 사용.
- 테스트 헬퍼는 크레이트별 `testutil.rs`에 집중.

## Constraints

- 제한 없음. 공개 API, 타입 시그니처, 모듈 구조 자유 변경.
- 컴파일 통과 + 테스트 통과가 유일한 기준.

## PR Strategy

- 에러 인프라: 1 PR
- 고위험 파일 묶음: 3~4 PR
- 테스트 커버리지 갭: 1 PR
