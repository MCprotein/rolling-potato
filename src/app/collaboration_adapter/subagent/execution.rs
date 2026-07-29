use std::time::Duration;

use crate::adapters::filesystem::{layout as paths, lease};
use crate::app::workflow_adapter::{ledger, state};
use crate::foundation::error::AppError;
use crate::runtime_core::collaboration::subagent::{SubagentRecordV1, SubagentStatus};

use super::{
    append_lifecycle_event, checkpoint_record, display_list, load_record, records_for_parent,
    AdmittedLaunch, AdmittedTeamMember,
};

mod completion;
mod dispatch;
mod member;
mod parent_merge;

pub(super) use completion::terminalize_locked;
use completion::{complete_generation, terminalize_running_error};
pub(super) use dispatch::dispatch_admitted;
#[cfg(test)]
pub(super) use dispatch::prepare_running;
use dispatch::{execute_prepared_launch, prepare_admitted_launch};
pub(crate) use member::{
    execute_admitted_team_member_with, execute_prepared_team_member_with, prepare_team_members,
    terminalize_interrupted_team_members,
};
pub(super) use parent_merge::{merge_completed_result, recover_completed_parent_merges};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkerGeneration {
    pub backend_event_id: String,
    pub effective_max_tokens: u32,
    pub response: String,
}

pub(crate) struct PreparedTeamMember {
    pub lane: u32,
    pub member_id: String,
    prepared: PreparedLaunch,
}

impl PreparedTeamMember {
    pub fn subagent_id(&self) -> &str {
        &self.prepared.running.subagent_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompletedTeamMember {
    pub lane: u32,
    pub member_id: String,
    pub record: SubagentRecordV1,
    pub summary: String,
}

#[derive(Debug)]
pub(super) struct CompletedLaunch {
    pub(super) record: SubagentRecordV1,
    pub(super) context: crate::app::context_adapter::ContextPack,
    pub(super) summary: String,
}

struct PreparedLaunch {
    _execution_lease: lease::RecoverableLease,
    running: SubagentRecordV1,
    context: crate::app::context_adapter::ContextPack,
    task: String,
}
