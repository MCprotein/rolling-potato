//! Local attachment capture and text-request composition for the interactive TUI.

use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

use crate::adapters::filesystem::layout as paths;
use crate::foundation::error::AppError;
use crate::foundation::integrity;
use crate::runtime_core::inference::backend::{
    BackendChatImage, BackendChatInput, ResponseLanguage,
};
use crate::surfaces::tui::runtime_bridge::{TuiAttachment, TuiAttachmentKind};

const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;
const MAX_TEXT_BYTES: u64 = 256 * 1024;
const MAX_ATTACHMENTS: usize = 8;
const RESPONSE_RESERVE_TOKENS: usize = 512;
const RUNTIME_PROMPT_RESERVE_TOKENS: usize = 512;

mod capture;
mod compose;
mod format;
mod path;

pub(super) use capture::capture;
pub(super) use compose::compose_request;
use format::{attachment_kind, validate_content};
use path::{normalized_source_path, safe_leaf};

#[cfg(test)]
#[path = "attachment/tests.rs"]
mod tests;
