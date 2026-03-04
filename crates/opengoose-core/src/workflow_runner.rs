use anyhow::Result;
use tracing::info;

use opengoose_types::{AppEventKind, EventBus};
use opengoose_workflows::{StepOutcome, WorkflowEngine, WorkflowLoader};

/// Runs antfarm-style multi-agent workflows over the OpenGoose gateway.
///
/// Each workflow step is executed by relaying a prompt (with the step's agent
/// persona context) through the existing Goose session. The runner emits
/// workflow lifecycle events on the event bus so the TUI can display progress.
pub struct WorkflowRunner {
    loader: WorkflowLoader,
    event_bus: EventBus,
}

impl WorkflowRunner {
    pub fn new(event_bus: EventBus) -> Self {
        Self {
            loader: WorkflowLoader::new(),
            event_bus,
        }
    }

    /// Load bundled workflow definitions from the `workflows/` directory.
    pub fn load_bundled(&mut self) -> Result<usize> {
        let dir = WorkflowLoader::bundled_dir();
        let count = self.loader.load_dir(&dir)?;
        Ok(count)
    }

    /// Load a workflow from a YAML string.
    pub fn load_yaml(&mut self, yaml: &str) -> Result<()> {
        self.loader.load_str(yaml)?;
        Ok(())
    }

    /// List available workflow names.
    pub fn list_workflows(&self) -> Vec<&str> {
        self.loader.list()
    }

    /// Start a workflow execution for the given session.
    ///
    /// The `execute_step` callback is invoked for each step with
    /// `(system_prompt, user_prompt)` and should return the agent's response.
    /// This allows callers to route steps through Goose sessions, direct API
    /// calls, or any other backend.
    pub async fn run<F, Fut>(
        &self,
        workflow_name: &str,
        input: String,
        mut execute_step: F,
    ) -> Result<String>
    where
        F: FnMut(String, String) -> Fut,
        Fut: std::future::Future<Output = Result<String>>,
    {
        let def = self
            .loader
            .get(workflow_name)
            .ok_or_else(|| opengoose_workflows::WorkflowError::NotFound {
                name: workflow_name.to_string(),
            })?
            .clone();

        let mut engine = WorkflowEngine::new(def, input.clone());

        self.event_bus.emit(AppEventKind::WorkflowStarted {
            workflow: workflow_name.to_string(),
            input: input.clone(),
        });

        while let Some(prompt) = engine.current_prompt() {
            let (step_id, agent_name, system_prompt) = {
                let (sid, _sname, aname) = engine
                    .current_step_info()
                    .expect("current_prompt returned Some so step must exist");
                let sp = engine
                    .current_agent_system_prompt()
                    .unwrap_or("")
                    .to_string();
                (sid.to_string(), aname.to_string(), sp)
            };

            self.event_bus.emit(AppEventKind::WorkflowStepStarted {
                workflow: workflow_name.to_string(),
                step: step_id.clone(),
                agent: agent_name.clone(),
            });

            engine.mark_running();

            let (completed, total) = engine.progress();
            info!(
                workflow = workflow_name,
                step = %step_id,
                agent = %agent_name,
                progress = format!("{}/{}", completed + 1, total),
                "executing workflow step"
            );

            match execute_step(system_prompt, prompt).await {
                Ok(output) => {
                    self.event_bus.emit(AppEventKind::WorkflowStepCompleted {
                        workflow: workflow_name.to_string(),
                        step: step_id.clone(),
                    });

                    if !engine.record_outcome(StepOutcome::Completed { output }) {
                        break;
                    }
                }
                Err(e) => {
                    let reason = e.to_string();
                    self.event_bus.emit(AppEventKind::WorkflowStepFailed {
                        workflow: workflow_name.to_string(),
                        step: step_id.clone(),
                        reason: reason.clone(),
                    });

                    if !engine.record_outcome(StepOutcome::Retry { reason }) {
                        break;
                    }
                }
            }
        }

        let state = engine.state();
        if state.is_failed() {
            let reason = "one or more steps failed".to_string();
            self.event_bus.emit(AppEventKind::WorkflowFailed {
                workflow: workflow_name.to_string(),
                reason: reason.clone(),
            });
            anyhow::bail!("workflow '{}' failed: {}", workflow_name, reason);
        }

        self.event_bus.emit(AppEventKind::WorkflowCompleted {
            workflow: workflow_name.to_string(),
        });

        // Return the output of the final step as the workflow result
        let final_output = state
            .steps
            .last()
            .and_then(|s| s.output.as_deref())
            .unwrap_or("(no output)")
            .to_string();

        Ok(final_output)
    }
}
