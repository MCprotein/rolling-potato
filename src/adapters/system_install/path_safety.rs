//! Canonical path comparison and normalization for install safety checks.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::foundation::error::AppError;

pub(super) fn equivalent_path(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

pub(super) fn paths_resolve_equal(left: &Path, right: &Path) -> bool {
    equivalent_path(left, right) || left == right
}

pub(super) fn resolve_existing_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub(super) fn absolute_path(path: &Path) -> Result<PathBuf, AppError> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    env::current_dir()
        .map(|current| current.join(path))
        .map_err(|err| AppError::runtime(format!("현재 directory 확인 실패: {err}")))
}
