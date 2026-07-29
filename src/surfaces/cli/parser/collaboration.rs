//! Collaboration command parser facade.

use super::{parse_positive_u32, AppError, ModelTier, SubagentCommand, TeamCommand};

mod shared;
mod subagent;
mod team_admission;
mod team_dispatch;
mod team_identity;

pub(super) use subagent::parse_subagent_launch_args;
pub(super) use team_admission::parse_team_admit_args;
pub(super) use team_dispatch::{parse_team_dispatch_args, parse_team_governor_args};
pub(super) use team_identity::{
    parse_team_cancel_args, parse_team_execute_args, parse_team_plan_args,
    parse_team_reconcile_args,
};
