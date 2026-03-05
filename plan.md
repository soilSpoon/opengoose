# OpenGoose 실시간 스트리밍 응답 설계안

## 현재 상태

현재 OpenGoose는 **요청-응답(RPC) 패턴**만 지원합니다:
- LLM이 전체 응답을 생성 완료한 후 → `send_message()`로 한 번에 전송
- 사용자는 응답이 완전히 생성될 때까지 아무것도 보지 못함
- 유일한 피드백은 Telegram의 typing indicator뿐

## 목표

1. LLM 토큰이 생성되는 동안 실시간으로 메시지를 업데이트 (Discord/Slack/Telegram 모두)
2. 기존 Gateway 트레이트와의 호환성 유지 (goose 크레이트 의존)
3. 깔끔한 모듈 분리와 확장성

## 핵심 설계: StreamResponder 패턴

### 아키텍처 개요

```
LLM Token Stream
       │
       ▼
┌─────────────────────┐
│   ResponseStream     │  (opengoose-types)
│   tokio::broadcast   │  토큰 단위 이벤트 발행
└──────────┬──────────┘
           │
           ▼
┌─────────────────────┐
│   StreamResponder    │  (opengoose-core, 새 모듈)
│                      │  쓰로틀링 + 메시지 편집 조율
│   ┌───────────────┐  │
│   │ ThrottlePolicy│  │  플랫폼별 업데이트 빈도 제어
│   └───────────────┘  │
└──────────┬──────────┘
           │
     ┌─────┼─────┐
     ▼     ▼     ▼
  Discord Slack  Telegram
  edit    update editMsg
  Message        Text
```

### 1단계: 스트리밍 이벤트 타입 (opengoose-types)

```rust
// crates/opengoose-types/src/streaming.rs

/// 스트리밍 응답의 개별 청크
#[derive(Debug, Clone)]
pub enum StreamChunk {
    /// 토큰 단위 텍스트 조각
    Delta(String),
    /// 스트리밍 완료
    Done,
    /// 에러 발생 시
    Error(String),
}

/// 스트리밍 세션을 식별하는 핸들
#[derive(Debug, Clone)]
pub struct StreamId(pub String);

/// EventBus에 추가할 새 이벤트 종류
// AppEventKind에 추가:
// StreamStarted { session_key, stream_id }
// StreamChunk { session_key, stream_id, delta }
// StreamCompleted { session_key, stream_id, full_text }
```

### 2단계: StreamResponder 트레이트 (opengoose-core)

```rust
// crates/opengoose-core/src/stream_responder.rs

use async_trait::async_trait;

/// 플랫폼별 스트리밍 응답 능력을 정의하는 트레이트.
/// Gateway 트레이트를 확장하지 않고, 별도 옵셔널 능력으로 분리.
#[async_trait]
pub trait StreamResponder: Send + Sync {
    /// 플랫폼이 메시지 편집 기반 스트리밍을 지원하는지
    fn supports_streaming(&self) -> bool;

    /// "생각 중..." 초기 메시지를 보내고, 편집 가능한 핸들을 반환
    async fn create_draft(&self, channel_id: &str) -> anyhow::Result<DraftHandle>;

    /// 기존 메시지를 새 내용으로 업데이트 (편집)
    async fn update_draft(&self, handle: &DraftHandle, content: &str) -> anyhow::Result<()>;

    /// 스트리밍 완료 — 최종 메시지로 확정
    async fn finalize_draft(&self, handle: &DraftHandle, content: &str) -> anyhow::Result<()>;
}

/// 플랫폼이 반환하는 편집 가능한 메시지 핸들
#[derive(Debug, Clone)]
pub struct DraftHandle {
    /// 플랫폼별 메시지 ID (Discord message ID, Slack ts, Telegram message_id)
    pub message_id: String,
    /// 메시지가 속한 채널/대화 ID
    pub channel_id: String,
}
```

### 3단계: 쓰로틀링 정책 (opengoose-core)

