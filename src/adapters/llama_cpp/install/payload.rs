use std::fs::{self, File};
use std::path::{Path, PathBuf};

use crate::foundation::error::AppError;
use crate::foundation::integrity as checksum;

use super::{BackendArchiveKind, BackendReleaseArtifact, InstalledPayload};

pub(crate) fn prepare_install(
    artifact: &BackendReleaseArtifact,
    archive_path: &Path,
    managed_binary: &Path,
    staging_dir: &Path,
) -> Result<InstalledPayload, AppError> {
    remove_dir_if_exists(staging_dir)?;
    fs::create_dir_all(staging_dir).map_err(|err| {
        AppError::runtime(format!(
            "backend staging directory를 만들지 못했습니다: {} ({err})",
            staging_dir.display()
        ))
    })?;

    if let Err(err) = extract_archive(artifact, archive_path, staging_dir) {
        let _ = fs::remove_dir_all(staging_dir);
        return Err(err);
    }
    let extracted_binary = match find_extracted_binary(artifact, staging_dir) {
        Ok(path) => path,
        Err(err) => {
            let _ = fs::remove_dir_all(staging_dir);
            return Err(err);
        }
    };
    if let Err(err) = place_managed_payload(&extracted_binary, staging_dir, managed_binary) {
        let _ = fs::remove_dir_all(staging_dir);
        return Err(err);
    }
    let binary_sha256 = checksum::sha256_file(managed_binary)?;

    Ok(InstalledPayload {
        archive_path: archive_path.to_path_buf(),
        extracted_binary,
        managed_binary: managed_binary.to_path_buf(),
        binary_sha256,
    })
}

pub(crate) fn cleanup_staging(staging_dir: &Path) -> Result<(), AppError> {
    remove_dir_if_exists(staging_dir)
}

fn extract_archive(
    artifact: &BackendReleaseArtifact,
    archive_path: &Path,
    staging_dir: &Path,
) -> Result<(), AppError> {
    match artifact.archive_kind {
        BackendArchiveKind::TarGz => extract_tar_gz_archive(archive_path, staging_dir),
        BackendArchiveKind::Zip => extract_zip_archive(archive_path, staging_dir),
    }
}

fn extract_tar_gz_archive(archive_path: &Path, staging_dir: &Path) -> Result<(), AppError> {
    let file = File::open(archive_path).map_err(|err| {
        AppError::runtime(format!(
            "backend tar.gz archive를 열지 못했습니다: {} ({err})",
            archive_path.display()
        ))
    })?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    archive.unpack(staging_dir).map_err(|err| {
        AppError::runtime(format!(
            "backend tar.gz archive extraction 실패: {} -> {} ({err})",
            archive_path.display(),
            staging_dir.display()
        ))
    })
}

fn extract_zip_archive(archive_path: &Path, staging_dir: &Path) -> Result<(), AppError> {
    let file = File::open(archive_path).map_err(|err| {
        AppError::runtime(format!(
            "backend zip archive를 열지 못했습니다: {} ({err})",
            archive_path.display()
        ))
    })?;
    let mut archive = zip::ZipArchive::new(file).map_err(|err| {
        AppError::runtime(format!(
            "backend zip archive metadata를 읽지 못했습니다: {} ({err})",
            archive_path.display()
        ))
    })?;
    archive.extract(staging_dir).map_err(|err| {
        AppError::runtime(format!(
            "backend zip archive extraction 실패: {} -> {} ({err})",
            archive_path.display(),
            staging_dir.display()
        ))
    })
}

fn find_extracted_binary(
    artifact: &BackendReleaseArtifact,
    staging_dir: &Path,
) -> Result<PathBuf, AppError> {
    let hinted_path = staging_dir.join(artifact.binary_relative_path);
    if is_regular_file_no_symlink(&hinted_path) {
        return Ok(hinted_path);
    }

    let binary_name = Path::new(artifact.binary_relative_path)
        .file_name()
        .ok_or_else(|| {
            AppError::blocked(format!(
                "archive 내부 binary path가 유효하지 않습니다: {}",
                artifact.binary_relative_path
            ))
        })?;
    let mut matches = Vec::new();
    collect_binary_matches(staging_dir, binary_name, &mut matches)?;
    matches.sort();

    match matches.len() {
        0 => Err(AppError::blocked(format!(
            "backend archive에서 binary를 찾지 못했습니다\n- expected: {}\n- staging: {}",
            artifact.binary_relative_path,
            staging_dir.display()
        ))),
        1 => Ok(matches.remove(0)),
        _ => Err(AppError::blocked(format!(
            "backend archive에서 binary 후보가 여러 개입니다\n- expected: {}\n- count: {}\n- staging: {}",
            artifact.binary_relative_path,
            matches.len(),
            staging_dir.display()
        ))),
    }
}

fn collect_binary_matches(
    directory: &Path,
    binary_name: &std::ffi::OsStr,
    matches: &mut Vec<PathBuf>,
) -> Result<(), AppError> {
    for entry in fs::read_dir(directory).map_err(|err| {
        AppError::runtime(format!(
            "backend extraction directory를 읽지 못했습니다: {} ({err})",
            directory.display()
        ))
    })? {
        let entry =
            entry.map_err(|err| AppError::runtime(format!("directory entry read 실패: {err}")))?;
        let path = entry.path();
        let file_type = fs::symlink_metadata(&path)
            .map_err(|err| {
                AppError::runtime(format!(
                    "backend extracted path metadata를 읽지 못했습니다: {} ({err})",
                    path.display()
                ))
            })?
            .file_type();
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_binary_matches(&path, binary_name, matches)?;
        } else if file_type.is_file() && path.file_name() == Some(binary_name) {
            matches.push(path);
        }
    }
    Ok(())
}

