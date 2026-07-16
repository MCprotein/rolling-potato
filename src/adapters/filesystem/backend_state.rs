use std::fs;
use std::path::PathBuf;

use crate::adapters::filesystem::layout;
use crate::foundation::error::AppError;
use crate::runtime_core::inference::backend::lifecycle::{
    parse_sidecar_record, render_sidecar_record, BackendSidecarRecord,
};

pub(crate) fn sidecar_record_path() -> PathBuf {
    layout::state_dir().join("backend-llama.cpp-sidecar.txt")
}

pub(crate) fn write_sidecar_record(record: &BackendSidecarRecord) -> Result<(), AppError> {
    let path = sidecar_record_path();
    let parent = path.parent().ok_or_else(|| {
        AppError::runtime(format!(
            "backend sidecar record parent path를 계산하지 못했습니다: {}",
            path.display()
        ))
    })?;
    fs::create_dir_all(parent).map_err(|err| {
        AppError::runtime(format!(
            "backend sidecar record directory를 만들지 못했습니다: {} ({err})",
            parent.display()
        ))
    })?;

    fs::write(&path, render_sidecar_record(record)).map_err(|err| {
        AppError::runtime(format!(
            "backend sidecar record를 쓰지 못했습니다: {} ({err})",
            path.display()
        ))
    })
}

pub(crate) fn read_sidecar_record() -> Result<Option<BackendSidecarRecord>, AppError> {
    let path = sidecar_record_path();
    if !path.exists() {
        return Ok(None);
    }
    let contents = fs::read_to_string(&path).map_err(|err| {
        AppError::runtime(format!(
            "backend sidecar record를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    parse_sidecar_record(&contents).map(Some).ok_or_else(|| {
        AppError::blocked(format!(
            "backend sidecar record 형식이 유효하지 않습니다: {}",
            path.display()
        ))
    })
}

pub(crate) fn remove_sidecar_record() -> Result<(), AppError> {
    let path = sidecar_record_path();
    if path.exists() {
        fs::remove_file(&path).map_err(|err| {
            AppError::runtime(format!("file 삭제 실패: {} ({err})", path.display()))
        })?;
    }
    Ok(())
}
