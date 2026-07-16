#![allow(dead_code)]

#[path = "../src/foundation/error.rs"]
pub(crate) mod source_error;
#[path = "../src/foundation/serialization.rs"]
pub(crate) mod source_serialization;

mod foundation {
    pub(crate) use crate::source_error as error;
    pub(crate) use crate::source_serialization as serialization;
}

#[path = "../src/runtime_core/workflow/storage_compat/ledger.rs"]
mod ledger;
#[path = "../src/runtime_core/workflow/storage_compat/record.rs"]
mod record;
#[path = "../src/runtime_core/workflow/storage_compat/transcript.rs"]
mod transcript;

#[path = "workflow/storage_compat.rs"]
mod storage_compat;
