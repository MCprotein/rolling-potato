use super::*;

pub(super) const MAX_ARCHIVE_BYTES: u64 = 128 * 1024 * 1024;
pub(super) fn copy_with_limit(
    reader: &mut impl Read,
    writer: &mut impl Write,
    limit: u64,
) -> Result<u64, AppError> {
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|err| AppError::runtime(format!("release archive stream 읽기 실패: {err}")))?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if total > limit {
            return Err(AppError::blocked(format!(
                "release archive가 허용 크기를 초과했습니다: {limit} bytes"
            )));
        }
        writer
            .write_all(&buffer[..read])
            .map_err(|err| AppError::runtime(format!("release archive 기록 실패: {err}")))?;
    }
    Ok(total)
}

pub(super) fn extract_binary(
    plan: &ReleaseAssetPlan,
    archive_path: &Path,
    target: &Path,
) -> Result<(), AppError> {
    match plan.archive_kind {
        ReleaseArchiveKind::TarGz => extract_tar_binary(plan, archive_path, target),
        ReleaseArchiveKind::Zip => extract_zip_binary(plan, archive_path, target),
    }
}

fn extract_tar_binary(
    plan: &ReleaseAssetPlan,
    archive_path: &Path,
    target: &Path,
) -> Result<(), AppError> {
    let file = File::open(archive_path).map_err(|err| {
        AppError::runtime(format!(
            "release tar.gz 열기 실패: {} ({err})",
            archive_path.display()
        ))
    })?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    let expected_root = plan.archive_name.trim_end_matches(".tar.gz");
    let mut found = false;
    for entry in archive
        .entries()
        .map_err(|err| AppError::runtime(format!("release tar.gz metadata 읽기 실패: {err}")))?
    {
        let mut entry = entry
            .map_err(|err| AppError::runtime(format!("release tar.gz entry 읽기 실패: {err}")))?;
        let path = entry
            .path()
            .map_err(|err| AppError::blocked(format!("release tar.gz path 오류: {err}")))?;
        let components = safe_components(&path)?;
        if components == [expected_root, plan.binary_name.as_str()] {
            if found || !entry.header().entry_type().is_file() {
                return Err(AppError::blocked(
                    "release tar.gz binary entry가 중복되었거나 regular file이 아닙니다.",
                ));
            }
            write_extracted_binary(&mut entry, target)?;
            found = true;
        }
    }
    if !found {
        return Err(AppError::blocked(
            "release tar.gz에서 정확한 rpotato binary를 찾지 못했습니다.",
        ));
    }
    Ok(())
}

fn extract_zip_binary(
    plan: &ReleaseAssetPlan,
    archive_path: &Path,
    target: &Path,
) -> Result<(), AppError> {
    let file = File::open(archive_path).map_err(|err| {
        AppError::runtime(format!(
            "release zip 열기 실패: {} ({err})",
            archive_path.display()
        ))
    })?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|err| AppError::runtime(format!("release zip metadata 읽기 실패: {err}")))?;
    let mut match_index = None;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|err| AppError::runtime(format!("release zip entry 읽기 실패: {err}")))?;
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| AppError::blocked("release zip에 안전하지 않은 path가 있습니다."))?;
        let components = safe_components(&enclosed)?;
        if components == [plan.binary_name.as_str()] {
            if match_index.is_some() || entry.is_dir() {
                return Err(AppError::blocked(
                    "release zip binary entry가 중복되었거나 regular file이 아닙니다.",
                ));
            }
            match_index = Some(index);
        }
    }
    let index = match_index.ok_or_else(|| {
        AppError::blocked("release zip에서 정확한 rpotato.exe binary를 찾지 못했습니다.")
    })?;
    let mut entry = archive
        .by_index(index)
        .map_err(|err| AppError::runtime(format!("release zip binary 읽기 실패: {err}")))?;
    write_extracted_binary(&mut entry, target)
}

pub(super) fn safe_components(path: &Path) -> Result<Vec<&str>, AppError> {
    path.components()
        .map(|component| match component {
            Component::Normal(value) => value
                .to_str()
                .ok_or_else(|| AppError::blocked("release archive path가 UTF-8이 아닙니다.")),
            _ => Err(AppError::blocked(
                "release archive에 안전하지 않은 path component가 있습니다.",
            )),
        })
        .collect()
}

fn write_extracted_binary(reader: &mut impl Read, target: &Path) -> Result<(), AppError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o755);
    }
    let mut output = options.open(target).map_err(|err| {
        AppError::runtime(format!(
            "update binary staging 생성 실패: {} ({err})",
            target.display()
        ))
    })?;
    let copied = copy_with_limit(reader, &mut output, MAX_ARCHIVE_BYTES);
    if let Err(error) = copied {
        drop(output);
        let _ = fs::remove_file(target);
        return Err(error);
    }
    output
        .flush()
        .and_then(|_| output.sync_all())
        .map_err(|err| AppError::runtime(format!("update binary staging sync 실패: {err}")))
}
