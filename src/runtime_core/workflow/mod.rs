//! Durable workflow, transition, storage-compatibility, and recovery ownership.
//!
//! Cross-store ordering remains in legacy modules until the transaction-
//! coordinator migration release. Storage DTOs and canonical codecs live in
//! the compatibility boundary so their bytes can remain stable while domain
//! views evolve independently.

pub(crate) mod application;
pub(crate) mod domain;
pub(crate) mod storage_compat;
