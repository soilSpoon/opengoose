use tracing::{info, warn};

use crate::definition::{OnFailStrategy, WorkflowDef};
use crate::error::WorkflowError;
use crate::state::{LoopState, StepStatus, WorkflowState};

/// Outcome of executing a single step.
#[derive(Debug)]
pub enum StepOutcome {
    /// Step completed successfully with output text.
    Completed { output: String },
    /// Step needs to be retried (criteria not met).
    Retry { reason: String },
    /// Step permanently failed.
    Failed { reason: String },
}

/// Structured context passed to the execution callback.
#[derive(Debug, Clone)]
pub struct StepContext {
    pub step_id: String,
    pub step_name: String,
    pub agent_id: String,
    pub agent_name: String,
    pub system_prompt: String,
    pub user_prompt: String,
    pub progress: (usize, usize),
    /// For loop steps: which iteration (0-based) and total items.
    pub loop_iteration: Option<(usize, usize)>,
    /// Whether this is a verification sub-step (for verify_each loops).
    pub is_verify: bool,
    /// Per-step timeout, if configured.
    pub timeout_seconds: Option<u64>,
}

/// Drives a workflow through its steps.
///
/// The engine itself is transport-agnostic: callers provide an `execute_fn`
/// callback that sends the prompt to whichever LLM backend they use.
pub struct WorkflowEngine {
    definition: WorkflowDef,
    state: WorkflowState,
}

impl WorkflowEngine {
    /// Create a new engine for the given workflow definition and user input.
    pub fn new(definition: WorkflowDef, input: String) -> Self {
        let step_ids: Vec<String> = definition.steps.iter().map(|s| s.id.clone()).collect();
        let state = WorkflowState::new(definition.name.clone(), input, step_ids);
        Self { definition, state }
    }

    /// Resume from a previously persisted state.
    ///
    /// Returns `Err` if the state doesn't match the definition (e.g. different
    /// step count, or mismatched step IDs).
    pub fn resume(definition: WorkflowDef, state: WorkflowState) -> Result<Self, WorkflowError> {
        if state.steps.len() != definition.steps.len() {
            return Err(WorkflowError::InvalidDefinition {
                reason: format!(
                    "state has {} steps but definition has {}",
                    state.steps.len(),
                    definition.steps.len()
                ),
            });
        }
        for (i, (ss, sd)) in state.steps.iter().zip(definition.steps.iter()).enumerate() {
            if ss.step_id != sd.id {
                return Err(WorkflowError::InvalidDefinition {
                    reason: format!(
                        "step {}: state has id '{}' but definition has '{}'",
                        i, ss.step_id, sd.id
                    ),
                });
            }
        }
        Ok(Self { definition, state })
    }

    /// Get current workflow state (for persistence or UI display).
    pub fn state(&self) -> &WorkflowState {
        &self.state
    }

    /// Get a mutable reference to the workflow state.
    pub fn state_mut(&mut self) -> &mut WorkflowState {
        &mut self.state
    }

    /// Evaluate a `when` condition against current context.
    ///
    /// Supports:
    /// - `"{{key}} == value"` — equality check
    /// - `"{{key}} != value"` — inequality check
    ///
    /// Returns `true` if no condition is set (unconditional step).
    pub fn evaluate_condition(&self) -> bool {
        let step_idx = self.state.current_step;
        let step_def = match self.definition.steps.get(step_idx) {
            Some(s) => s,
            None => return false,
        };

        let condition = match &step_def.when {
            Some(c) => c,
            None => return true, // No condition = always execute
        };

        // Resolve placeholders in the condition string
        let mut resolved = condition.clone();
        resolved = resolved.replace("{{input}}", &self.state.input);
        for (key, value) in &self.state.context {
            let placeholder = format!("{{{{{key}}}}}");
            resolved = resolved.replace(&placeholder, value);
        }

        // Parse operator
        if let Some((lhs, rhs)) = resolved.split_once("!=") {
            lhs.trim() != rhs.trim()
        } else if let Some((lhs, rhs)) = resolved.split_once("==") {
            lhs.trim() == rhs.trim()
        } else {
            // If no operator, treat as truthy (non-empty after resolution)
            !resolved.trim().is_empty()
        }
    }

