//! Pure lifecycle state for the project-scoped local tool loop.
//!
//! This module owns no clock, filesystem, process, prompt, or policy adapter.
//! Callers supply elapsed durations and typed observations, which keeps every
//! transition deterministic and unit-testable.

use std::collections::HashSet;
use std::time::Duration;

use crate::runtime_core::agent::{
    AgentToolRegistrySnapshot, LocalAgentToolCall, LocalDecisionErrorKind, ToolObservation,
    ToolObservationReason, ToolObservationStatus,
};

pub(super) const MAX_MODEL_TURNS: u8 = 8;
pub(super) const MAX_TOOL_CALLS: u8 = 6;
pub(super) const TOOL_TIMEOUT: Duration = Duration::from_secs(5);
pub(super) const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
pub(super) const MAX_OBSERVATION_BYTES: usize = 16 * 1024;
pub(super) const MAX_CUMULATIVE_OBSERVATION_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LocalLoopTerminalReason {
    ModelTurnBudget,
    ToolCallBudget,
    RepeatedToolCall,
    ProtocolError,
    Cancelled,
    ToolTimeout,
    RequestDeadline,
    ObservationBudget,
    Answer,
    ProposeMutation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LocalLoopTerminal {
    pub(super) reason: LocalLoopTerminalReason,
    pub(super) observation: Option<ToolObservation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ToolAdmission {
    Execute,
    Replan(ToolObservation),
    Terminate(LocalLoopTerminal),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ObservationTransition {
    Replan(ToolObservation),
    Terminate(LocalLoopTerminal),
}

#[derive(Debug, Clone)]
pub(super) struct LocalLoopState {
    registry: AgentToolRegistrySnapshot,
    model_turns: u8,
    admitted_tool_calls: u8,
    cumulative_observation_bytes: usize,
    consecutive_protocol_errors: u8,
    normalized_calls: HashSet<String>,
}

impl LocalLoopState {
    pub(super) fn new(registry: AgentToolRegistrySnapshot) -> Self {
        Self {
            registry,
            model_turns: 0,
            admitted_tool_calls: 0,
            cumulative_observation_bytes: 0,
            consecutive_protocol_errors: 0,
            normalized_calls: HashSet::new(),
        }
    }

    pub(super) fn tool_timeout(&self) -> Duration {
        TOOL_TIMEOUT
    }

    pub(super) fn remaining_request_time(
        &self,
        elapsed: Duration,
    ) -> Result<Duration, LocalLoopTerminal> {
        REQUEST_TIMEOUT
            .checked_sub(elapsed)
            .filter(|remaining| !remaining.is_zero())
            .ok_or_else(|| terminal(LocalLoopTerminalReason::RequestDeadline, None))
    }

    pub(super) fn begin_model_turn(&mut self, elapsed: Duration) -> Result<(), LocalLoopTerminal> {
        self.ensure_request_time(elapsed)?;
        if self.model_turns >= MAX_MODEL_TURNS {
            return Err(terminal(LocalLoopTerminalReason::ModelTurnBudget, None));
        }
        self.model_turns += 1;
        Ok(())
    }

    pub(super) fn admit_tool_call(
        &mut self,
        call: &LocalAgentToolCall,
        elapsed: Duration,
    ) -> ToolAdmission {
        if let Err(terminal) = self.ensure_request_time(elapsed) {
            return ToolAdmission::Terminate(terminal);
        }
        if !self.registry.advertises(call.id) {
            return self.protocol_error(Some(call.id), LocalDecisionErrorKind::UnknownOrStale);
        }

        let normalized = call.normalized_key();
        if self.normalized_calls.contains(&normalized) {
            return ToolAdmission::Terminate(terminal(
                LocalLoopTerminalReason::RepeatedToolCall,
                None,
            ));
        }
        if self.admitted_tool_calls >= MAX_TOOL_CALLS {
            return ToolAdmission::Terminate(terminal(
                LocalLoopTerminalReason::ToolCallBudget,
                None,
            ));
        }

        self.normalized_calls.insert(normalized);
        self.admitted_tool_calls += 1;
        self.consecutive_protocol_errors = 0;
        ToolAdmission::Execute
    }

    pub(super) fn record_protocol_error(
        &mut self,
        tool_id: Option<crate::runtime_core::agent::AgentToolId>,
        kind: LocalDecisionErrorKind,
        elapsed: Duration,
    ) -> ToolAdmission {
        if let Err(terminal) = self.ensure_request_time(elapsed) {
            return ToolAdmission::Terminate(terminal);
        }
        self.protocol_error(tool_id, kind)
    }

    pub(super) fn record_observation(
        &mut self,
        observation: ToolObservation,
        elapsed: Duration,
    ) -> ObservationTransition {
        if let Err(mut terminal) = self.ensure_request_time(elapsed) {
            terminal.observation = Some(self.bound_observation(observation).0);
            return ObservationTransition::Terminate(terminal);
        }

        let terminal_status = match observation.status {
            ToolObservationStatus::Cancelled => Some(LocalLoopTerminalReason::Cancelled),
            ToolObservationStatus::Timeout => Some(LocalLoopTerminalReason::ToolTimeout),
            _ => None,
        };
        let protocol_kind = match observation.status {
            ToolObservationStatus::Malformed => Some(LocalDecisionErrorKind::Malformed),
            ToolObservationStatus::UnknownOrStale => Some(LocalDecisionErrorKind::UnknownOrStale),
            _ => None,
        };
        let (observation, observation_budget_exhausted) = self.bound_observation(observation);

        if observation_budget_exhausted {
            return ObservationTransition::Terminate(terminal(
                LocalLoopTerminalReason::ObservationBudget,
                Some(observation),
            ));
        }
        if let Some(reason) = terminal_status {
            return ObservationTransition::Terminate(terminal(reason, Some(observation)));
        }
        if protocol_kind.is_some() {
            self.consecutive_protocol_errors += 1;
            if self.consecutive_protocol_errors > 1 {
                return ObservationTransition::Terminate(terminal(
                    LocalLoopTerminalReason::ProtocolError,
                    Some(observation),
                ));
            }
        }

        ObservationTransition::Replan(observation)
    }

    pub(super) fn terminal_decision(&self, mutation: bool, elapsed: Duration) -> LocalLoopTerminal {
        if elapsed >= REQUEST_TIMEOUT {
            return terminal(LocalLoopTerminalReason::RequestDeadline, None);
        }
        terminal(
            if mutation {
                LocalLoopTerminalReason::ProposeMutation
            } else {
                LocalLoopTerminalReason::Answer
            },
            None,
        )
    }

    fn protocol_error(
        &mut self,
        tool_id: Option<crate::runtime_core::agent::AgentToolId>,
        kind: LocalDecisionErrorKind,
    ) -> ToolAdmission {
        self.consecutive_protocol_errors += 1;
        let (status, reason, content) = match kind {
            LocalDecisionErrorKind::Malformed => (
                ToolObservationStatus::Malformed,
                ToolObservationReason::InvalidArguments,
                "tool call arguments are malformed",
            ),
            LocalDecisionErrorKind::UnknownOrStale => (
                ToolObservationStatus::UnknownOrStale,
                ToolObservationReason::UnknownOrStaleTool,
                "tool id is unknown or was not advertised for this request",
            ),
        };
        let observation = ToolObservation::new(tool_id, status, reason, content);
        if self.consecutive_protocol_errors > 1 {
            ToolAdmission::Terminate(terminal(
                LocalLoopTerminalReason::ProtocolError,
                Some(observation),
            ))
        } else {
            ToolAdmission::Replan(observation)
        }
    }

    fn ensure_request_time(&self, elapsed: Duration) -> Result<(), LocalLoopTerminal> {
        self.remaining_request_time(elapsed).map(|_| ())
    }

    fn bound_observation(&mut self, mut observation: ToolObservation) -> (ToolObservation, bool) {
        let original_bytes = observation.content.len();
        let remaining =
            MAX_CUMULATIVE_OBSERVATION_BYTES.saturating_sub(self.cumulative_observation_bytes);
        let allowed = MAX_OBSERVATION_BYTES.min(remaining);
        if original_bytes > allowed {
            observation.content = truncate_utf8(&observation.content, allowed).to_string();
            observation.status = ToolObservationStatus::Truncated;
            observation.reason = if remaining == 0 {
                ToolObservationReason::ObservationBudgetExceeded
            } else {
                ToolObservationReason::OutputTruncated
            };
        }
        let returned_bytes = observation.content.len();
        observation.truncation = crate::runtime_core::agent::ToolObservationTruncation {
            truncated: returned_bytes < original_bytes,
            original_bytes,
            returned_bytes,
        };
        self.cumulative_observation_bytes += returned_bytes;
        let exhausted = remaining == 0 || original_bytes > remaining;
        (observation, exhausted)
    }
}

fn terminal(
    reason: LocalLoopTerminalReason,
    observation: Option<ToolObservation>,
) -> LocalLoopTerminal {
    LocalLoopTerminal {
        reason,
        observation,
    }
}

fn truncate_utf8(value: &str, max_bytes: usize) -> &str {
    if value.len() <= max_bytes {
        return value;
    }
    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_core::agent::{AgentToolId, ToolObservationReason};

    fn call(id: AgentToolId, input: &str) -> LocalAgentToolCall {
        LocalAgentToolCall {
            id,
            input: input.to_string(),
        }
    }

    fn observation(status: ToolObservationStatus, content: impl Into<String>) -> ToolObservation {
        let reason = match status {
            ToolObservationStatus::Ok => ToolObservationReason::Completed,
            ToolObservationStatus::NotFound => ToolObservationReason::NotFound,
            ToolObservationStatus::Denied => ToolObservationReason::PolicyDenied,
            ToolObservationStatus::ToolError => ToolObservationReason::ExecutionFailed,
            ToolObservationStatus::Truncated => ToolObservationReason::OutputTruncated,
            ToolObservationStatus::Malformed => ToolObservationReason::InvalidArguments,
            ToolObservationStatus::UnknownOrStale => ToolObservationReason::UnknownOrStaleTool,
            ToolObservationStatus::Cancelled => ToolObservationReason::RequestCancelled,
            ToolObservationStatus::Timeout => ToolObservationReason::ToolTimedOut,
        };
        ToolObservation::new(Some(AgentToolId::ReadFile), status, reason, content)
    }

    #[test]
    fn enforces_model_call_repeat_and_deadline_budgets() {
        let registry = AgentToolRegistrySnapshot::local_default();
        let mut turns = LocalLoopState::new(registry.clone());
        for _ in 0..MAX_MODEL_TURNS {
            assert_eq!(turns.begin_model_turn(Duration::ZERO), Ok(()));
        }
        assert_eq!(
            turns.begin_model_turn(Duration::ZERO).unwrap_err().reason,
            LocalLoopTerminalReason::ModelTurnBudget
        );

        let mut calls = LocalLoopState::new(registry.clone());
        for index in 0..MAX_TOOL_CALLS {
            assert_eq!(
                calls.admit_tool_call(
                    &call(AgentToolId::ReadFile, &format!("file-{index}")),
                    Duration::ZERO,
                ),
                ToolAdmission::Execute
            );
        }
        assert_eq!(
            calls.admit_tool_call(&call(AgentToolId::ListDirectory, "."), Duration::ZERO,),
            ToolAdmission::Terminate(terminal(LocalLoopTerminalReason::ToolCallBudget, None))
        );

        let mut repeated = LocalLoopState::new(registry);
        assert_eq!(
            repeated.admit_tool_call(
                &call(AgentToolId::ReadFile, " src/main.rs "),
                Duration::ZERO,
            ),
            ToolAdmission::Execute
        );
        assert_eq!(
            repeated.admit_tool_call(&call(AgentToolId::ReadFile, "src/main.rs"), Duration::ZERO,),
            ToolAdmission::Terminate(terminal(LocalLoopTerminalReason::RepeatedToolCall, None))
        );
        assert_eq!(TOOL_TIMEOUT, repeated.tool_timeout());
        assert_eq!(
            repeated
                .remaining_request_time(REQUEST_TIMEOUT - Duration::from_millis(7))
                .unwrap(),
            Duration::from_millis(7)
        );
        assert_eq!(
            repeated
                .begin_model_turn(REQUEST_TIMEOUT)
                .unwrap_err()
                .reason,
            LocalLoopTerminalReason::RequestDeadline
        );
    }

    #[test]
    fn protocol_repair_budget_resets_after_a_valid_tool_call() {
        let mut state = LocalLoopState::new(AgentToolRegistrySnapshot::local_default());
        assert!(matches!(
            state.record_protocol_error(None, LocalDecisionErrorKind::Malformed, Duration::ZERO),
            ToolAdmission::Replan(ToolObservation {
                status: ToolObservationStatus::Malformed,
                ..
            })
        ));
        assert_eq!(
            state.admit_tool_call(&call(AgentToolId::ReadFile, "src/main.rs"), Duration::ZERO,),
            ToolAdmission::Execute
        );
        assert!(matches!(
            state.record_protocol_error(
                Some(AgentToolId::WebSearch),
                LocalDecisionErrorKind::UnknownOrStale,
                Duration::ZERO,
            ),
            ToolAdmission::Replan(ToolObservation {
                status: ToolObservationStatus::UnknownOrStale,
                ..
            })
        ));
        assert!(matches!(
            state.record_protocol_error(None, LocalDecisionErrorKind::Malformed, Duration::ZERO),
            ToolAdmission::Terminate(LocalLoopTerminal {
                reason: LocalLoopTerminalReason::ProtocolError,
                ..
            })
        ));

        let mut stale = LocalLoopState::new(AgentToolRegistrySnapshot::local_default());
        assert!(matches!(
            stale.admit_tool_call(&call(AgentToolId::WebSearch, "Rust"), Duration::ZERO,),
            ToolAdmission::Replan(ToolObservation {
                status: ToolObservationStatus::UnknownOrStale,
                ..
            })
        ));
    }

    #[test]
    fn observation_statuses_follow_the_normative_transition_table() {
        for status in [
            ToolObservationStatus::Ok,
            ToolObservationStatus::NotFound,
            ToolObservationStatus::Denied,
            ToolObservationStatus::ToolError,
            ToolObservationStatus::Truncated,
        ] {
            let mut state = LocalLoopState::new(AgentToolRegistrySnapshot::local_default());
            assert!(matches!(
                state.record_observation(observation(status, "result"), Duration::ZERO),
                ObservationTransition::Replan(_)
            ));
        }

        for (status, reason) in [
            (
                ToolObservationStatus::Cancelled,
                LocalLoopTerminalReason::Cancelled,
            ),
            (
                ToolObservationStatus::Timeout,
                LocalLoopTerminalReason::ToolTimeout,
            ),
        ] {
            let mut state = LocalLoopState::new(AgentToolRegistrySnapshot::local_default());
            assert!(matches!(
                state.record_observation(observation(status, "stopped"), Duration::ZERO),
                ObservationTransition::Terminate(LocalLoopTerminal { reason: actual, .. })
                    if actual == reason
            ));
        }

        let state = LocalLoopState::new(AgentToolRegistrySnapshot::local_default());
        assert_eq!(
            state.terminal_decision(false, Duration::ZERO).reason,
            LocalLoopTerminalReason::Answer
        );
        assert_eq!(
            state.terminal_decision(true, Duration::ZERO).reason,
            LocalLoopTerminalReason::ProposeMutation
        );
    }

    #[test]
    fn bounds_each_observation_and_cumulative_output_on_utf8_boundaries() {
        let mut state = LocalLoopState::new(AgentToolRegistrySnapshot::local_default());
        let oversized = "가".repeat(MAX_OBSERVATION_BYTES);
        let first = state.record_observation(
            observation(ToolObservationStatus::Ok, oversized),
            Duration::ZERO,
        );
        let ObservationTransition::Replan(first) = first else {
            panic!("first bounded observation must replan");
        };
        assert!(first.truncation.truncated);
        assert!(first.content.len() <= MAX_OBSERVATION_BYTES);
        assert!(first.content.is_char_boundary(first.content.len()));

        for _ in 0..2 {
            assert!(matches!(
                state.record_observation(
                    observation(ToolObservationStatus::Ok, "x".repeat(MAX_OBSERVATION_BYTES)),
                    Duration::ZERO,
                ),
                ObservationTransition::Replan(_)
            ));
        }
        assert!(matches!(
            state.record_observation(
                observation(
                    ToolObservationStatus::Ok,
                    "x".repeat(MAX_OBSERVATION_BYTES * 2)
                ),
                Duration::ZERO,
            ),
            ObservationTransition::Terminate(LocalLoopTerminal {
                reason: LocalLoopTerminalReason::ObservationBudget,
                ..
            })
        ));

        let mut exact = LocalLoopState::new(AgentToolRegistrySnapshot::local_default());
        for _ in 0..4 {
            assert!(matches!(
                exact.record_observation(
                    observation(ToolObservationStatus::Ok, "x".repeat(MAX_OBSERVATION_BYTES)),
                    Duration::ZERO,
                ),
                ObservationTransition::Replan(_)
            ));
        }
        assert!(matches!(
            exact.record_observation(observation(ToolObservationStatus::Ok, "x"), Duration::ZERO,),
            ObservationTransition::Terminate(LocalLoopTerminal {
                reason: LocalLoopTerminalReason::ObservationBudget,
                ..
            })
        ));
    }
}
