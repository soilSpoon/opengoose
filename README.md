# OpenGoose

Block의 [Goose](https://github.com/block/goose) AI 코딩 에이전트를 여러 개 동시에 돌려서 작업을 분산 처리하는 오케스트레이션 프레임워크.
에이전트들이 Wanted Board(작업 큐)에서 할 일을 스스로 가져가고, 격리된 git worktree에서 실행하고, 테스트를 돌리고, 결과를 제출한다.

## 사용 예시

```bash
# 대화형 채팅 모드 (Operator)
opengoose

# 보드에 작업 등록
opengoose board create "auth 미들웨어 리팩토링"

# 단일 작업을 헤드리스로 실행
opengoose run "README 업데이트"
```

## 원칙

- **Goose가 일한다.** OpenGoose는 에이전트 실행과 작업 분배만 담당한다.
- **Pull, not push.** 에이전트가 Wanted Board에서 작업을 직접 가져간다.
- **두 가지 모드.** Operator(대화형 채팅)와 Worker(백그라운드 풀 루프).

## 구조

```
crates/
├── opengoose/           # CLI 바이너리, TUI, 웹 대시보드
├── opengoose-board/     # Wanted Board + Beads + CowStore (SQLite + in-memory CoW)
├── opengoose-rig/       # Agent Rig (Operator: 채팅, Worker: 풀 루프)
├── opengoose-skills/    # 스킬 카탈로그 로딩 및 관리
├── opengoose-evolver/   # 스탬프 기반 자동 스킬 진화
└── opengoose-sandbox/   # 실험적 macOS HVF microVM 샌드박스
```

자세한 설계: [docs/v0.2/ARCHITECTURE.md](docs/v0.2/ARCHITECTURE.md)

## 참조 프로젝트

Goose, Dolt, Beads, Wasteland, Portless, Gas Town, Open SWE, Stripe Minions, Ramp Inspect.

## License

MIT
