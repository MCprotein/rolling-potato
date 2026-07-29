pub(crate) use crate::runtime_core::collaboration::subagent::*;

mod admission;
mod execution;
mod lifecycle;
mod persistence;
mod reporting;

#[cfg(test)]
use crate::adapters::filesystem::layout as paths;
#[cfg(test)]
use crate::app::workflow_adapter::{ledger, state};
#[cfg(test)]
use crate::foundation::error::AppError;
#[cfg(test)]
use std::fs;

#[cfg(test)]
use admission::admit_launch;
pub(crate) use admission::{
    admit_team_members, resume_admitted_team_member, AdmittedLaunch, AdmittedTeamMember,
    TeamMemberLaunch,
};
#[cfg(test)]
use execution::dispatch_admitted;
pub(crate) use execution::{
    execute_admitted_team_member_with, execute_prepared_team_member_with, prepare_team_members,
    terminalize_interrupted_team_members, CompletedTeamMember, WorkerGeneration,
};
#[cfg(test)]
use execution::{merge_completed_result, prepare_running};
use lifecycle::append_lifecycle_event;
pub use lifecycle::cancel_report;
#[cfg(test)]
use persistence::create_record;
pub(crate) use persistence::records_for_parent;
pub use persistence::{checkpoint_record, load_record};
use reporting::display_list;
pub use reporting::{launch_report, status_report};

#[cfg(test)]
#[path = "subagent/tests.rs"]
mod tests;
