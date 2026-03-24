# FP Quality Sweep v2 — Design Spec

## Overview

OpenGoose 코드베이스(4 crates, ~21K LOC, 593 tests)의 종합 코드 품질 개선.
단일 PR로 전체 sweep — 에러 핸들링, pure function 추출, proptest 도입, 테스트 코드 품질.

현재 사용자 없음, 배포 없음 — 공격적 변경 허용.

## Approach: Top-Down (Impact-First)

가장 문제가 큰 파일부터 공략. 파일 단위로 unwrap 제거 + pure function 추출 + 테스트 추가를 한번에 처리.

## Error Handling: Hybrid

**원칙: 발견 기반 설계**

에러 타입을 미리 설계하지 않는다. unwrap 제거 과정에서 발견한다.

1. `.unwrap()` → `?` + `.context("why")`. anyhow 유지.
2. 호출자가 match해서 복구해야 하는 에러 경로 식별 시 → 도메인 에러 enum 승격.

**크레이트별:**
- `opengoose-board` — `BoardError` 기존 유지, variant 추가 시에만 확장.
- `opengoose-rig`, `opengoose-skills` — 필요 발견 시에만 `thiserror` 도메인 에러 생성.
- `opengoose` (bin) — anyhow로 최종 수집.

## Phase 순서 (단일 PR, 논리 커밋)

### Phase 1 — 고밀도 파일 sweep

파일별로 동일한 3-step 패턴 적용:

**Step 1: Pure function 추출**
- 큰 함수에서 부수효과 없는 로직을 별도 함수로 분리
- 검증 로직, 데이터 변환, 문자열 조합, 조건 판단
- 추출된 함수는 `&self` 없이 입력→출력만

**Step 2: unwrap 제거**
- `?.context("why")` 패턴으로 교체
- 함수 시그니처 `Result<T>` 변경 시 호출자 연쇄 수정

**Step 3: 테스트 추가**
- 추출된 pure function마다 단위 테스트
- 정상 경로 + 에러 경로 + 경계값

**대상 파일 (우선순위 순):**

| # | 파일 | LOC | unwrap | tests | 문제 |
|---|---|---|---|---|---|
| 1 | `web/api/skills.rs` | 763 | 84 | 13 | 최고 unwrap 밀도, 최저 테스트 커버리지 |
| 2 | `manage/promote.rs` | 423 | 73 | 15 | 중첩 경로 로직 |
| 3 | `manage/add.rs` | 414 | 57 | 12 | clone → temp dir 로직 |
| 4 | `evolver/sweep.rs` | 821 | 77 | 40 | 복잡한 재평가 로직 |
| 5 | `evolver/pipeline.rs` | 966 | 75 | 141 | 이미 잘 테스트됨, unwrap 정리 중심 |

### Phase 2 — proptest 도입

**CRDT/merge (수학적 속성):**
- `merge(a, merge(b, c)) == merge(merge(a, b), c)` — 결합법칙
- `merge(a, b) == merge(b, a)` — 교환법칙
- `merge(a, a) == a` — 멱등성

**상태 전이 (불변 조건):**
- 임의의 `Vec<Transition>` 시퀀스 생성
- 유효하지 않은 전이 → 항상 에러 반환
- 유효한 전이 후 → 상태 올바름

**파서/검증 (robustness):**
- 임의 문자열 입력 시 패닉 없음 (Ok 또는 Err)
- roundtrip 가능한 곳: `parse(format(x)) == x`

### Phase 3 — 나머지 프로덕션 unwrap 정리

중밀도 파일 일괄 처리: middleware, work_items, relations 등.

### Phase 4 — 테스트 코드 품질

전체 테스트 코드에서 `.unwrap()` → `.expect("reason")` 교체.

## FP Patterns

| 패턴 | 적용 기준 |
|---|---|
| Pure function 추출 | 부수효과 함수에서 순수 로직 분리 |
| Iterator chain | for loop + mut accumulator → `.iter().filter().map().collect()` |
| Option/Result combinator | `.map()`, `.and_then()`, `.unwrap_or_else()` — 가독성 유지 시에만 |
| Guard clause + `?` | 깊은 중첩 → 조기 반환으로 평탄화 |
| Named abstraction | 복잡한 클로저/체인에 의미 있는 이름 |

**적용 기준**: 가독성이 나빠지면 적용하지 않는다. FP는 수단이지 목적이 아님.

## Testing Strategy

1. **Pure function 추출 → 단위 테스트** — 추출과 동시에 테스트 작성
2. **proptest** — CRDT, 상태 전이, 파서/검증에 프로퍼티 기반 테스트
3. **에러 경로** — happy path + 실패 케이스
4. **테스트 코드 품질** — `.unwrap()` → `.expect("reason")`, 반복 setup → 헬퍼 추출

## Constraints

- 제한 없음. 공개 API, 타입 시그니처, 모듈 구조 자유 변경.
- 컴파일 통과 + 테스트 통과가 유일한 기준.
- 단일 PR, 논리 커밋 단위로 분리.
