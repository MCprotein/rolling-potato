//! Surface-neutral runtime capabilities.
//!
//! Each child owns one cohesive runtime capability. Cross-capability access is
//! admitted through the owning capability boundary, not through concrete files.

pub(crate) mod collaboration;
pub(crate) mod extensions;
pub(crate) mod inference;
pub(crate) mod knowledge;
pub(crate) mod observability;
pub(crate) mod patch;
pub(crate) mod policy;
pub(crate) mod reporting;
pub(crate) mod workflow;
