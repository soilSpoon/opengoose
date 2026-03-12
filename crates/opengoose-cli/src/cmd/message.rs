use std::sync::Arc;
use std::time::Duration;

use crate::error::{CliError, CliResult};
use clap::Subcommand;
use tokio::sync::broadcast::error::RecvError;

use opengoose_persistence::{AgentMessageStore, Database};

#[derive(Subcommand)]
/// Subcommands for `opengoose message`.
pub enum MessageAction {
    /// Send a directed message from one agent to another
    Send {
        /// Sender agent name
        #[arg(long)]
        from: String,
        /// Recipient agent name (for directed messages)
        #[arg(long)]
        to: Option<String>,
        /// Channel name (for pub/sub messages)
        #[arg(long)]
        channel: Option<String>,
        /// Message payload
        payload: String,
        /// Session key (e.g. discord:guild:channel)
        #[arg(long, default_value = "cli:local:default")]
        session: String,
    },
    /// List recent messages for a session
    List {
        /// Session key
        #[arg(long, default_value = "cli:local:default")]
        session: String,
        /// Maximum number of messages to show
        #[arg(long, short, default_value_t = 20)]
        limit: i64,
        /// Filter by agent name (shows messages to/from this agent)
        #[arg(long)]
        agent: Option<String>,
        /// Filter by channel name
        #[arg(long)]
        channel: Option<String>,
    },
    /// Subscribe to real-time messages on a channel or for an agent (Ctrl-C to exit)
    Subscribe {
        /// Channel name to subscribe to
        #[arg(long)]
        channel: Option<String>,
        /// Agent name to receive directed messages for
        #[arg(long)]
        agent: Option<String>,
        /// Timeout in seconds (0 = indefinite)
        #[arg(long, default_value_t = 0)]
        timeout: u64,
    },
    /// Show pending (undelivered) directed messages for an agent
    Pending {
        /// Agent name to check pending messages for
        agent: String,
        /// Session key
        #[arg(long, default_value = "cli:local:default")]
        session: String,
    },
}

/// Dispatch and execute the selected message subcommand.
pub async fn execute(action: MessageAction) -> CliResult<()> {
    match action {
        MessageAction::Send {
            from,
            to,
            channel,
            payload,
            session,
        } => cmd_send(&session, &from, to.as_deref(), channel.as_deref(), &payload),
        MessageAction::List {
            session,
            limit,
            agent,
            channel,
        } => cmd_list(&session, limit, agent.as_deref(), channel.as_deref()),
        MessageAction::Subscribe {
            channel,
            agent,
            timeout,
        } => cmd_subscribe(channel.as_deref(), agent.as_deref(), timeout).await,
        MessageAction::Pending { agent, session } => cmd_pending(&session, &agent),
    }
}

fn open_db() -> CliResult<Arc<Database>> {
    Ok(Arc::new(Database::open()?))
}

fn cmd_send(
    session: &str,
    from: &str,
    to: Option<&str>,
    channel: Option<&str>,
    payload: &str,
) -> CliResult<()> {
    match (to, channel) {
        (Some(_), Some(_)) => {
            return Err(CliError::Validation(
                "specify either --to or --channel, not both".into(),
            ));
        }
        (None, None) => {
            return Err(CliError::Validation(
                "specify either --to <agent> or --channel <name>".into(),
            ));
        }
        (Some(to_agent), None) => {
            let store = AgentMessageStore::new(open_db()?);
            let id = store.send_directed(session, from, to_agent, payload)?;
            println!("Directed message sent (id={id})");
            println!("  From:    {from}");
            println!("  To:      {to_agent}");
            println!("  Payload: {payload}");
            Ok(())
        }
        (None, Some(ch)) => {
            let store = AgentMessageStore::new(open_db()?);
            let id = store.publish(session, from, ch, payload)?;
            println!("Channel message published (id={id})");
            println!("  From:    {from}");
            println!("  Channel: {ch}");
            println!("  Payload: {payload}");
            Ok(())
        }
    }
}