    /// Skip the current step (used when `when` condition is false).
    pub fn skip_current(&mut self) {
        let idx = self.state.current_step;
        if let Some(step) = self.state.steps.get_mut(idx) {
            info!(step = %step.step_id, "skipping step (condition not met)");
            step.status = StepStatus::Skipped;
        }
        self.state.current_step += 1;
    }

    /// Build the fully-resolved prompt for the current step.
    ///
    /// Resolves placeholders in this order:
    /// 1. `{{input}}` — original user request
    /// 2. `{{step_id}}` — output from a named dependency step
    /// 3. `{{key}}` — value from the shared mutable context
    /// 4. Loop-specific: `{{current_item}}`, `{{completed_items}}`, `{{items_remaining}}`
    ///
    /// Returns `Err` if a declared dependency has no output.
    pub fn current_prompt(&self) -> Result<Option<String>, WorkflowError> {
        let step_idx = self.state.current_step;
        let step_def = match self.definition.steps.get(step_idx) {
            Some(s) => s,
            None => return Ok(None),
        };

        let mut prompt = step_def.prompt.clone();

        // 1. Substitute {{input}}
        prompt = prompt.replace("{{input}}", &self.state.input);

        // 2. Substitute {{step_id}} from dependency outputs
        for dep_id in &step_def.depends_on {
            let placeholder = format!("{{{{{dep_id}}}}}");
            match self.state.step_output(dep_id) {
                Some(output) => {
                    prompt = prompt.replace(&placeholder, output);
                }
                None => {
                    let dep_status = self
                        .state
                        .steps
                        .iter()
                        .find(|s| s.step_id == *dep_id)
                        .map(|s| format!("{:?}", s.status))
                        .unwrap_or_else(|| "unknown".into());
                    return Err(WorkflowError::UnsatisfiedDependency {
                        step: step_def.id.clone(),
                        dependency: dep_id.clone(),
                        status: dep_status,
                    });
                }
            }
        }

        // 3. Substitute {{key}} from shared context (antfarm-style)
        for (key, value) in &self.state.context {
            let placeholder = format!("{{{{{key}}}}}");
            prompt = prompt.replace(&placeholder, value);
        }

        // 4. Loop-specific substitutions
        if let Some(loop_state) = self
            .state
            .steps
            .get(step_idx)
            .and_then(|s| s.loop_state.as_ref())
        {
            if let Some(item) = loop_state.current_item() {
                prompt = prompt.replace("{{current_item}}", item);
            }

            let completed: Vec<&str> = loop_state
                .iteration_outputs
                .iter()
                .filter_map(|o| o.as_deref())
                .collect();
            prompt = prompt.replace("{{completed_items}}", &completed.join("\n---\n"));

            let remaining = loop_state
                .items
                .len()
                .saturating_sub(loop_state.current_index + 1);
            prompt = prompt.replace("{{items_remaining}}", &remaining.to_string());
        }

        // 5. Append acceptance criteria
        if !step_def.expects.is_empty() {
            prompt.push_str(
                "\n\n---\nAcceptance Criteria (your output MUST satisfy all of these):\n",
            );
            for (i, criterion) in step_def.expects.iter().enumerate() {
                prompt.push_str(&format!("{}. {}\n", i + 1, criterion));
            }
        }

        Ok(Some(prompt))
    }

