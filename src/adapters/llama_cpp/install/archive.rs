use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;

use crate::foundation::error::AppError;
use crate::foundation::integrity as checksum;

use super::{ArchiveDownloadStatus, BackendReleaseArtifact};

pub(crate) fn download_archive(
    artifact: &BackendReleaseArtifact,
    archive_path: &Path,
) -> Result<ArchiveDownloadStatus, AppError> {
    if archive_path.exists() && !archive_path.is_file() {
        return Err(AppError::blocked(format!(
            "backend archive cache path가 file이 아닙니다: {}",
            archive_path.display()
        )));
    }
    if archive_path.is_file() && verify_archive_file(artifact, archive_path).is_ok() {
        return Ok(ArchiveDownloadStatus::CacheHit);
    }

    let parent = archive_path.parent().ok_or_else(|| {
        AppError::runtime(format!(
            "backend archive parent path를 계산하지 못했습니다: {}",
            archive_path.display()
        ))
    })?;
    fs::create_dir_all(parent).map_err(|err| {
        AppError::runtime(format!(
            "backend archive download directory를 만들지 못했습니다: {} ({err})",
            parent.display()
        ))
    })?;

    let part_path = archive_path.with_file_name(format!("{}.part", artifact.archive_name));
    remove_file_if_exists(&part_path)?;

    let response = ureq::get(artifact.archive_url)
        .header("User-Agent", concat!("rpotato/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|err| {
            AppError::runtime(format!(
                "backend archive 다운로드 실패\n- url: {}\n- error: {err}",
                artifact.archive_url
            ))
        })?;
    let (_, body) = response.into_parts();
    let mut reader = body.into_reader();
    let mut file = File::create(&part_path).map_err(|err| {
        AppError::runtime(format!(
            "backend archive partial file을 만들지 못했습니다: {} ({err})",
            part_path.display()
        ))
    })?;

    let copied_bytes =
        match copy_reader_with_limit(&mut reader, &mut file, artifact.archive_size_bytes) {
            Ok(copied_bytes) => copied_bytes,
            Err(err) => {
                drop(file);
                let _ = fs::remove_file(&part_path);
                return Err(err);
            }
        };
    file.sync_all().map_err(|err| {
        AppError::runtime(format!(
            "backend archive partial file sync 실패: {} ({err})",
            part_path.display()
        ))
    })?;
    drop(file);

    if copied_bytes != artifact.archive_size_bytes {
        remove_file_if_exists(&part_path)?;
        return Err(AppError::blocked(format!(
            "backend archive size 검증 실패\n- expected: {}\n- actual: {}\n- path: {}",
            artifact.archive_size_bytes,
            copied_bytes,
            part_path.display()
        )));
    }
    if let Err(err) = verify_archive_file(artifact, &part_path) {
        remove_file_if_exists(&part_path)?;
        return Err(err);
    }

    remove_file_if_exists(archive_path)?;
    fs::rename(&part_path, archive_path).map_err(|err| {
        AppError::runtime(format!(
            "backend archive cache 배치 실패: {} -> {} ({err})",
            part_path.display(),
            archive_path.display()
        ))
    })?;

    Ok(ArchiveDownloadStatus::Downloaded)
}

pub(crate) fn verify_archive_file(
    artifact: &BackendReleaseArtifact,
    archive_path: &Path,
) -> Result<(), AppError> {
    let metadata = archive_path.metadata().map_err(|err| {
        AppError::runtime(format!(
            "backend archive metadata를 읽지 못했습니다: {} ({err})",
            archive_path.display()
        ))
    })?;
    if !metadata.is_file() {
        return Err(AppError::blocked(format!(
            "backend archive path가 file이 아닙니다: {}",
            archive_path.display()
        )));
    }
    if metadata.len() != artifact.archive_size_bytes {
        return Err(AppError::blocked(format!(
            "backend archive size 검증 실패\n- expected: {}\n- actual: {}\n- path: {}",
            artifact.archive_size_bytes,
            metadata.len(),
            archive_path.display()
        )));
    }

    let actual_sha256 = checksum::sha256_file(archive_path)?;
    if !actual_sha256.eq_ignore_ascii_case(artifact.archive_sha256) {
        return Err(AppError::blocked(format!(
            "backend archive SHA-256 검증 실패\n- expected: {}\n- actual: {}\n- path: {}",
            artifact.archive_sha256,
            actual_sha256,
            archive_path.display()
        )));
    }

    Ok(())
}

fn copy_reader_with_limit<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    expected_bytes: u64,
) -> Result<u64, AppError> {
    const DOWNLOAD_BUFFER_BYTES: usize = 64 * 1024;

    let mut copied_bytes = 0_u64;
    let mut buffer = [0_u8; DOWNLOAD_BUFFER_BYTES];
    loop {
        let bytes_read = reader.read(&mut buffer).map_err(|err| {
            AppError::runtime(format!("backend archive download stream read 실패: {err}"))
        })?;
        if bytes_read == 0 {
            break;
        }
        copied_bytes += bytes_read as u64;
        if copied_bytes > expected_bytes {
            return Err(AppError::blocked(format!(
                "backend archive size limit 초과\n- expected: {}\n- actual-at-least: {}",
                expected_bytes, copied_bytes
            )));
        }
        writer.write_all(&buffer[..bytes_read]).map_err(|err| {
            AppError::runtime(format!("backend archive partial file write 실패: {err}"))
        })?;
    }
    writer
        .flush()
        .map_err(|err| AppError::runtime(format!("backend archive flush 실패: {err}")))?;
    Ok(copied_bytes)
}

fn remove_file_if_exists(path: &Path) -> Result<(), AppError> {
    if path.exists() {
        fs::remove_file(path).map_err(|err| {
            AppError::runtime(format!("file 삭제 실패: {} ({err})", path.display()))
        })?;
    }
    Ok(())
}
