//! Small dependency-free primitives shared by owned runtime boundaries.
//!
//! This root is not a general utility bucket. A primitive belongs here only
//! when its invariant is independent of a runtime capability.

mod integrity;
mod serialization;