    /// Build a verification prompt for the current loop iteration.
    ///
    /// Returns `None` if verify_each is disabled or no verify_step is configured.
    pub fn current_verify_context(&self) -> Result<Option<StepContext>, WorkflowError> {
        let step_idx = self.state.current_step;
        let step_def = match self.definition.steps.get(step_idx) {
            Some(s) => s,
            None => return Ok(None),
        };

        let loop_config = match &step_def.loop_config {
            Some(c) if c.verify_each => c,
            _ => return Ok(None),
        };

        let verify_step_id = match &loop_config.verify_step {
            Some(id) => id,
            None => return Ok(None),
        };

        // Find the verify step definition
        let verify_def = self
            .definition
            .steps
            .iter()
            .find(|s| s.id == *verify_step_id)
            .ok_or_else(|| WorkflowError::InvalidDefinition {
                reason: format!(
                    "loop step '{}': verify_step '{}' not found",
                    step_def.id, verify_step_id
                ),
            })?;

        let agent = self
            .definition
            .agents
            .iter()
            .find(|a| a.id == verify_def.agent)
            .ok_or_else(|| WorkflowError::UnknownAgent {
                step: verify_def.id.clone(),
                agent: verify_def.agent.clone(),
            })?;

        let loop_state = self
            .state
            .steps
            .get(step_idx)
            .and_then(|s| s.loop_state.as_ref())
            .ok_or_else(|| WorkflowError::InvalidDefinition {
                reason: format!(
                    "loop step '{}': loop state not initialized",
                    step_def.id
                ),
            })?;

        // Build verify prompt with special placeholders
        let mut prompt = verify_def.prompt.clone();
        prompt = prompt.replace("{{input}}", &self.state.input);

        if let Some(item) = loop_state.current_item() {
            prompt = prompt.replace("{{current_item}}", item);
        }

        // {{iteration_output}} is the output from the iteration just completed
        let iteration_output = loop_state
            .iteration_outputs
            .get(loop_state.current_index)
            .and_then(|o| o.as_deref())
            .unwrap_or("");
        prompt = prompt.replace("{{iteration_output}}", iteration_output);

        // Also resolve context keys
        for (key, value) in &self.state.context {
            let placeholder = format!("{{{{{key}}}}}");
            prompt = prompt.replace(&placeholder, value);
        }

        if !verify_def.expects.is_empty() {
            prompt.push_str(
                "\n\n---\nAcceptance Criteria (your output MUST satisfy all of these):\n",
            );
            for (i, criterion) in verify_def.expects.iter().enumerate() {
                prompt.push_str(&format!("{}. {}\n", i + 1, criterion));
            }
        }

        Ok(Some(StepContext {
            step_id: format!("{}_verify", step_def.id),
            step_name: format!("{} (verify)", verify_def.name),
            agent_id: agent.id.clone(),
            agent_name: agent.name.clone(),
            system_prompt: agent.system_prompt.clone(),
            user_prompt: prompt,
            progress: self.progress(),
            loop_iteration: Some((loop_state.current_index, loop_state.items.len())),
            is_verify: true,
            timeout_seconds: verify_def.timeout_seconds,
        }))
    }

    /// Build a structured `StepContext` for the current step.
    pub fn current_step_context(&self) -> Result<Option<StepContext>, WorkflowError> {
        let prompt = match self.current_prompt()? {
            Some(p) => p,
            None => return Ok(None),
        };

        let step_idx = self.state.current_step;
        let step_def = match self.definition.steps.get(step_idx) {
            Some(s) => s,
            None => return Ok(None),
        };
        let agent = self
            .definition
            .agents
            .iter()
            .find(|a| a.id == step_def.agent)
            .ok_or_else(|| WorkflowError::UnknownAgent {
                step: step_def.id.clone(),
                agent: step_def.agent.clone(),
            })?;

        let loop_iteration = self
            .state
            .steps
            .get(step_idx)
            .and_then(|s| s.loop_state.as_ref())
            .map(|ls| (ls.current_index, ls.items.len()));

        Ok(Some(StepContext {
            step_id: step_def.id.clone(),
            step_name: step_def.name.clone(),
            agent_id: agent.id.clone(),
            agent_name: agent.name.clone(),
            system_prompt: agent.system_prompt.clone(),
            user_prompt: prompt,
            progress: self.progress(),
            loop_iteration,
            is_verify: false,
            timeout_seconds: step_def.timeout_seconds,
        }))
    }

