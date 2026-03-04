use anyhow::Result;
use tracing::info;

use opengoose_types::{AppEventKind, EventBus};
use opengoose_workflows::{
    StepContext, StepOutcome, WorkflowEngine, WorkflowLoader, WorkflowStore,
};

/// Runs antfarm-style multi-agent workflows over the OpenGoose gateway.
///
/// Each workflow step is executed by relaying a prompt (with the step's agent
/// persona context) through the existing Goose session. The runner emits
/// workflow lifecycle events on the event bus so the TUI can display progress.
pub struct WorkflowRunner {
    loader: WorkflowLoader,
    event_bus: EventBus,
    store: Option<WorkflowStore>,
}

impl WorkflowRunner {
    pub fn new(event_bus: EventBus) -> Self {
        // Try to create persistence store; proceed without it if it fails
        let store = WorkflowStore::new(WorkflowStore::default_dir()).ok();
        Self {
            loader: WorkflowLoader::new(),
            event_bus,
            store,
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

    /// Resume a previously saved workflow run.
    pub fn resume_run(&self, run_id: &str, workflow_name: &str) -> Result<WorkflowEngine> {
        let store = self
            .store
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("persistence not available"))?;

        let state = store.load(run_id, workflow_name)?;

        let def = self
            .loader
            .get(workflow_name)
            .ok_or_else(|| opengoose_workflows::WorkflowError::NotFound {
                name: workflow_name.to_string(),
            })?
            .clone();

        Ok(WorkflowEngine::resume(def, state))
    }

    /// Start a workflow execution.
    ///
    /// The `execute_step` callback receives a [`StepContext`] with full metadata
    /// about the current step (agent persona, resolved prompt, progress) and
    /// should return the agent's response text.
    pub async fn run<F, Fut>(
        &self,
        workflow_name: &str,
        input: String,
        run_id: &str,
        mut execute_step: F,
    ) -> Result<String>
    where
        F: FnMut(StepContext) -> Fut,
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

        loop {
            // Initialize loop steps before prompting
            if engine.is_current_loop() {
                match engine.init_loop() {
                    Ok(true) => {} // Loop initialized or already running
                    Ok(false) => continue, // Loop was empty, step was skipped
                    Err(e) => {
                        self.event_bus.emit(AppEventKind::WorkflowFailed {
                            workflow: workflow_name.to_string(),
                            reason: e.to_string(),
                        });
                        anyhow::bail!(e);
                    }
                }
            }

            let ctx = match engine.current_step_context() {
                Ok(Some(ctx)) => ctx,
                Ok(None) => break,
                Err(e) => {
                    self.event_bus.emit(AppEventKind::WorkflowFailed {
                        workflow: workflow_name.to_string(),
                        reason: e.to_string(),
                    });
                    anyhow::bail!(e);
                }
            };

            let step_id = ctx.step_id.clone();
            let agent_name = ctx.agent_name.clone();

            let iteration_info = ctx
                .loop_iteration
                .map(|(i, n)| format!(" [{}/{}]", i + 1, n))
                .unwrap_or_default();

            self.event_bus.emit(AppEventKind::WorkflowStepStarted {
                workflow: workflow_name.to_string(),
                step: format!("{step_id}{iteration_info}"),
                agent: agent_name.clone(),
            });

            engine.mark_running();

            let (completed, total) = ctx.progress;
            info!(
                workflow = workflow_name,
                step = %step_id,
                agent = %agent_name,
                progress = format!("{}/{}", completed + 1, total),
                "executing workflow step"
            );

            match execute_step(ctx).await {
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

            // Persist state after each step outcome
            if let Some(ref store) = self.store {
                if let Err(e) = store.save(run_id, engine.state()) {
                    info!("failed to persist workflow state: {e}");
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

        // Clean up persisted state on success
        if let Some(ref store) = self.store {
            let _ = store.remove(run_id, workflow_name);
        }

        let final_output = state
            .steps
            .last()
            .and_then(|s| s.output.as_deref())
            .unwrap_or("(no output)")
            .to_string();

        Ok(final_output)
    }
}
