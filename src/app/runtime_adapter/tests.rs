use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;
use crate::app::tui_adapter::canonical_read_page as read_tui_page;
use crate::app::workflow_adapter::transcript;
use crate::surfaces::tui::outcome::{
    exact_tui_outcome, verification_credential_issued, TuiEffect, TuiNextAction, TuiOutcomeContext,
    TuiOutcomeStatus,
};
use crate::surfaces::tui::runtime_bridge::{
    OneShotSecret, TuiFreshness, TuiReadBudget, TuiReadContinuation, TuiReadRequest,
};

include!("tests/support.rs");
include!("tests/read_views.rs");
include!("tests/outcome_matrix.rs");
include!("tests/outcome_contract.rs");
include!("tests/reports.rs");