fn cmd_list(session: &str, limit: i64, agent: Option<&str>, channel: Option<&str>) -> CliResult<()> {
    let store = AgentMessageStore::new(open_db()?);

    let mut messages = if let Some(agent_name) = agent {
        store.list_for_agent(session, agent_name, limit)?
    } else if let Some(ch) = channel {
        store.channel_history(session, ch, None)?
    } else {
        let mut msgs = store.list_recent(session, limit)?;
        msgs.reverse(); // list_recent returns newest-first; display oldest-first
        msgs
    };

    if messages.is_empty() {
        println!("No messages found.");
        return Ok(());
    }

    // For list_for_agent, also reverse to oldest-first
    if agent.is_some() {
        messages.reverse();
    }

    println!(
        "{:<6} {:<20} {:<20} {:<12} {:<12} PAYLOAD",
        "ID", "FROM", "TO/CHANNEL", "TYPE", "STATUS"
    );
    println!("{}", "-".repeat(90));

    for msg in &messages {
        let dest = msg
            .to_agent
            .as_deref()
            .or(msg.channel.as_deref())
            .unwrap_or("-");
        let kind = if msg.is_directed() {
            "directed"
        } else {
            "channel"
        };
        let preview = if msg.payload.len() > 40 {
            format!("{}…", &msg.payload[..39])
        } else {
            msg.payload.clone()
        };
        println!(
            "{:<6} {:<20} {:<20} {:<12} {:<12} {}",
            msg.id,
            &msg.from_agent[..msg.from_agent.len().min(20)],
            &dest[..dest.len().min(20)],
            kind,
            msg.status.as_str(),
            preview
        );
    }

    println!("\n{} message(s).", messages.len());
    Ok(())
}

async fn cmd_subscribe(
    channel: Option<&str>,
    agent: Option<&str>,
    timeout_secs: u64,
) -> CliResult<()> {
    use opengoose_teams::MessageBus;

    match (channel, agent) {
        (None, None) => {
            return Err(CliError::Validation(
                "specify either --channel <name> or --agent <name>".into(),
            ));
        }
        (Some(_), Some(_)) => {
            return Err(CliError::Validation(
                "specify either --channel or --agent, not both".into(),
            ));
        }
        _ => {}
    }

    // The subscribe command creates a fresh in-process bus and listens.
    // In production this would connect to a shared bus (e.g. via a socket or
    // shared memory) — for now it demonstrates the API and is useful for
    // testing publish/subscribe in the same process.
    let bus = MessageBus::new(128);

    if let Some(ch) = channel {
        println!("Subscribed to channel '{ch}' (Ctrl-C to exit)…");
        let mut rx = bus.subscribe_channel(ch);
        recv_loop(&mut rx, timeout_secs, |e| {
            println!("[{}] {} → #{}: {}", e.timestamp, e.from, ch, e.payload);
        })
        .await;
    } else if let Some(agent_name) = agent {
        println!("Subscribed to directed messages for '{agent_name}' (Ctrl-C to exit)…");
        let mut rx = bus.subscribe_agent(agent_name);
        recv_loop(&mut rx, timeout_secs, |e| {
            println!(
                "[{}] {} → {}: {}",
                e.timestamp, e.from, agent_name, e.payload
            );
        })
        .await;
    }

    Ok(())
}

async fn recv_loop<F>(
    rx: &mut tokio::sync::broadcast::Receiver<opengoose_teams::message_bus::BusEvent>,
    timeout_secs: u64,
    print: F,
) where
    F: Fn(&opengoose_teams::message_bus::BusEvent),
{
    let deadline = if timeout_secs > 0 {
        Some(tokio::time::Instant::now() + Duration::from_secs(timeout_secs))
    } else {
        None
    };

    loop {
        let recv_fut = rx.recv();
        let event = if let Some(dl) = deadline {
            match tokio::time::timeout_at(dl, recv_fut).await {
                Ok(r) => r,
                Err(_) => {
                    println!("Subscription timeout.");
                    break;
                }
            }
        } else {
            recv_fut.await
        };

        match event {
            Ok(e) => print(&e),
            Err(RecvError::Lagged(n)) => eprintln!("Warning: lagged {n} messages"),
            Err(RecvError::Closed) => {
                println!("Subscription closed.");
                break;
            }
        }
    }
}