```rust
// crates/opengoose-core/src/throttle.rs

use std::time::{Duration, Instant};

/// 플랫폼별 API rate limit을 존중하는 쓰로틀러
pub struct ThrottlePolicy {
    /// 최소 업데이트 간격
    min_interval: Duration,
    /// 최소 텍스트 변화량 (바이트)
    min_delta_bytes: usize,
    /// 마지막 업데이트 시각
    last_update: Option<Instant>,
    /// 마지막 전송된 텍스트 길이
    last_sent_len: usize,
}

impl ThrottlePolicy {
    /// Discord: 5 edits/5s → 최소 1초 간격
    pub fn discord() -> Self {
        Self {
            min_interval: Duration::from_secs(1),
            min_delta_bytes: 50,
            last_update: None,
            last_sent_len: 0,
        }
    }

    /// Slack: chat.update Tier 3 → 최소 1.2초 간격
    pub fn slack() -> Self {
        Self {
            min_interval: Duration::from_millis(1200),
            min_delta_bytes: 80,
            last_update: None,
            last_sent_len: 0,
        }
    }

    /// Telegram: editMessageText 30/sec global → 최소 1초 간격
    pub fn telegram() -> Self {
        Self {
            min_interval: Duration::from_secs(1),
            min_delta_bytes: 50,
            last_update: None,
            last_sent_len: 0,
        }
    }

    /// 지금 업데이트를 보내도 되는지 판단
    pub fn should_update(&self, current_len: usize) -> bool {
        let enough_delta = current_len - self.last_sent_len >= self.min_delta_bytes;
        let enough_time = self.last_update
            .map(|t| t.elapsed() >= self.min_interval)
            .unwrap_or(true);
        enough_delta && enough_time
    }

    /// 업데이트 전송 후 상태 기록
    pub fn record_update(&mut self, sent_len: usize) {
        self.last_update = Some(Instant::now());
        self.last_sent_len = sent_len;
    }
}
```

### 4단계: 스트리밍 오케스트레이터 (opengoose-core)

```rust
// crates/opengoose-core/src/stream_orchestrator.rs

/// 토큰 스트림을 받아 플랫폼에 쓰로틀링된 업데이트를 보내는 핵심 루프.
/// 각 Gateway 어댑터에서 공통으로 사용.
pub async fn drive_stream(
    responder: &dyn StreamResponder,
    channel_id: &str,
    mut rx: broadcast::Receiver<StreamChunk>,
    mut throttle: ThrottlePolicy,
) -> anyhow::Result<String> {
    let handle = responder.create_draft(channel_id).await?;
    let mut buffer = String::new();

    loop {
        match rx.recv().await {
            Ok(StreamChunk::Delta(delta)) => {
                buffer.push_str(&delta);
                if throttle.should_update(buffer.len()) {
                    // 메시지 길이 제한 내에서만 (초과 시 잘라서 표시)
                    let display = truncate_for_display(&buffer);
                    let _ = responder.update_draft(&handle, &display).await;
                    throttle.record_update(buffer.len());
                }
            }
            Ok(StreamChunk::Done) => {
                responder.finalize_draft(&handle, &buffer).await?;
                break;
            }
            Ok(StreamChunk::Error(e)) => {
                let error_msg = format!("{buffer}\n\n⚠️ Error: {e}");
                responder.finalize_draft(&handle, &error_msg).await?;
                return Err(anyhow::anyhow!(e));
            }
            Err(_) => break, // channel closed
        }
    }

    Ok(buffer)
}
```

### 5단계: 플랫폼별 StreamResponder 구현

#### Discord (opengoose-discord)

```rust
#[async_trait]
impl StreamResponder for DiscordGateway {
    fn supports_streaming(&self) -> bool { true }

    async fn create_draft(&self, channel_id: &str) -> Result<DraftHandle> {
        let ch_id = Id::<ChannelMarker>::new(channel_id.parse()?);
        let msg = self.http.create_message(ch_id)
            .content("⏳ Thinking...")
            .await?
            .model().await?;
        Ok(DraftHandle {
            message_id: msg.id.to_string(),
            channel_id: channel_id.to_string(),
        })
    }

    async fn update_draft(&self, handle: &DraftHandle, content: &str) -> Result<()> {
        let ch_id = Id::<ChannelMarker>::new(handle.channel_id.parse()?);
        let msg_id = Id::new(handle.message_id.parse()?);
        self.http.update_message(ch_id, msg_id)
            .content(Some(&truncate(content, 2000)))
            .await?;
        Ok(())
    }

    async fn finalize_draft(&self, handle: &DraftHandle, content: &str) -> Result<()> {
        // 최종 메시지가 2000자 초과 시: 원본 메시지 편집 + 나머지는 새 메시지로
        let chunks = split_message(content, DISCORD_MAX_LEN);
        // 첫 번째 청크로 기존 메시지 편집
        self.update_draft(handle, chunks[0]).await?;
        // 나머지는 새 메시지로 전송
        for chunk in &chunks[1..] {
            let ch_id = Id::<ChannelMarker>::new(handle.channel_id.parse()?);
            self.http.create_message(ch_id).content(chunk).await?;
        }
        Ok(())
    }
}
```

