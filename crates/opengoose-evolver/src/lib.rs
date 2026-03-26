// Evolver — stamp_notify listener with lazy Agent init.
// Queries unprocessed low stamps, creates work items, analyzes with LLM.

mod loop_driver;
mod pipeline;
mod sweep;

use async_trait::async_trait;
use futures::StreamExt;
use goose::agents::{Agent, AgentEvent, SessionConfig};
use goose::conversation::message::Message;
use opengoose_rig::work_mode::evolve_session_id;

pub use loop_driver::run;

pub(crate) const EVOLVER_SYSTEM_PROMPT: &str = "You are a skill analyst for OpenGoose.\n\
     Analyze failed tasks and extract concrete, actionable lessons as SKILL.md files.\n\n\
     Rules:\n\
     - description MUST start with 'Use when...' (triggering conditions only)\n\
     - description must NOT summarize the skill's workflow\n\
     - Every lesson must be specific to THIS failure, not generic advice\n\
     - Include a 'Common Mistakes' table with specific rationalizations\n\
     - Include a 'Red Flags' list for self-checking\n\
     - If the lesson is something any competent agent already knows, output SKIP\n\
     - If an existing skill covers the same lesson, output UPDATE:{skill-name}\n\n\
     Output format: raw SKILL.md content with YAML frontmatter, OR 'SKIP', OR 'UPDATE:{name}'.";

pub(crate) const LOW_STAMP_THRESHOLD: f32 = 0.3;
const FALLBACK_SWEEP_SECS: u64 = 300; // 5 minutes

#[async_trait]
pub(crate) trait AgentCaller: Send + Sync {
    async fn call(&self, prompt: &str, work_id: i64) -> anyhow::Result<String>;
}

struct RealAgentCaller<'a> {
    agent: &'a Agent,
}

#[async_trait]
impl AgentCaller for RealAgentCaller<'_> {
    async fn call(&self, prompt: &str, work_id: i64) -> anyhow::Result<String> {
        let message = Message::user().with_text(prompt);
        let session_config = SessionConfig {
            id: evolve_session_id(work_id),
            schedule_id: None,
            max_turns: None,
            retry_config: None,
        };

        let stream = self.agent.reply(message, session_config, None).await?;
        tokio::pin!(stream);

        let mut response_text = String::new();
        while let Some(event) = stream.next().await {
            match event {
                Ok(AgentEvent::Message(msg)) => {
                    use goose::conversation::message::MessageContent;
                    for content in &msg.content {
                        if let MessageContent::Text(t) = content {
                            response_text.push_str(&t.text);
                        }
                    }
                }
                Err(e) => return Err(e),
                _ => {}
            }
        }

        Ok(response_text)
    }
}

pub(crate) use opengoose_rig::home_dir;

// ---------------------------------------------------------------------------
// read_conversation_log — moved from binary crate's skills::evolve
// ---------------------------------------------------------------------------

pub(crate) fn read_conversation_log(work_item_id: i64) -> String {
    let session_id = format!("task-{work_item_id}");
    match opengoose_rig::conversation_log::read_log(&session_id) {
        Some(content) => opengoose_skills::evolution::prompts::summarize_for_prompt(&content, 4000),
        None => {
            tracing::warn!(work_item_id, %session_id, "evolver: conversation log not found or unreadable");
            String::new()
        }
    }
}

// ---------------------------------------------------------------------------
// test_env_lock — local test lock for environment variable isolation
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(crate) fn test_env_lock() -> &'static std::sync::Mutex<()> {
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    &ENV_LOCK
}
