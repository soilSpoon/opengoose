//! Witness module: detects stuck and zombie agents during team execution.
//!
//! The Witness subscribes to the [`EventBus`] via `subscribe_reliable()` and
//! maintains a map of agent states. It periodically checks for agents that
//! have exceeded configurable timeout thresholds, emitting `AgentStuck` and
//! `AgentZombie` events.

use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use opengoose_types::{AppEvent, AppEventKind, EventBus};

/// Agent lifecycle state as observed by the Witness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentState {
    Idle,
    Working,
    Stuck,
    Zombie,
}

/// Tracked status for a single agent.
#[derive(Debug, Clone)]
pub struct AgentStatus {
    pub agent_name: String,
    pub team_name: String,
    pub state: AgentState,
    pub last_event_at: Instant,
    pub started_at: Instant,
}

/// Configuration for the Witness module.
#[derive(Debug, Clone)]
pub struct WitnessConfig {
    /// Duration after which a working agent is considered stuck (default 300s).
    pub stuck_timeout: Duration,
    /// Duration after which a working agent is considered a zombie (default 600s).
    pub zombie_timeout: Duration,
    /// How often to check for stuck/zombie agents (default 5s).
    pub check_interval: Duration,
}

impl Default for WitnessConfig {
    fn default() -> Self {
        Self {
            stuck_timeout: Duration::from_secs(300),
            zombie_timeout: Duration::from_secs(600),
            check_interval: Duration::from_secs(5),
        }
    }
}

/// Handle returned by [`spawn_witness`]. Provides read access to agent states
/// and a way to stop the witness task.
pub struct WitnessHandle {
    /// Live view of all tracked agents.
    pub agents: Arc<DashMap<String, AgentStatus>>,
    cancel: CancellationToken,
}

impl WitnessHandle {
    /// Stop the witness background task.
    pub fn stop(&self) {
        self.cancel.cancel();
    }

    /// Get a snapshot of all agents in a given state.
    pub fn agents_in_state(&self, state: &AgentState) -> Vec<AgentStatus> {
        self.agents
            .iter()
            .filter(|entry| &entry.value().state == state)
            .map(|entry| entry.value().clone())
            .collect()
    }
}

/// Spawn the witness background task.
///
/// Returns a [`WitnessHandle`] for querying agent states and stopping the task.
pub fn spawn_witness(event_bus: &EventBus, config: WitnessConfig) -> WitnessHandle {
    let agents: Arc<DashMap<String, AgentStatus>> = Arc::new(DashMap::new());
    let cancel = CancellationToken::new();
    let rx = event_bus.subscribe_reliable();

    let handle = WitnessHandle {
        agents: agents.clone(),
        cancel: cancel.clone(),
    };

    let event_bus = event_bus.clone();
    tokio::spawn(witness_loop(agents, rx, event_bus, config, cancel));

    handle
}

async fn witness_loop(
    agents: Arc<DashMap<String, AgentStatus>>,
    mut rx: mpsc::UnboundedReceiver<AppEvent>,
    event_bus: EventBus,
    config: WitnessConfig,
    cancel: CancellationToken,
) {
    let mut check_interval = tokio::time::interval(config.check_interval);
    check_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                debug!("witness: cancelled, shutting down");
                break;
            }
            Some(event) = rx.recv() => {
                process_event(&agents, &event.kind);
            }
            _ = check_interval.tick() => {
                check_timeouts(&agents, &config, &event_bus);
            }
        }
    }
}

fn agent_key(team: &str, agent: &str) -> String {
    format!("{team}::{agent}")
}

fn process_event(agents: &DashMap<String, AgentStatus>, kind: &AppEventKind) {
    match kind {
        AppEventKind::TeamStepStarted { team, agent, .. } => {
            let key = agent_key(team, agent);
            let now = Instant::now();
            agents.insert(
                key,
                AgentStatus {
                    agent_name: agent.clone(),
                    team_name: team.clone(),
                    state: AgentState::Working,
                    last_event_at: now,
                    started_at: now,
                },
            );
        }
        AppEventKind::TeamStepCompleted { team, agent } | AppEventKind::TeamStepFailed { team, agent, .. } => {
            let key = agent_key(team, agent);
            if let Some(mut entry) = agents.get_mut(&key) {
                entry.state = AgentState::Idle;
                entry.last_event_at = Instant::now();
            }
        }
        // Liveness signals: update last_event_at for the agent's session
        AppEventKind::ModelChanged { .. }
        | AppEventKind::ContextCompacted { .. }
        | AppEventKind::ExtensionNotification { .. } => {
            // These events don't carry team/agent info directly, so we update
            // all currently working agents' last_event_at as a liveness proof.
            let now = Instant::now();
            for mut entry in agents.iter_mut() {
                if entry.state == AgentState::Working {
                    entry.last_event_at = now;
                }
            }
        }
        _ => {}
    }
}