fn cmd_pending(session: &str, agent: &str) -> CliResult<()> {
    let store = AgentMessageStore::new(open_db()?);
    let pending = store.receive_pending(session, agent)?;

    if pending.is_empty() {
        println!("No pending messages for '{agent}'.");
        return Ok(());
    }

    println!("Pending messages for '{agent}':");
    println!("{:<6} {:<20} PAYLOAD", "ID", "FROM");
    println!("{}", "-".repeat(60));
    for msg in &pending {
        let preview = if msg.payload.len() > 40 {
            format!("{}…", &msg.payload[..39])
        } else {
            msg.payload.clone()
        };
        println!(
            "{:<6} {:<20} {}",
            msg.id,
            &msg.from_agent[..msg.from_agent.len().min(20)],
            preview
        );
    }
    println!("\n{} pending message(s).", pending.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use opengoose_persistence::Database;
    use opengoose_teams::message_bus::BusEvent;

    fn make_store() -> opengoose_persistence::AgentMessageStore {
        let db = Arc::new(Database::open_in_memory().unwrap());
        opengoose_persistence::AgentMessageStore::new(db)
    }

    // ── recv_loop tests ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn recv_loop_exits_when_channel_closed() {
        let (tx, mut rx) = tokio::sync::broadcast::channel::<BusEvent>(8);
        drop(tx); // sender gone → Closed variant

        let call_count = Arc::new(Mutex::new(0usize));
        let cc = call_count.clone();
        recv_loop(&mut rx, 0, move |_| *cc.lock().unwrap() += 1).await;

        assert_eq!(*call_count.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn recv_loop_processes_single_event() {
        let (tx, mut rx) = tokio::sync::broadcast::channel::<BusEvent>(8);
        tx.send(BusEvent::directed("a", "b", "hello")).unwrap();
        drop(tx);

        let received: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let r = received.clone();
        recv_loop(&mut rx, 0, move |e| {
            r.lock().unwrap().push(e.payload.clone())
        })
        .await;

        let r = received.lock().unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0], "hello");
    }

    #[tokio::test]
    async fn recv_loop_processes_multiple_events() {
        let (tx, mut rx) = tokio::sync::broadcast::channel::<BusEvent>(8);
        tx.send(BusEvent::directed("a", "b", "msg1")).unwrap();
        tx.send(BusEvent::directed("a", "b", "msg2")).unwrap();
        tx.send(BusEvent::directed("a", "b", "msg3")).unwrap();
        drop(tx);

        let received: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let r = received.clone();
        recv_loop(&mut rx, 0, move |e| {
            r.lock().unwrap().push(e.payload.clone())
        })
        .await;

        let r = received.lock().unwrap();
        assert_eq!(r.len(), 3);
        assert_eq!(r[0], "msg1");
        assert_eq!(r[2], "msg3");
    }

    #[tokio::test]
    async fn recv_loop_timeout_exits_without_blocking() {
        let (_tx, mut rx) = tokio::sync::broadcast::channel::<BusEvent>(8);
        // timeout = 1 s; no messages will arrive — should exit after the deadline
        recv_loop(&mut rx, 1, |_| {}).await;
        // If we reach here the test passed (recv_loop did not block forever)
    }

    // ── AgentMessageStore message-flow tests (in-memory DB) ─────────────────

    #[test]
    fn store_directed_message_appears_as_pending() {
        let store = make_store();
        let id = store.send_directed("sess", "alice", "bob", "ping").unwrap();
        assert!(id > 0);

        let pending = store.receive_pending("sess", "bob").unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].from_agent, "alice");
        assert_eq!(pending[0].payload, "ping");
    }

    #[test]
    fn store_channel_message_appears_in_history() {
        let store = make_store();
        let id = store
            .publish("sess", "alice", "general", "announcement")
            .unwrap();
        assert!(id > 0);

        let history = store.channel_history("sess", "general", None).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].from_agent, "alice");
        assert_eq!(history[0].payload, "announcement");
    }

    #[test]
    fn store_list_recent_returns_messages_newest_first() {
        let store = make_store();
        store.publish("sess", "a", "ch", "first").unwrap();
        store.publish("sess", "a", "ch", "second").unwrap();

        let recent = store.list_recent("sess", 10).unwrap();
        assert_eq!(recent.len(), 2);
        // list_recent returns newest first
        assert_eq!(recent[0].payload, "second");
        assert_eq!(recent[1].payload, "first");
    }

    #[test]
    fn store_list_for_agent_filters_by_agent() {
        let store = make_store();
        store
            .send_directed("sess", "alice", "bob", "for-bob")
            .unwrap();
        store
            .send_directed("sess", "alice", "carol", "for-carol")
            .unwrap();

        let bob_msgs = store.list_for_agent("sess", "bob", 10).unwrap();
        assert_eq!(bob_msgs.len(), 1);
        assert_eq!(bob_msgs[0].payload, "for-bob");

        let carol_msgs = store.list_for_agent("sess", "carol", 10).unwrap();
        assert_eq!(carol_msgs.len(), 1);
        assert_eq!(carol_msgs[0].payload, "for-carol");
    }

    #[test]
    fn store_pending_empty_when_no_messages() {
        let store = make_store();
        let pending = store.receive_pending("sess", "nobody").unwrap();
        assert!(pending.is_empty());
    }

    #[test]
    fn store_session_isolation() {
        let store = make_store();
        store.send_directed("sess-a", "x", "y", "in-a").unwrap();
        store.send_directed("sess-b", "x", "y", "in-b").unwrap();

        let a_pending = store.receive_pending("sess-a", "y").unwrap();
        let b_pending = store.receive_pending("sess-b", "y").unwrap();
        assert_eq!(a_pending.len(), 1);
        assert_eq!(a_pending[0].payload, "in-a");
        assert_eq!(b_pending.len(), 1);
        assert_eq!(b_pending[0].payload, "in-b");
    }

    // ── Existing validation tests ────────────────────────────────────────────

    #[tokio::test]
    async fn send_rejects_both_to_and_channel() {
        let err = execute(MessageAction::Send {
            from: "agent-a".into(),
            to: Some("agent-b".into()),
            channel: Some("general".into()),
            payload: "hello".into(),
            session: "cli:local:default".into(),
        })
        .await
        .unwrap_err();

        assert!(
            err.to_string().contains("not both"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn send_rejects_neither_to_nor_channel() {
        let err = execute(MessageAction::Send {
            from: "agent-a".into(),
            to: None,
            channel: None,
            payload: "hello".into(),
            session: "cli:local:default".into(),
        })
        .await
        .unwrap_err();

        let msg = err.to_string();
        assert!(
            msg.contains("--to") || msg.contains("--channel"),
            "unexpected error: {msg}"
        );
    }

    #[tokio::test]
    async fn subscribe_rejects_neither_channel_nor_agent() {
        let err = execute(MessageAction::Subscribe {
            channel: None,
            agent: None,
            timeout: 0,
        })
        .await
        .unwrap_err();

        let msg = err.to_string();
        assert!(
            msg.contains("--channel") || msg.contains("--agent"),
            "unexpected error: {msg}"
        );
    }

    #[tokio::test]
    async fn subscribe_rejects_both_channel_and_agent() {
        let err = execute(MessageAction::Subscribe {
            channel: Some("general".into()),
            agent: Some("bot".into()),
            timeout: 0,
        })
        .await
        .unwrap_err();

        assert!(
            err.to_string().contains("not both"),
            "unexpected error: {err}"
        );
    }
}
