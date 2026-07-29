use super::*;

mod codec;
mod file_io;
mod lease_view;
mod promotion;
mod status;

pub(super) use codec::{
    parse_current_state, parse_current_state_v2, render_current_state_v2,
    render_current_state_v2_payload,
};
pub(super) use file_io::read_open_file_bounded;
pub(crate) use file_io::read_regular_file_bounded;
pub(crate) use lease_view::{
    current_state_lease_view, current_state_lease_view_under_transition,
    tui_entry_initialization_required, tui_lease_matches_terminal_selection_under_transition,
    tui_lease_matches_workflow_under_transition, tui_state_snapshot_read_only,
    validated_identity_from_current_state,
};
pub(super) use lease_view::{
    migrate_matching_legacy_current_state, synchronize_current_state_ledger, tui_detail_value,
};
pub(super) use promotion::promote_current_state_v1;
#[cfg(test)]
pub(super) use status::classify_current_state;
pub(super) use status::{current_state_status, read_current_state_summary, CurrentStateStatus};