fn check_timeouts(
    agents: &DashMap<String, AgentStatus>,
    config: &WitnessConfig,
    event_bus: &EventBus,
) {
    let now = Instant::now();
    for mut entry in agents.iter_mut() {
        if entry.state != AgentState::Working && entry.state != AgentState::Stuck {
            continue;
        }
        let elapsed = now.duration_since(entry.last_event_at);

        if elapsed >= config.zombie_timeout && entry.state != AgentState::Zombie {
            warn!(
                agent = %entry.agent_name,
                team = %entry.team_name,
                elapsed_secs = elapsed.as_secs(),
                "agent detected as zombie"
            );
            entry.state = AgentState::Zombie;
            event_bus.emit(AppEventKind::AgentZombie {
                team: entry.team_name.clone(),
                agent: entry.agent_name.clone(),
            });
        } else if elapsed >= config.stuck_timeout && entry.state == AgentState::Working {
            warn!(
                agent = %entry.agent_name,
                team = %entry.team_name,
                elapsed_secs = elapsed.as_secs(),
                "agent detected as stuck"
            );
            entry.state = AgentState::Stuck;
            event_bus.emit(AppEventKind::AgentStuck {
                team: entry.team_name.clone(),
                agent: entry.agent_name.clone(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_agent_starts_working_on_step_started() {
        let bus = EventBus::new(16);
        let config = WitnessConfig {
            stuck_timeout: Duration::from_secs(300),
            zombie_timeout: Duration::from_secs(600),
            check_interval: Duration::from_secs(60),
        };
        let handle = spawn_witness(&bus, config);

        bus.emit(AppEventKind::TeamStepStarted {
            team: "test-team".into(),
            agent: "coder".into(),
            step: 0,
        });

        // Give the event loop time to process
        tokio::time::sleep(Duration::from_millis(50)).await;

        let key = agent_key("test-team", "coder");
        let status = handle.agents.get(&key).unwrap();
        assert_eq!(status.state, AgentState::Working);
        assert_eq!(status.agent_name, "coder");

        handle.stop();
    }

    #[tokio::test]
    async fn test_agent_becomes_idle_on_step_completed() {
        let bus = EventBus::new(16);
        let config = WitnessConfig {
            stuck_timeout: Duration::from_secs(300),
            zombie_timeout: Duration::from_secs(600),
            check_interval: Duration::from_secs(60),
        };
        let handle = spawn_witness(&bus, config);

        bus.emit(AppEventKind::TeamStepStarted {
            team: "t".into(),
            agent: "a".into(),
            step: 0,
        });
        tokio::time::sleep(Duration::from_millis(50)).await;

        bus.emit(AppEventKind::TeamStepCompleted {
            team: "t".into(),
            agent: "a".into(),
        });
        tokio::time::sleep(Duration::from_millis(50)).await;

        let key = agent_key("t", "a");
        let status = handle.agents.get(&key).unwrap();
        assert_eq!(status.state, AgentState::Idle);

        handle.stop();
    }

    #[tokio::test]
    async fn test_agent_becomes_idle_on_step_failed() {
        let bus = EventBus::new(16);
        let config = WitnessConfig {
            stuck_timeout: Duration::from_secs(300),
            zombie_timeout: Duration::from_secs(600),
            check_interval: Duration::from_secs(60),
        };
        let handle = spawn_witness(&bus, config);

        bus.emit(AppEventKind::TeamStepStarted {
            team: "t".into(),
            agent: "a".into(),
            step: 0,
        });
        tokio::time::sleep(Duration::from_millis(50)).await;

        bus.emit(AppEventKind::TeamStepFailed {
            team: "t".into(),
            agent: "a".into(),
            reason: "timeout".into(),
        });
        tokio::time::sleep(Duration::from_millis(50)).await;

        let key = agent_key("t", "a");
        let status = handle.agents.get(&key).unwrap();
        assert_eq!(status.state, AgentState::Idle);

        handle.stop();
    }

    #[test]
    fn test_process_event_directly() {
        let agents = DashMap::new();

        // Start
        process_event(
            &agents,
            &AppEventKind::TeamStepStarted {
                team: "t".into(),
                agent: "a".into(),
                step: 0,
            },
        );
        assert_eq!(agents.get("t::a").unwrap().state, AgentState::Working);

        // Complete
        process_event(
            &agents,
            &AppEventKind::TeamStepCompleted {
                team: "t".into(),
                agent: "a".into(),
            },
        );
        assert_eq!(agents.get("t::a").unwrap().state, AgentState::Idle);
    }

    #[test]
    fn test_check_timeouts_stuck() {
        let agents = DashMap::new();
        let past = Instant::now() - Duration::from_secs(350);
        agents.insert(
            "t::a".into(),
            AgentStatus {
                agent_name: "a".into(),
                team_name: "t".into(),
                state: AgentState::Working,
                last_event_at: past,
                started_at: past,
            },
        );

        let bus = EventBus::new(16);
        let mut rx = bus.subscribe_reliable();
        let config = WitnessConfig::default();

        check_timeouts(&agents, &config, &bus);

        assert_eq!(agents.get("t::a").unwrap().state, AgentState::Stuck);

        // Should have emitted AgentStuck
        let event = rx.try_recv().unwrap();
        assert_eq!(event.kind.key(), "agent_stuck");
    }

    #[test]
    fn test_check_timeouts_zombie() {
        let agents = DashMap::new();
        let past = Instant::now() - Duration::from_secs(650);
        agents.insert(
            "t::a".into(),
            AgentStatus {
                agent_name: "a".into(),
                team_name: "t".into(),
                state: AgentState::Working,
                last_event_at: past,
                started_at: past,
            },
        );

        let bus = EventBus::new(16);
        let mut rx = bus.subscribe_reliable();
        let config = WitnessConfig::default();

        check_timeouts(&agents, &config, &bus);

        assert_eq!(agents.get("t::a").unwrap().state, AgentState::Zombie);

        // Should have emitted AgentZombie (not AgentStuck, goes straight to zombie)
        let event = rx.try_recv().unwrap();
        assert_eq!(event.kind.key(), "agent_zombie");
    }

    #[test]
    fn test_check_timeouts_no_timeout() {
        let agents = DashMap::new();
        let now = Instant::now();
        agents.insert(
            "t::a".into(),
            AgentStatus {
                agent_name: "a".into(),
                team_name: "t".into(),
                state: AgentState::Working,
                last_event_at: now,
                started_at: now,
            },
        );

        let bus = EventBus::new(16);
        let mut rx = bus.subscribe_reliable();
        let config = WitnessConfig::default();

        check_timeouts(&agents, &config, &bus);

        assert_eq!(agents.get("t::a").unwrap().state, AgentState::Working);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn test_agents_in_state() {
        let agents: Arc<DashMap<String, AgentStatus>> = Arc::new(DashMap::new());
        let now = Instant::now();
        agents.insert(
            "t::a".into(),
            AgentStatus {
                agent_name: "a".into(),
                team_name: "t".into(),
                state: AgentState::Working,
                last_event_at: now,
                started_at: now,
            },
        );
        agents.insert(
            "t::b".into(),
            AgentStatus {
                agent_name: "b".into(),
                team_name: "t".into(),
                state: AgentState::Idle,
                last_event_at: now,
                started_at: now,
            },
        );

        let handle = WitnessHandle {
            agents,
            cancel: CancellationToken::new(),
        };

        let working = handle.agents_in_state(&AgentState::Working);
        assert_eq!(working.len(), 1);
        assert_eq!(working[0].agent_name, "a");

        let idle = handle.agents_in_state(&AgentState::Idle);
        assert_eq!(idle.len(), 1);
        assert_eq!(idle[0].agent_name, "b");
    }

    #[test]
    fn test_witness_config_default() {
        let config = WitnessConfig::default();
        assert_eq!(config.stuck_timeout, Duration::from_secs(300));
        assert_eq!(config.zombie_timeout, Duration::from_secs(600));
        assert_eq!(config.check_interval, Duration::from_secs(5));
    }
}
