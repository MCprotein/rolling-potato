//! Official GitHub Release discovery, caching, download, and payload extraction.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use crate::adapters::filesystem::{atomic_write, layout};
use crate::foundation::error::AppError;
use crate::foundation::{integrity, serialization};
use crate::runtime_core::update::{
    parse_checksum_line, parse_release_tag, release_asset_plan, ReleaseArchiveKind,
    ReleaseAssetPlan,
};

mod archive;
mod discovery;
mod download;

pub(crate) use discovery::{
    latest_release_with_cache_fallback, refresh_latest_release, LatestRelease,
};
pub(crate) use download::download_release_binary;

#[cfg(test)]
#[path = "github_release/tests.rs"]
mod tests;
