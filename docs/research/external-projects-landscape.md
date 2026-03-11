# 외부 프로젝트 랜드스케이프 분석

> 작성일: 2026-03-11
> 소스: GitHub 레포지토리, GeekNews, TechCrunch, MarkTechPost, The New Stack, 박재홍의 실리콘밸리

---

## 개요

AI 에이전트 코딩 생태계가 빠르게 성장하면서, Gastown/Goosetown 패러다임 외에도 다양한 접근 방식이 등장하고 있다. 이 문서는 주목할 만한 외부 프로젝트 3개를 분석하고, Gastown 생태계와의 관계 및 차이점을 정리한다.

---

## 1. Autoresearch (Andrej Karpathy)

- **저장소**: [github.com/karpathy/autoresearch](https://github.com/karpathy/autoresearch)
- **공개일**: 2026-03-07
- **언어**: Python (~630줄)
- **라이선스**: MIT

### 1.1 컨셉

> "인간은 코드를 작성한다 → AI에게 코드를 작성하라고 말한다 → AI에게 코드 작성에 대해 어떻게 생각할지를 말한다. 각 단계에서 인간 개입이 한 겹씩 제거된다."

Autoresearch는 **AI 에이전트가 밤새 자율적으로 ML 실험을 수행**하는 프레임워크다. 인간이 잠든 동안 에이전트가 코드를 수정하고, 학습하고, 평가하고, 결과가 좋으면 유지하고 나쁘면 버리는 과정을 반복한다.

### 1.2 아키텍처

```
┌─────────────────────────────────────────┐
│              program.md                  │ ← 인간이 작성하는 유일한 "코드"
│         (에이전트 행동 지시서)             │    마크다운으로 연구 방향 설정
├─────────────────────────────────────────┤
│              train.py                    │ ← 에이전트가 수정하는 유일한 파일
│    (GPT 모델 + 옵티마이저 + 학습 루프)     │    ~630줄, 전체가 LLM 컨텍스트에 적재됨
├─────────────────────────────────────────┤
│              prepare.py                  │ ← 불변 인프라
│       (데이터 준비 + 토크나이저 + 평가)     │    에이전트가 수정하지 않음
└─────────────────────────────────────────┘
```

**핵심 설계 원칙:**

| 원칙 | 설명 |
|------|------|
| **고정 시간 예산** | 모든 실험이 정확히 5분간 학습. 하드웨어 무관하게 결과 비교 가능 |
| **단일 메트릭** | val_bpb (validation bits-per-byte). 어휘 크기에 독립적 |
| **Git 기반 이력** | 개선된 변경은 feature 브랜치에 커밋. 악화된 변경은 폐기 |
| **단일 파일 수정** | 에이전트는 train.py만 수정 가능. 범위 제한으로 안전성 확보 |

### 1.3 program.md 패러다임

Autoresearch의 가장 독창적인 아이디어는 **program.md**다. 연구자는 Python 코드를 직접 수정하지 않고, 마크다운 문서로 에이전트의 "연구 방향"을 프로그래밍한다.

```
기존 방식:  연구자 → Python 코드 수정 → 실험 실행
Autoresearch: 연구자 → program.md 수정 → 에이전트가 Python 수정 → 실험 실행
```

이는 **"연구 조직 코드를 프로그래밍하는 것"**이라고 Karpathy가 표현한다. CLAUDE.md나 .cursorrules와 유사한 접근이지만, 범위가 "ML 연구 자동화"로 특화되어 있다.

### 1.4 초기 결과

- Karpathy 본인: val_bpb 1.0 → 0.97 (자율 실험)
- Shopify CEO Tobi Lutke: 에이전트가 최적화한 소형 모델이 수동 구성된 대형 모델을 능가 (**19% 성능 향상**)
- 하룻밤 ~100개 실험 (12실험/시간 × 8시간)

### 1.5 Gastown 생태계와의 비교

| 차원 | Autoresearch | Gas Town |
|------|-------------|----------|
| **에이전트 수** | 1개 (순차 실험) | 20~30개 (병렬 작업) |
| **대상 도메인** | ML 학습 최적화 | 범용 소프트웨어 개발 |
| **감독 체계** | 없음 (자율 루프) | Mayor/Witness/Deacon 계층 |
| **머지 전략** | 불필요 (단일 파일) | Refinery (자동 머지 큐) |
| **메모리** | Git 커밋 이력 | Beads (영속적 이슈 트래커) |
| **복잡도** | 630줄 | 300+ Go 파일 |
| **인간 개입** | program.md 작성 후 방치 | Mayor가 주기적 방향 조정 |

**핵심 차이:** Autoresearch는 "하나의 에이전트, 하나의 파일, 하나의 메트릭"이라는 극도로 제약된 환경에서 작동한다. Gas Town은 "많은 에이전트, 많은 파일, 복합 목표"라는 현실적 복잡성을 다룬다. 두 프로젝트의 철학은 상반되지만 보완적이다.

---

## 2. Entire (Thomas Dohmke, 전 GitHub CEO)

- **저장소**: [github.com/entireio/cli](https://github.com/entireio/cli)
- **공개일**: 2026-02-10
- **언어**: Go
- **라이선스**: Apache 2.0
- **펀딩**: $60M 시드 (기업가치 $300M), Felicis/M12/Madrona 등
- **GeekNews**: [Entire - AI 에이전트를 위한 새로운 개발자 플랫폼](https://news.hada.io/topic?id=26583)

### 2.1 컨셉

> "GitHub는 인간 대 인간 상호작용을 위해 구축되었다. 이슈에서 풀 리퀘스트, 배포에 이르기까지 전체 시스템이 AI 에이전트의 새로운 시대를 위해 설계되지 않았다." — Thomas Dohmke

Entire는 **AI 에이전트 시대의 "관찰 가능성(observability) 레이어"**다. 에이전트가 코드를 생성할 때, 그 코드뿐만 아니라 **왜 그런 결정을 했는지**(프롬프트, 추론 과정, 도구 호출, 토큰 사용량)를 함께 버전 관리한다.

### 2.2 아키텍처

```
┌──────────────────────────────────────────────┐
│                AI 에이전트                      │
│  (Claude Code, Gemini CLI, Cursor, Copilot)   │
├──────────────────────────────────────────────┤
│            Entire Checkpoints CLI              │ ← Git 훅으로 자동 캡처
│  ┌────────────┬────────────┬───────────────┐  │
│  │ Transcript │  Prompts   │  Tool Calls   │  │
│  │  (대화록)   │ (프롬프트)  │ (도구 호출)    │  │
│  ├────────────┼────────────┼───────────────┤  │
│  │ Files      │  Tokens    │  Decisions    │  │
│  │ (수정 파일) │ (토큰 사용) │ (결정 추적)    │  │
│  └────────────┴────────────┴───────────────┘  │
├──────────────────────────────────────────────┤
│              Git Repository                    │
│  main branch: 소스 코드                        │
│  entire/checkpoints/v1 branch: 세션 메타데이터  │ ← 코드와 분리된 별도 브랜치
└──────────────────────────────────────────────┘
```

**3계층 플랫폼 비전:**

| 계층 | 설명 | 현재 상태 |
|------|------|----------|
| **Git 호환 데이터베이스** | 코드 + 프롬프트 + 제약 조건 + 결정 사항 + 실행 추적을 커밋과 함께 기록 | Checkpoints CLI로 첫 단계 출시 |
| **시맨틱 추론 레이어** | 여러 AI 에이전트가 함께 작업할 수 있는 범용 추론 계층 | 개발 중 |
| **AI 네이티브 UI** | 에이전트-인간 협업을 위한 전용 인터페이스 | 개발 중 |

### 2.3 핵심 기능: Checkpoints

```bash
# 프로젝트에서 활성화
cd your-project && entire enable

# 자동 동작: 에이전트 세션마다 컨텍스트를 캡처
# 수동 복원: 에이전트가 잘못된 방향으로 갔을 때
entire rewind --to <checkpoint-id>   # 비파괴적: 커밋 이력을 변경하지 않음
```

**Git Worktree 지원:** 각 worktree에서 독립적인 세션 추적이 가능하다. 여러 AI 세션을 다른 worktree에서 동시 실행해도 충돌하지 않는다.

**보안:** 세션 트랜스크립트가 Git에 저장되므로, 공개 레포의 경우 누구나 볼 수 있다. API 키, 토큰, 자격 증명 등의 시크릿은 자동 검출/편집(best-effort)된다.

### 2.4 Gastown 생태계와의 비교

| 차원 | Entire | Gas Town + Beads |
|------|--------|-----------------|
| **핵심 관심사** | 에이전트 투명성/감사(audit) | 에이전트 자율 운영 |
| **에이전트 운영** | 관여하지 않음 (관찰만) | 직접 오케스트레이션 |
| **메타데이터 저장** | Git 별도 브랜치 | Beads (Git 커밋 내 이슈), Dolt (DB) |
| **머지 처리** | 없음 | Refinery (자동 머지 큐) |
| **Worktree 지원** | 독립적 세션 추적 | 에이전트당 worktree 격리 |
| **범위** | 단일 에이전트 세션 기록 | 다수 에이전트 조율 + 기록 |
| **비즈니스 모델** | 상용 플랫폼 ($60M 투자) | 오픈소스 프로토타입 |

**핵심 차이:** Entire는 "에이전트가 무엇을 왜 했는지"를 추적하는 **사후 분석 도구**다. Gas Town은 "에이전트가 무엇을 해야 하는지"를 지시하는 **사전 조율 도구**다. 두 레이어는 상호 보완적이며, 이론적으로 Gas Town의 Polecat이 Entire Checkpoints를 활성화한 채로 작업하면 양쪽 이점을 모두 얻을 수 있다.

**OpenGoose에의 시사점:** Entire의 Checkpoints 모델은 OpenGoose의 세션 로깅 전략에 직접적인 참고가 된다. 특히 `entire/checkpoints/v1` 브랜치 패턴은 코드와 메타데이터의 분리라는 깔끔한 설계를 제시한다.

---

## 3. 박재홍의 실리콘밸리 — AI 에이전트 생태계 분석

- **블로그**: [wikidocs.net/blog/@jaehong](https://wikidocs.net/blog/@jaehong/)
- **대표 저서**: "바이브 코딩 (AI 코딩 에이전트 사용법)", "Claude Code 가이드: AI 에이전틱 코딩 시작하기"
- **참고**: [wikidocs.net/blog/@jaehong/8970](https://wikidocs.net/blog/@jaehong/8970/) (접근 제한으로 전문 미확인)

### 3.1 핵심 관점: "세션 로그가 진짜 자산이다"

박재홍의 블로그에서 반복적으로 등장하는 주장:

> "에이전트가 만들어낸 코드와 그 코드가 만들어지기까지의 과정, 둘 중 더 가치 있는 건 뭘까? 나는 후자라고 생각한다. 즉, 세션 로그가 코딩 에이전트의 핵심 자산이다."
> — [코딩 에이전트의 진짜 자산은 코드가 아니라 세션 로그다](https://wikidocs.net/blog/@jaehong/8086/)

이 관점은 Entire의 Checkpoints와 정확히 같은 문제 인식을 공유하며, Gastown의 Beads가 해결하려는 "에이전트 메모리 영속화"와도 맞닿아 있다.

### 3.2 주요 분석 글 요약

| 글 | 핵심 주장 | Gastown 생태계와의 관련성 |
|----|----------|------------------------|
| **랄프 루프 (Ralph Loop)** | 2026년을 정의할 AI 코딩 기법. 자율 반복(autonomous loop) 패턴 | Gas Town의 Polecat 실행 루프와 동일한 패턴 |
| **세션 로그가 진짜 자산** | 코드보다 과정이 더 가치 있다 | Beads의 존재 이유와 정확히 일치 |
| **Claude Cowork의 10GB VM 번들** | 보안과 사용성 사이의 트레이드오프 | Gas Town의 worktree 격리 vs VM 격리 논쟁 |
| **GitHub Agentic Workflows** | AI가 매일 아침 PR을 만들어주는 세상 | Gas Town이 이미 구현한 자율 PR 생성 |
| **Show HN이 죽어가고 있다** | 바이브코딩 시대, 사이드 프로젝트의 품질 위기 | 에이전트 감독(Witness)의 필요성을 방증 |

### 3.3 OpenGoose에의 시사점

박재홍의 분석은 한국어권에서 AI 에이전트 코딩 생태계를 가장 체계적으로 추적하는 소스 중 하나다. 특히 "세션 로그 = 핵심 자산" 관점은 OpenGoose의 세션 관리 전략 수립에 직접적인 영감을 준다.

---

## 4. 프로젝트 간 포지셔닝 맵

```
                    관찰/기록 ←─────────────────→ 조율/실행
                         │                           │
              ┌──────────┤                           ├──────────┐
              │          │                           │          │
          Entire     Beads                     Autoresearch  Gas Town
       (에이전트 세션   (에이전트 작업              (단일 에이전트  (다수 에이전트
        투명성 추적)    상태 영속화)                자율 실험)     자율 운영)
              │          │                           │          │
              └──────┬───┘                           └────┬─────┘
                     │                                    │
              "왜 이렇게 했나"                       "무엇을 해야 하나"
                     │                                    │
                     └──────────── OpenGoose ─────────────┘
                              (양쪽을 통합하려는 시도)
```

### 차원별 비교

| 차원 | Autoresearch | Entire | Gas Town | Goosetown | OpenGoose |
|------|-------------|--------|----------|-----------|-----------|
| **에이전트 수** | 1 | 관여 안함 | 20~30 | 3~5 | 다수 |
| **대상** | ML 연구 | 범용 | 범용 개발 | 리서치 | 범용 |
| **감독** | 없음 | 없음 (관찰) | 계층적 | 플랫 | TBD |
| **메모리** | Git 커밋 | Git 브랜치 | Beads+Dolt | gtwall | TBD |
| **머지** | 불필요 | 불필요 | 자동 | 수동 | TBD |
| **복잡도** | 630줄 | CLI 도구 | 300+ 파일 | 4,500줄 | 진행 중 |
| **비즈니스** | 오픈소스 | 상용 ($60M) | 오픈소스 | 오픈소스 | 오픈소스 |

---

## 5. 시사점 및 트렌드

### 5.1 수렴하는 패턴

세 프로젝트 모두 독립적으로 동일한 결론에 도달하고 있다:

1. **Git이 에이전트의 기반 인프라다** — Autoresearch는 Git 커밋으로 실험 이력을, Entire는 Git 브랜치로 세션 메타데이터를, Gas Town은 Git worktree로 에이전트 격리를 구현한다.

2. **마크다운이 에이전트의 프로그래밍 언어다** — Autoresearch의 `program.md`, Gas Town의 `CLAUDE.md`, Entire의 세션 트랜스크립트 모두 마크다운 기반이다.

3. **관찰 가능성이 핵심이다** — "에이전트가 무엇을 했는가"를 추적하는 능력이 점점 중요해지고 있다. Entire는 이를 전면에 내세운 첫 상용 플랫폼이다.

### 5.2 열린 질문

- **Autoresearch의 단일 에이전트 접근이 다중 에이전트로 확장될까?** 현재 설계는 의도적으로 단순하지만, 여러 GPU에서 병렬 실험을 돌리려는 수요는 있다.
- **Entire와 Gas Town은 경쟁인가 보완인가?** 현재는 다른 레이어를 다루지만, Entire의 "시맨틱 추론 레이어"가 오케스트레이션으로 확장되면 겹칠 수 있다.
- **"세션 로그 = 자산"이라면, 그 포맷은 표준화될까?** Entire, Gas Town, 박재홍 모두 같은 주장을 하지만, 저장 포맷은 각각 다르다.

---

## 6. 참고 자료

### Autoresearch
- [GitHub - karpathy/autoresearch](https://github.com/karpathy/autoresearch)
- [MarkTechPost - Karpathy Open-Sources Autoresearch](https://www.marktechpost.com/2026/03/08/andrej-karpathy-open-sources-autoresearch-a-630-line-python-tool-letting-ai-agents-run-autonomous-ml-experiments-on-single-gpus/)
- [Garry's List - Karpathy Turned One GPU Into a Research Lab](https://garryslist.org/posts/karpathy-just-turned-one-gpu-into-a-research-lab-f55754a6)
- [TopAIProduct - Autoresearch Overnight AI Researcher](https://topaiproduct.com/2026/03/07/autoresearch-karpathys-overnight-ai-researcher-that-runs-100-experiments-while-you-sleep/)

### Entire
- [GitHub - entireio/cli](https://github.com/entireio/cli)
- [GeekNews - Entire AI 에이전트 플랫폼](https://news.hada.io/topic?id=26583)
- [TechCrunch - Former GitHub CEO raises record $60M](https://techcrunch.com/2026/02/10/former-github-ceo-raises-record-60m-dev-tool-seed-round-at-300m-valuation/)
- [The New Stack - Thomas Dohmke Interview](https://thenewstack.io/thomas-dohmke-interview-entire/)
- [GeekWire - Former GitHub CEO launches new developer platform](https://www.geekwire.com/2026/former-github-ceo-launches-new-developer-platform-with-huge-60m-seed-round/)

### 박재홍의 실리콘밸리
- [WikiDocs 블로그](https://wikidocs.net/blog/@jaehong/)
- [세션 로그가 진짜 자산](https://wikidocs.net/blog/@jaehong/8086/)
- [Claude Cowork의 10GB VM 번들](https://wikidocs.net/blog/@jaehong/8599/)
- [GitHub Agentic Workflows](https://wikidocs.net/blog/@jaehong/7028/)
- [참고 글 (8970) — 접근 제한으로 미확인](https://wikidocs.net/blog/@jaehong/8970/)

### GeekNews 참고
- [GeekNews topic #27367 — 접근 제한으로 미확인](https://news.hada.io/topic?id=27367)