    /// Get the system prompt (persona) for the current step's agent.
    pub fn current_agent_system_prompt(&self) -> Option<&str> {
        let step_def = self.definition.steps.get(self.state.current_step)?;
        self.definition
            .agents
            .iter()
            .find(|a| a.id == step_def.agent)
            .map(|a| a.system_prompt.as_str())
    }

    /// Get metadata about the current step.
    pub fn current_step_info(&self) -> Option<(&str, &str, &str)> {
        let step_def = self.definition.steps.get(self.state.current_step)?;
        let agent = self
            .definition
            .agents
            .iter()
            .find(|a| a.id == step_def.agent)?;
        Some((&step_def.id, &step_def.name, &agent.name))
    }

    /// Initialize a loop step by parsing items from context or a dependency.
    ///
    /// Call this before the first iteration of a loop step. The items are
    /// sourced from the context key specified in `loop.over`, parsed as a
    /// JSON array of strings.
    pub fn init_loop(&mut self) -> Result<bool, WorkflowError> {
        let idx = self.state.current_step;
        let step_def = match self.definition.steps.get(idx) {
            Some(s) => s,
            None => return Ok(false),
        };

        let loop_config = match &step_def.loop_config {
            Some(c) => c,
            None => return Ok(false), // Not a loop step
        };

        // Already initialized
        if self.state.steps[idx].loop_state.is_some() {
            return Ok(true);
        }

        // Context keys are stored lowercased (see extract_context), so
        // normalize the lookup key to match.
        let key = loop_config.over.to_lowercase();

        // Try to find items from context (lowercased key)
        let items_json = self
            .state
            .context
            .get(&key)
            .cloned()
            .ok_or_else(|| WorkflowError::UnsatisfiedDependency {
                step: step_def.id.clone(),
                dependency: format!("context key '{key}'"),
                status: "not found in context".into(),
            })?;

        // Parse as JSON array of strings (or objects serialized to strings)
        let items: Vec<String> = serde_json::from_str::<Vec<serde_json::Value>>(&items_json)
            .map_err(|e| WorkflowError::InvalidDefinition {
                reason: format!(
                    "loop step '{}': failed to parse '{}' as JSON array: {}",
                    step_def.id, key, e
                ),
            })?
            .into_iter()
            .map(|v| match v {
                serde_json::Value::String(s) => s,
                other => other.to_string(),
            })
            .collect();

        if items.is_empty() {
            info!(step = %step_def.id, "loop has zero items, skipping");
            self.state.steps[idx].status = StepStatus::Skipped;
            self.state.steps[idx].output = Some("(no items to process)".into());
            self.state.current_step += 1;
            return Ok(false);
        }

        info!(step = %step_def.id, items = items.len(), "initialized loop step");
        self.state.steps[idx].loop_state = Some(LoopState::new(items));
        Ok(true)
    }

    /// Check if the current loop iteration needs verification.
    pub fn needs_verify(&self) -> bool {
        let idx = self.state.current_step;
        let step_def = match self.definition.steps.get(idx) {
            Some(s) => s,
            None => return false,
        };

        match &step_def.loop_config {
            Some(c) if c.verify_each && c.verify_step.is_some() => self
                .state
                .steps
                .get(idx)
                .and_then(|s| s.loop_state.as_ref())
                .map_or(false, |ls| ls.pending_verify),
            _ => false,
        }
    }

