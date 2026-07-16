//! Small dependency-free primitives shared by owned runtime boundaries.
//!
//! This root is not a general utility bucket. A primitive belongs here only
//! when its invariant is independent of a runtime capability.

pub(crate) mod error;
pub(crate) mod integrity;
pub(crate) mod serialization;