#### Slack (opengoose-slack)

```rust
#[async_trait]
impl StreamResponder for SlackGateway {
    fn supports_streaming(&self) -> bool { true }

    async fn create_draft(&self, channel: &str) -> Result<DraftHandle> {
        let resp: PostMessageResponse = self.client
            .post("https://slack.com/api/chat.postMessage")
            .bearer_auth(&self.bot_token)
            .json(&json!({ "channel": channel, "text": "⏳ Thinking..." }))
            .send().await?.json().await?;
        Ok(DraftHandle {
            message_id: resp.ts.unwrap_or_default(),  // Slack의 메시지 타임스탬프
            channel_id: channel.to_string(),
        })
    }

    async fn update_draft(&self, handle: &DraftHandle, content: &str) -> Result<()> {
        self.client
            .post("https://slack.com/api/chat.update")
            .bearer_auth(&self.bot_token)
            .json(&json!({
                "channel": handle.channel_id,
                "ts": handle.message_id,
                "text": &truncate(content, 4000),
            }))
            .send().await?;
        Ok(())
    }

    async fn finalize_draft(&self, handle: &DraftHandle, content: &str) -> Result<()> {
        let chunks = split_message(content, SLACK_MAX_LEN);
        self.update_draft(handle, chunks[0]).await?;
        for chunk in &chunks[1..] {
            self.post_message(&handle.channel_id, chunk).await?;
        }
        Ok(())
    }
}
```

#### Telegram (opengoose-telegram)

```rust
#[async_trait]
impl StreamResponder for TelegramGateway {
    fn supports_streaming(&self) -> bool { true }

    async fn create_draft(&self, chat_id: &str) -> Result<DraftHandle> {
        let resp: TelegramResponse<TelegramSentMessage> = self.client
            .post(self.api_url("sendMessage"))
            .json(&json!({ "chat_id": chat_id, "text": "⏳ Thinking..." }))
            .send().await?.json().await?;
        let msg = resp.result.ok_or_else(|| anyhow!("no message in response"))?;
        Ok(DraftHandle {
            message_id: msg.message_id.to_string(),
            channel_id: chat_id.to_string(),
        })
    }

    async fn update_draft(&self, handle: &DraftHandle, content: &str) -> Result<()> {
        self.client
            .post(self.api_url("editMessageText"))
            .json(&json!({
                "chat_id": handle.channel_id,
                "message_id": handle.message_id.parse::<i64>()?,
                "text": &truncate(content, 4096),
            }))
            .send().await?;
        Ok(())
    }

    async fn finalize_draft(&self, handle: &DraftHandle, content: &str) -> Result<()> {
        // Telegram은 메시지 편집만 하고, 4096자 초과 시 나머지를 새 메시지로
        let (first, rest) = if content.len() > 4096 {
            (&content[..4096], Some(&content[4096..]))
        } else {
            (content, None)
        };
        self.update_draft(handle, first).await?;
        if let Some(rest) = rest {
            self.client.post(self.api_url("sendMessage"))
                .json(&json!({ "chat_id": handle.channel_id, "text": rest }))
                .send().await?;
        }
        Ok(())
    }
}
```

### 6단계: Bridge/Engine 통합

```rust
// GatewayBridge에 추가되는 메서드:

/// 스트리밍 모드로 메시지를 릴레이.
/// team orchestration 결과를 스트림으로 반환.
pub async fn relay_message_streaming(
    &self,
    session_key: &SessionKey,
    display_name: Option<String>,
    text: &str,
) -> anyhow::Result<Option<broadcast::Receiver<StreamChunk>>> {
    // Team orchestration이 활성화된 경우 스트리밍 모드 사용
    match self.engine.process_message_streaming(session_key, display_name.as_deref(), text).await? {
        Some(rx) => Ok(Some(rx)),
        None => {
            // Goose 단일 에이전트 — 기존 콜백 방식 유지
            // (goose의 Gateway trait은 수정 불가)
            let guard = self.handler.read().await;
            let handler = guard.as_ref().ok_or(GatewayError::HandlerNotReady)?;
            handler.handle_message(/* ... */).await?;
            Ok(None)
        }
    }
}
```