    /// Record the outcome of a verification step for the current loop iteration.
    ///
    /// If the verifier output contains `STATUS: retry`, the iteration is retried.
    /// Otherwise, the iteration is accepted and the loop advances.
    pub fn record_verify_outcome(&mut self, outcome: StepOutcome) -> bool {
        let idx = self.state.current_step;
        let max_retries = self.definition.steps.get(idx).map_or(2, |s| s.max_retries);

        match outcome {
            StepOutcome::Completed { output } => {
                // Check if verifier says retry
                let should_retry = output.lines().any(|line| {
                    let line = line.trim();
                    line.starts_with("STATUS:") && line.contains("retry")
                });

                if should_retry {
                    let step = &mut self.state.steps[idx];
                    if let Some(ref mut ls) = step.loop_state {
                        ls.pending_verify = false;
                        let cur = ls.current_index;
                        if cur < ls.iteration_outputs.len() {
                            ls.iteration_outputs[cur] = None;
                        }
                        ls.iteration_retries += 1;
                        if ls.iteration_retries >= max_retries {
                            warn!(
                                step = %step.step_id,
                                iteration = cur,
                                "loop iteration exhausted verify retries"
                            );
                            ls.advance();
                        } else {
                            info!(
                                step = %step.step_id,
                                iteration = cur,
                                "verify requested retry for iteration"
                            );
                        }
                        step.status = StepStatus::Pending;
                    }
                } else {
                    // Verification passed — extract context first, then advance
                    self.state.extract_context(&output);
                    let step = &mut self.state.steps[idx];
                    if let Some(ref mut ls) = step.loop_state {
                        ls.pending_verify = false;
                        ls.advance();
                        if ls.is_done() {
                            info!(step = %step.step_id, "loop step completed all iterations (verified)");
                            let accumulated = step
                                .loop_state
                                .as_ref()
                                .map(|ls| ls.accumulated_output())
                                .unwrap_or_default();
                            step.status = StepStatus::Completed;
                            step.output = Some(accumulated);
                            self.state.current_step += 1;
                        } else {
                            step.status = StepStatus::Pending;
                        }
                    }
                }
            }
            StepOutcome::Retry { .. } | StepOutcome::Failed { .. } => {
                // Treat verify failure as "accept and move on"
                let step = &mut self.state.steps[idx];
                if let Some(ref mut ls) = step.loop_state {
                    ls.pending_verify = false;
                    ls.advance();
                    if ls.is_done() {
                        let accumulated = step
                            .loop_state
                            .as_ref()
                            .map(|ls| ls.accumulated_output())
                            .unwrap_or_default();
                        step.status = StepStatus::Completed;
                        step.output = Some(accumulated);
                        self.state.current_step += 1;
                    } else {
                        step.status = StepStatus::Pending;
                    }
                }
            }
        }

        !self.state.is_terminal()
    }

