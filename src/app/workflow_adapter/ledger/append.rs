//! Durable JSONL append primitive for the runtime ledger adapter.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use crate::foundation::error::AppError;

pub(super) fn append_line(path: &Path, line: &str) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            AppError::runtime(format!(
                "디렉터리를 만들지 못했습니다: {} ({err})",
                parent.display()
            ))
        })?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| {
            AppError::runtime(format!(
                "파일을 열지 못했습니다: {} ({err})",
                path.display()
            ))
        })?;

    writeln!(file, "{line}").map_err(|err| {
        AppError::runtime(format!(
            "파일에 기록하지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    file.sync_all()
        .map_err(|err| AppError::runtime(format!("ledger sync 실패: {} ({err})", path.display())))
}
