# OpenGoose v0.2

Goose-native pull 아키텍처 + Wasteland 수준 에이전트 자율성.

## 원칙

- **Goose가 에이전트 작업을 한다.** OpenGoose는 조율만 한다.
- **Pull, not push.** 에이전트가 Wanted Board에서 작업을 가져간다.
- **3개 크레이트.** 그 이상은 없다.

## 구조

```
crates/
├── opengoose/         # CLI (대화형 + 헤드리스)
├── opengoose-board/   # Wanted Board + Beads + CoW Store
└── opengoose-rig/     # Agent Rig (영속 pull 루프)
```

## 참조 프로젝트

Goose, Dolt, Beads, Wasteland, Portless, Gas Town, Open SWE, Stripe Minions, Ramp Inspect.

자세한 설계: [docs/v0.2/ARCHITECTURE.md](docs/v0.2/ARCHITECTURE.md)

## License

MIT
