// Middleware — Before/After 훅
//
// pre_hydrate(): 에이전트 루프 전 결정론적 컨텍스트 수집
// post_execute(): 에이전트 완료 후 검증 + 안전망
//
// 내장 미들웨어:
// - ContextHydrator (AGENTS.md, 워크스페이스 파일)
// - ValidationGate (lint + test)
// - SafetyNet (커밋/PR 자동 생성)
// - BoundedRetry (CI 최대 2라운드)
