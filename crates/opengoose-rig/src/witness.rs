// Witness — Stuck/Zombie 감지
//
// AgentEvent 스트림 모니터링.
// 5분 무응답 → Stuck, 10분 → Zombie → CancellationToken.
// GUPP 위반 감지: 보드에 작업 있는데 idle.
