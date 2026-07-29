//! Deterministic, side-effect-free hook policy facade.

mod codec;
mod policy;
mod registry;
mod report;
mod types;

pub(crate) use policy::{dispatch, status_label};
pub(crate) use registry::HOOK_POINTS;
pub(crate) use report::{list_report, validate_result_report};
pub(crate) use types::{HookDispatch, HookInput, HookLayer, HookRule, HookStatus};

#[cfg(test)]
mod tests;
