use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use crate::adapters::filesystem::layout;
use crate::foundation::error::AppError;
use crate::runtime_core::inference::backend::lifecycle::{
    parse_generation_record, parse_sidecar_record, record_value, render_generation_record,
    render_sidecar_record, BackendGenerationRecord, BackendGenerationTerminalRecord,
    BackendSidecarRecord,
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

pub(crate) fn generation_record_path() -> PathBuf {
    layout::state_dir().join("backend-active-generation.txt")
}

pub(crate) fn generation_lock_path() -> PathBuf {
    layout::state_dir().join("backend-active-generation.lock")
}

pub(crate) fn generation_cancel_path() -> PathBuf {
    layout::state_dir().join("backend-active-generation.cancel")
}

pub(crate) fn generation_terminal_path(generation_id: &str) -> PathBuf {
    layout::state_dir()
        .join("backend-generation-terminals")
        .join(format!("{generation_id}.txt"))
}

pub(crate) fn acquire_generation_lock(record: &BackendGenerationRecord) -> Result<(), AppError> {
    let path = generation_lock_path();
    let parent = path.parent().ok_or_else(|| {
        AppError::runtime(format!(
            "backend generation lock parent path를 계산하지 못했습니다: {}",
            path.display()
        ))
    })?;
    fs::create_dir_all(parent).map_err(|err| {
        AppError::runtime(format!(
            "backend generation lock directory를 만들지 못했습니다: {} ({err})",
            parent.display()
        ))
    })?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|err| {
            AppError::blocked(format!(
                "backend generation lease를 획득하지 못했습니다: {} ({err})",
                path.display()
            ))
        })?;
    if let Err(err) = file
        .write_all(render_generation_record(record).as_bytes())
        .and_then(|_| file.sync_all())
    {
        drop(file);
        let _ = fs::remove_file(&path);
        return Err(AppError::runtime(format!(
            "backend generation lease를 기록하지 못했습니다: {} ({err})",
            path.display()
        )));
    }
    Ok(())
}

pub(crate) fn read_generation_record() -> Result<Option<BackendGenerationRecord>, AppError> {
    read_generation_record_at(
        generation_record_path(),
        "backend generation record를 읽지 못했습니다",
        "backend generation record 형식이 유효하지 않습니다",
    )
}

pub(crate) fn read_generation_lock_record() -> Result<Option<BackendGenerationRecord>, AppError> {
    read_generation_record_at(
        generation_lock_path(),
        "backend generation lock을 읽지 못했습니다",
        "backend generation lock 형식이 유효하지 않습니다",
    )
}

fn read_generation_record_at(
    path: PathBuf,
    read_failure: &str,
    invalid_message: &str,
) -> Result<Option<BackendGenerationRecord>, AppError> {
    if !path.exists() {
        return Ok(None);
    }
    let contents = fs::read_to_string(&path)
        .map_err(|err| AppError::runtime(format!("{read_failure}: {} ({err})", path.display())))?;
    parse_generation_record(&contents)
        .map(Some)
        .ok_or_else(|| AppError::blocked(format!("{invalid_message}: {}", path.display())))
}

pub(crate) fn read_cancel_generation_id() -> Result<Option<String>, AppError> {
    let path = generation_cancel_path();
    if !path.exists() {
        return Ok(None);
    }
    let contents = fs::read_to_string(&path).map_err(|err| {
        AppError::runtime(format!(
            "backend generation cancel marker를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    Ok(record_value(&contents, "generation_id").map(str::to_string))
}

pub(crate) fn read_generation_terminal_record(
    generation_id: &str,
) -> Result<Option<BackendGenerationTerminalRecord>, AppError> {
    let path = generation_terminal_path(generation_id);
    if !path.exists() {
        return Ok(None);
    }
    let contents = fs::read_to_string(&path).map_err(|err| {
        AppError::runtime(format!(
            "backend generation terminal record를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    let record = BackendGenerationTerminalRecord {
        generation_id: record_value(&contents, "generation_id")
            .ok_or_else(|| AppError::blocked("generation terminal id가 없습니다."))?
            .to_string(),
        outcome: record_value(&contents, "outcome")
            .ok_or_else(|| AppError::blocked("generation terminal outcome이 없습니다."))?
            .to_string(),
        lifecycle_event: record_value(&contents, "lifecycle_event")
            .ok_or_else(|| AppError::blocked("generation terminal lifecycle event가 없습니다."))?
            .to_string(),
        recorded_at_ms: record_value(&contents, "recorded_at_ms")
            .and_then(|value| value.parse().ok())
            .ok_or_else(|| {
                AppError::blocked("generation terminal timestamp가 유효하지 않습니다.")
            })?,
    };
    if record.generation_id != generation_id {
        return Err(AppError::blocked(
            "generation terminal record id가 요청과 일치하지 않습니다.",
        ));
    }
    Ok(Some(record))
}

pub(crate) fn remove_generation_state_if_owned(generation_id: &str) -> Result<(), AppError> {
    remove_if_owned(&generation_record_path(), generation_id)?;
    remove_if_owned(&generation_cancel_path(), generation_id)?;
    remove_generation_lock_if_owned(generation_id)
}

pub(crate) fn remove_generation_lock_if_owned(generation_id: &str) -> Result<(), AppError> {
    remove_if_owned(&generation_lock_path(), generation_id)
}

fn remove_if_owned(path: &PathBuf, generation_id: &str) -> Result<(), AppError> {
    let owned = fs::read_to_string(path)
        .ok()
        .and_then(|contents| {
            record_value(&contents, "generation_id").map(|value| value == generation_id)
        })
        .unwrap_or(false);
    if owned {
        remove_file_if_exists(path)?;
    }
    Ok(())
}

pub(crate) fn remove_generation_terminal_record(generation_id: &str) -> Result<(), AppError> {
    remove_file_if_exists(&generation_terminal_path(generation_id))
}

pub(crate) fn prune_generation_terminal_records(now_ms: u128, retention_ms: u128) {
    let directory = layout::state_dir().join("backend-generation-terminals");
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let old = fs::read_to_string(&path)
            .ok()
            .and_then(|contents| {
                record_value(&contents, "recorded_at_ms")?
                    .parse::<u128>()
                    .ok()
            })
            .map(|recorded| now_ms.saturating_sub(recorded) > retention_ms)
            .unwrap_or(false);
        if old {
            let _ = fs::remove_file(path);
        }
    }
}

fn remove_file_if_exists(path: &PathBuf) -> Result<(), AppError> {
    if path.exists() {
        fs::remove_file(path).map_err(|err| {
            AppError::runtime(format!("file 삭제 실패: {} ({err})", path.display()))
        })?;
    }
    Ok(())
}