### 7단계: 공통 유틸리티 추출 (opengoose-core)

```rust
// crates/opengoose-core/src/message_utils.rs

/// UTF-8 안전 메시지 분할 — 모든 어댑터에서 공유
pub fn split_message(text: &str, max_len: usize) -> Vec<&str> { /* ... */ }

/// 디스플레이용 잘라내기 (스트리밍 중간 업데이트에 사용)
pub fn truncate_for_display(text: &str, max_len: usize) -> &str { /* ... */ }
```

## 파일 변경 목록

### 새 파일
| 파일 | 크레이트 | 설명 |
|------|---------|------|
| `src/streaming.rs` | opengoose-types | StreamChunk, StreamId 타입 |
| `src/stream_responder.rs` | opengoose-core | StreamResponder 트레이트 + DraftHandle |
| `src/throttle.rs` | opengoose-core | ThrottlePolicy |
| `src/stream_orchestrator.rs` | opengoose-core | drive_stream() 공용 루프 |
| `src/message_utils.rs` | opengoose-core | split_message, truncate 공용 함수 |

### 수정 파일
| 파일 | 변경 내용 |
|------|----------|
| `opengoose-types/src/lib.rs` | `pub mod streaming;` 추가 |
| `opengoose-types/src/events.rs` | StreamStarted/StreamChunk/StreamCompleted 이벤트 추가 |
| `opengoose-core/src/lib.rs` | 새 모듈 등록 |
| `opengoose-core/src/bridge.rs` | `relay_message_streaming()` 추가 |
| `opengoose-core/src/engine.rs` | `process_message_streaming()` 추가 |
| `opengoose-discord/src/gateway.rs` | StreamResponder impl + handle_message 스트리밍 분기 |
| `opengoose-slack/src/gateway.rs` | StreamResponder impl + handle_envelope 스트리밍 분기 |
| `opengoose-telegram/src/gateway.rs` | StreamResponder impl + 폴링 루프 스트리밍 분기 |

## 구현 순서 (단계별)

### Phase 1: 기반 타입 + 공용 유틸리티
1. `opengoose-types/src/streaming.rs` — StreamChunk, StreamId
2. `opengoose-core/src/message_utils.rs` — split_message 추출 (3개 어댑터 중복 제거)
3. `opengoose-core/src/throttle.rs` — ThrottlePolicy
4. `opengoose-core/src/stream_responder.rs` — StreamResponder 트레이트 + DraftHandle

### Phase 2: 스트리밍 오케스트레이터
5. `opengoose-core/src/stream_orchestrator.rs` — drive_stream() 구현
6. `opengoose-types/src/events.rs` — 스트리밍 이벤트 추가

### Phase 3: 플랫폼 어댑터 구현
7. Discord StreamResponder 구현
8. Slack StreamResponder 구현
9. Telegram StreamResponder 구현

### Phase 4: Bridge/Engine 통합
10. Engine에 process_message_streaming() 추가
11. GatewayBridge에 relay_message_streaming() 추가
12. 각 어댑터의 메시지 핸들러에서 스트리밍 분기 로직 추가

## 설계 결정 근거

1. **Gateway 트레이트를 수정하지 않음** — goose 외부 크레이트 의존성이므로 별도 `StreamResponder` 트레이트로 분리
2. **broadcast 채널 사용** — 다중 구독자(EventBus, TUI 등)가 동시에 스트림을 관찰 가능
3. **쓰로틀링을 코어에서** — 각 어댑터가 아닌 공용 레이어에서 rate limit 관리
4. **DraftHandle 패턴** — 플랫폼별 메시지 ID 형식을 추상화 (Discord u64, Slack ts string, Telegram i64)
5. **Graceful degradation** — Goose 단일 에이전트 경로는 기존 콜백 방식 유지 (스트리밍 불가시 폴백)
