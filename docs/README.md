# OpenGoose 문서

> **최종 정리:** 2026-03-12
> **총 문서:** 8개 (10,118줄 → 4,500줄로 통합)

---

## 아키텍처

| 문서 | 설명 |
|------|------|
| [storage-architecture.md](architecture/storage-architecture.md) | **스토리지 아키텍처 결정** — SQLite → prollytree 전환, Prolly Tree 기반 단일 바이너리 |
| [goose-deep-dive.md](architecture/goose-deep-dive.md) | Goose 소스 분석 + OpenGoose 활용 현황 |
| [opengoose-v2-architecture.md](architecture/opengoose-v2-architecture.md) | OpenGoose v2 전체 아키텍처 |

## 구현

| 문서 | 설명 |
|------|------|
| [opengoose-beads-redesign.md](implementation/opengoose-beads-redesign.md) | Beads TDD 재설계 — ready/prime/compact, Wisp, Landing the Plane |
| [api-reference.md](implementation/api-reference.md) | API 참조 문서 |

## 리서치

| 문서 | 설명 |
|------|------|
| [gastown-architecture-analysis.md](research/gastown-architecture-analysis.md) | Gas Town/Goosetown/Beads/Dolt/Wasteland 통합 분석 |

## 운영

| 문서 | 설명 |
|------|------|
| [codebase-review-2026-03.md](operations/codebase-review-2026-03.md) | 2026.03 코드베이스 리뷰 |
| [web-dashboard.md](operations/web-dashboard.md) | 웹 대시보드 설계 |

---

## 핵심 결정 요약

### 스토리지 아키텍처

```
결정: SQLite + Diesel → prollytree (순수 Rust Prolly Tree)

이유:
- 단일 바이너리 (외부 서버 불필요)
- 순수 Rust (C 의존성 제거)
- Prolly Tree 효율성 (구조적 공유, O(diff))
- 3-way Merge 내장
- Apache-2.0 라이선스

크레이트: prollytree = { version = "0.3.1", features = ["git", "sql"] }
```

### 사용 불가 도구

| 도구 | 이유 |
|------|------|
| **Dolt** | Go 서버 필요 (단일 바이너리 제약 위반) |
| **beads_rust** | Anthropic Rider 라이선스 (사용 금지) |
| **cr-sqlite** | 유지보수 중단 (2024.01 이후) |
| **dialog-db** | 실험적 단계, 문서 부족 |

### Beads 알고리즘 (자체 구현 ~500줄)

```rust
// prollytree 위에 구현
ready()    — 실행 가능한 태스크 필터링
prime()    — AI 컨텍스트 생성 (토큰 절약)
compact()  — 완료 태스크 요약
hash_id()  — SHA-256 + base36 충돌 방지 ID
```

---
