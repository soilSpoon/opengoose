// Board Platform Extension — 에이전트가 보드에 접근하는 내장 도구
//
// Goose의 Platform Extension (McpClientTrait 직접 구현)으로 내장.
// 별도 프로세스/바이너리 없음. MCP JSON-RPC 직렬화 오버헤드 제로.
//
// 도구 목록 (board__ 접두사로 노출):
//   board__claim_next, board__create_task, board__update_status,
//   board__delegate, board__broadcast, board__read_board, board__stamp