    /// Record the outcome of executing the current step and advance.
    ///
    /// For loop steps:
    /// - Each iteration's output is stored individually
    /// - On completion of all iterations, the accumulated output becomes the step output
    /// - If verify_each is enabled, the step enters `pending_verify` state
    ///   (caller must then run the verify context and call `record_verify_outcome`)
    ///
    /// Returns `true` if the workflow has more steps to execute.
    pub fn record_outcome(&mut self, outcome: StepOutcome) -> bool {
        let idx = self.state.current_step;

        if idx >= self.state.steps.len() || idx >= self.definition.steps.len() {
            return false;
        }

        let max_retries = self.definition.steps[idx].max_retries;
        let on_fail = self.definition.steps[idx].on_fail.clone();
        let is_loop = self.definition.steps[idx].loop_config.is_some();
        let has_verify = self.definition.steps[idx]
            .loop_config
            .as_ref()
            .map_or(false, |c| c.verify_each && c.verify_step.is_some());

        match outcome {
            StepOutcome::Completed { output } => {
                // Extract KEY: VALUE pairs into shared context first
                self.state.extract_context(&output);

                let step = &mut self.state.steps[idx];
                if is_loop {
                    if let Some(ref mut ls) = step.loop_state {
                        let cur = ls.current_index;
                        if cur < ls.iteration_outputs.len() {
                            ls.iteration_outputs[cur] = Some(output.clone());
                        }

                        if has_verify {
                            // Don't advance yet — wait for verification
                            ls.pending_verify = true;
                            step.status = StepStatus::Running;
                        } else {
                            ls.advance();
                            if ls.is_done() {
                                info!(step = %step.step_id, "loop step completed all iterations");
                                let accumulated = step
                                    .loop_state
                                    .as_ref()
                                    .map(|ls| ls.accumulated_output())
                                    .unwrap_or_default();
                                step.status = StepStatus::Completed;
                                step.output = Some(accumulated);
                                self.state.current_step += 1;
                            } else {
                                info!(
                                    step = %step.step_id,
                                    iteration = ls.current_index,
                                    total = ls.items.len(),
                                    "loop step advancing to next iteration"
                                );
                                step.status = StepStatus::Pending;
                            }
                        }
                    } else {
                        step.status = StepStatus::Completed;
                        step.output = Some(output);
                        self.state.current_step += 1;
                    }
                } else {
                    info!(step = %step.step_id, "step completed");
                    step.status = StepStatus::Completed;
                    step.output = Some(output);
                    self.state.current_step += 1;
                }
            }
            StepOutcome::Retry { reason } => {
                let step = &mut self.state.steps[idx];
                step.retries += 1;
                if step.retries >= max_retries {
                    warn!(step = %step.step_id, retries = step.retries, %reason, "step exhausted retries");
                    match on_fail {
                        OnFailStrategy::Skip => {
                            info!(step = %step.step_id, "skipping failed step per on_fail policy");
                            step.status = StepStatus::Skipped;
                            self.state.current_step += 1;
                        }
                        OnFailStrategy::Abort => {
                            step.status = StepStatus::Failed;
                        }
                    }
                } else {
                    info!(step = %step.step_id, retries = step.retries, %reason, "retrying step");
                    step.status = StepStatus::Pending;
                }
            }
            StepOutcome::Failed { reason } => {
                let step = &mut self.state.steps[idx];
                warn!(step = %step.step_id, %reason, "step failed permanently");
                match on_fail {
                    OnFailStrategy::Skip => {
                        info!(step = %step.step_id, "skipping failed step per on_fail policy");
                        step.status = StepStatus::Skipped;
                        self.state.current_step += 1;
                    }
                    OnFailStrategy::Abort => {
                        step.status = StepStatus::Failed;
                    }
                }
            }
        }

        !self.state.is_terminal()
    }

    /// Mark the current step as running.
    pub fn mark_running(&mut self) {
        if let Some(step) = self.state.steps.get_mut(self.state.current_step) {
            step.status = StepStatus::Running;
        }
    }

    /// Whether the current step is used only as a verify_step by a loop step.
    /// Such steps are invoked inline during loop verification and should be
    /// auto-skipped when encountered at the top level.
    pub fn is_current_verify_only(&self) -> bool {
        let step_id = match self.definition.steps.get(self.state.current_step) {
            Some(s) => &s.id,
            None => return false,
        };

        self.definition.steps.iter().any(|s| {
            s.loop_config
                .as_ref()
                .and_then(|c| c.verify_step.as_ref())
                .map_or(false, |vs| vs == step_id)
        })
    }

    /// Whether the current step is a loop step.
    pub fn is_current_loop(&self) -> bool {
        self.definition
            .steps
            .get(self.state.current_step)
            .and_then(|s| s.loop_config.as_ref())
            .is_some()
    }

    /// Total number of steps.
    pub fn total_steps(&self) -> usize {
        self.definition.steps.len()
    }

    /// Summary of progress: (completed, total).
    pub fn progress(&self) -> (usize, usize) {
        let completed = self
            .state
            .steps
            .iter()
            .filter(|s| matches!(s.status, StepStatus::Completed | StepStatus::Skipped))
            .count();
        (completed, self.total_steps())
    }
}