fn is_regular_file_no_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_file() && !metadata.file_type().is_symlink())
        .unwrap_or(false)
}

fn place_managed_payload(
    extracted_binary: &Path,
    staging_dir: &Path,
    managed_binary: &Path,
) -> Result<(), AppError> {
    let parent = managed_binary.parent().ok_or_else(|| {
        AppError::runtime(format!(
            "managed backend binary parent path를 계산하지 못했습니다: {}",
            managed_binary.display()
        ))
    })?;
    fs::create_dir_all(parent).map_err(|err| {
        AppError::runtime(format!(
            "managed backend directory를 만들지 못했습니다: {} ({err})",
            parent.display()
        ))
    })?;

    if managed_binary.exists() && !managed_binary.is_file() {
        return Err(AppError::blocked(format!(
            "managed backend path가 file이 아닙니다: {}",
            managed_binary.display()
        )));
    }
    if parent.exists() && !parent.is_dir() {
        return Err(AppError::blocked(format!(
            "managed backend directory path가 directory가 아닙니다: {}",
            parent.display()
        )));
    }

    let file_name = managed_binary
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("llama-server");
    let next_dir = parent.with_file_name("llama.cpp.next");
    let backup_dir = parent.with_file_name("llama.cpp.previous");
    remove_dir_if_exists(&next_dir)?;
    remove_dir_if_exists(&backup_dir)?;

    let payload_root = payload_root_for(extracted_binary, staging_dir)?;
    copy_release_tree(&payload_root, &next_dir)?;
    let next_binary = next_dir.join(file_name);
    if !next_binary.is_file() {
        fs::copy(extracted_binary, &next_binary).map_err(|err| {
            AppError::runtime(format!(
                "managed backend binary copy 실패: {} -> {} ({err})",
                extracted_binary.display(),
                next_binary.display()
            ))
        })?;
    }
    set_executable_bit(&next_binary)?;

    let had_existing = parent.exists();
    if had_existing {
        fs::rename(parent, &backup_dir).map_err(|err| {
            AppError::runtime(format!(
                "기존 managed backend directory backup 실패: {} -> {} ({err})",
                parent.display(),
                backup_dir.display()
            ))
        })?;
    }

    if let Err(err) = fs::rename(&next_dir, parent) {
        if had_existing && backup_dir.is_dir() {
            let _ = fs::rename(&backup_dir, parent);
        }
        let _ = fs::remove_dir_all(&next_dir);
        return Err(AppError::runtime(format!(
            "managed backend directory 배치 실패: {} -> {} ({err})",
            next_dir.display(),
            parent.display()
        )));
    }
    remove_dir_if_exists(&backup_dir)?;
    Ok(())
}

fn payload_root_for(extracted_binary: &Path, staging_dir: &Path) -> Result<PathBuf, AppError> {
    if !extracted_binary.starts_with(staging_dir) {
        return Err(AppError::runtime(format!(
            "extracted backend binary relative path 계산 실패: {} under {}",
            extracted_binary.display(),
            staging_dir.display()
        )));
    }
    extracted_binary
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            AppError::runtime(format!(
                "extracted backend binary parent path를 계산하지 못했습니다: {}",
                extracted_binary.display()
            ))
        })
}

fn copy_release_tree(source: &Path, destination: &Path) -> Result<(), AppError> {
    fs::create_dir_all(destination).map_err(|err| {
        AppError::runtime(format!(
            "managed backend payload directory를 만들지 못했습니다: {} ({err})",
            destination.display()
        ))
    })?;
    for entry in fs::read_dir(source).map_err(|err| {
        AppError::runtime(format!(
            "backend payload source directory를 읽지 못했습니다: {} ({err})",
            source.display()
        ))
    })? {
        let entry = entry
            .map_err(|err| AppError::runtime(format!("backend payload entry read 실패: {err}")))?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = fs::symlink_metadata(&source_path)
            .map_err(|err| {
                AppError::runtime(format!(
                    "backend payload metadata를 읽지 못했습니다: {} ({err})",
                    source_path.display()
                ))
            })?
            .file_type();

        if file_type.is_dir() {
            copy_release_tree(&source_path, &destination_path)?;
        } else if file_type.is_file() || file_type.is_symlink() {
            fs::copy(&source_path, &destination_path).map_err(|err| {
                AppError::runtime(format!(
                    "backend payload file copy 실패: {} -> {} ({err})",
                    source_path.display(),
                    destination_path.display()
                ))
            })?;
        }
    }
    Ok(())
}

#[cfg(unix)]
pub(crate) fn set_executable_bit(path: &Path) -> Result<(), AppError> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = path
        .metadata()
        .map_err(|err| {
            AppError::runtime(format!(
                "managed backend binary metadata를 읽지 못했습니다: {} ({err})",
                path.display()
            ))
        })?
        .permissions();
    permissions.set_mode(permissions.mode() | 0o755);
    fs::set_permissions(path, permissions).map_err(|err| {
        AppError::runtime(format!(
            "managed backend binary 실행 권한 설정 실패: {} ({err})",
            path.display()
        ))
    })
}

#[cfg(not(unix))]
pub(crate) fn set_executable_bit(_path: &Path) -> Result<(), AppError> {
    Ok(())
}

fn remove_dir_if_exists(path: &Path) -> Result<(), AppError> {
    if path.exists() {
        fs::remove_dir_all(path).map_err(|err| {
            AppError::runtime(format!("directory 삭제 실패: {} ({err})", path.display()))
        })?;
    }
    Ok(())
}
