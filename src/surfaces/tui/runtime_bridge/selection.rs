#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelectionLease {
    pub(crate) project_id: String,
    pub(crate) session_id: String,
    pub(crate) selected_object_id: String,
    pub(crate) current_revision: u64,
    pub(crate) current_hash: String,
    pub(crate) active_session_id: String,
    pub(crate) active_workflow: Option<ObservedWorkflow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ObservedWorkflow {
    pub(crate) workflow_id: String,
    pub(crate) revision: u64,
    pub(crate) hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelectionObservation {
    pub(crate) project_id: String,
    pub(crate) session_id: String,
    pub(crate) current_revision: u64,
    pub(crate) current_hash: String,
    pub(crate) active_workflow: Option<ObservedWorkflow>,
}

impl SelectionObservation {
    pub(crate) fn lease_for(&self, selected_object_id: &str) -> SelectionLease {
        SelectionLease {
            project_id: self.project_id.clone(),
            session_id: self.session_id.clone(),
            selected_object_id: selected_object_id.to_string(),
            current_revision: self.current_revision,
            current_hash: self.current_hash.clone(),
            active_session_id: self.session_id.clone(),
            active_workflow: self.active_workflow.clone(),
        }
    }
}

pub(crate) fn lease_matches_active_workflow(
    lease: &SelectionLease,
    workflow_id: &str,
    observation: &SelectionObservation,
) -> bool {
    lease.selected_object_id == workflow_id
        && observation
            .active_workflow
            .as_ref()
            .is_some_and(|workflow| workflow.workflow_id == workflow_id)
        && observation.lease_for(workflow_id) == *lease
}

pub(crate) fn lease_matches_terminal_selection(
    lease: &SelectionLease,
    workflow_id: &str,
    observation: &SelectionObservation,
) -> bool {
    lease.selected_object_id == workflow_id && observation.lease_for(workflow_id) == *lease
}
