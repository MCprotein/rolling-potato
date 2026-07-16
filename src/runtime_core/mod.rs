//! Surface-neutral runtime capabilities.
//!
//! Each child owns one cohesive runtime capability. Cross-capability access is
//! admitted through the owning capability boundary, not through concrete files.

mod collaboration;
mod extensions;
pub(crate) mod inference;
pub(crate) mod knowledge;
pub(crate) mod observability;
mod patch;
pub(crate) mod policy;
mod reporting;
pub(crate) mod workflow;
