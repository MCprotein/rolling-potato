use super::*;

pub(super) fn current_source_hash(relative_path: &str) -> Result<String, AppError> {
    let target = resolve_target_for("patch source hash", relative_path)?;
    fs::read(&target.absolute_path)
        .map(|bytes| sha256_bytes(&bytes))
        .map_err(|err| AppError::blocked(format!("source hash reread 실패: {err}")))
}

pub(super) fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(super) fn build_preview(
    path: &str,
    find: &str,
    replace: &str,
    workflow_id: &str,
    action_id: &str,
    verification_command: &str,
) -> Result<PatchPreview, AppError> {
    if find.is_empty() {
        return Err(AppError::usage(
            "patch preview의 --find 값은 비어 있을 수 없습니다.",
        ));
    }
    let target = resolve_target(path)?;
    let read_decision = policy::classify_path(PathMode::Read, &target.relative_path)?;
    if read_decision.decision != Decision::Allow {
        return Err(AppError::blocked(format!(
            "patch preview 차단\\n- 이유: target read policy가 allow가 아닙니다.\\n- path: {}\\n- decision: {}",
            target.relative_path,
            read_decision_label(read_decision.decision)
        )));
    }
    let write_decision = policy::classify_path(PathMode::Write, &target.relative_path)?;
    if write_decision.decision == Decision::Deny {
        return Err(AppError::blocked(format!(
            "patch preview 차단\\n- 이유: target write policy가 deny입니다.\\n- path: {}",
            target.relative_path
        )));
    }
    let metadata = fs::metadata(&target.absolute_path).map_err(|err| {
        AppError::runtime(format!(
            "patch preview 대상 파일 metadata를 읽지 못했습니다: {} ({err})",
            target.relative_path
        ))
    })?;
    if !metadata.is_file() {
        return Err(AppError::usage(format!(
            "patch preview 대상은 file이어야 합니다: {}",
            target.relative_path
        )));
    }
    if metadata.len() > proposal_domain::MAX_PATCH_FILE_BYTES {
        return Err(AppError::blocked(format!(
            "patch preview 차단\\n- 이유: 대상 파일이 preview 한도를 초과했습니다.\\n- path: {}\\n- size bytes: {}\\n- max bytes: {}",
            target.relative_path,
            metadata.len(),
            proposal_domain::MAX_PATCH_FILE_BYTES
        )));
    }
    let original = fs::read_to_string(&target.absolute_path).map_err(|err| {
        AppError::runtime(format!(
            "patch preview 대상 파일을 UTF-8 text로 읽지 못했습니다: {} ({err})",
            target.relative_path
        ))
    })?;
    let approval_token = if workflow_id.is_empty() {
        String::new()
    } else {
        issue_approval_token()?
    };

    proposal_domain::build_preview(PreviewInput {
        relative_path: &target.relative_path,
        original: &original,
        find,
        replace,
        workflow_id,
        action_id,
        verification_command,
        approval_token,
        proposal_dir: &paths::project_patch_proposals_dir(),
    })
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TargetPath {
    pub(super) absolute_path: PathBuf,
    pub(super) relative_path: String,
}

fn resolve_target(raw_path: &str) -> Result<TargetPath, AppError> {
    resolve_target_for("patch preview", raw_path)
}

pub(super) fn resolve_target_for(operation: &str, raw_path: &str) -> Result<TargetPath, AppError> {
    if raw_path.trim().is_empty() {
        return Err(AppError::usage(format!(
            "{operation}는 비어 있지 않은 --path 값이 필요합니다.",
        )));
    }
    let project_root = fs::canonicalize(paths::project_root()).map_err(|err| {
        AppError::runtime(format!(
            "project root를 해석하지 못했습니다: {} ({err})",
            paths::project_root().display()
        ))
    })?;
    let raw = Path::new(raw_path);
    let candidate = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        project_root.join(raw)
    };
    let absolute_path = fs::canonicalize(&candidate).map_err(|err| {
        AppError::runtime(format!(
            "{operation} 대상 path를 해석하지 못했습니다: {} ({err})",
            candidate.display()
        ))
    })?;
    let relative = absolute_path.strip_prefix(&project_root).map_err(|_| {
        AppError::blocked(format!(
            "{operation} 차단\n- 이유: project boundary 밖 path입니다.\n- path: {}",
            raw_path
        ))
    })?;
    let relative_path = relative
        .to_str()
        .ok_or_else(|| {
            AppError::blocked(format!(
                "{operation} 차단\n- 이유: canonical project-relative path가 UTF-8이 아닙니다.\n- 동작: proposal, journal, event, source를 변경하지 않았습니다."
            ))
        })?
        .replace('\\', "/");

    Ok(TargetPath {
        absolute_path,
        relative_path,
    })
}

pub(super) fn write_proposal_record(preview: &PatchPreview) -> Result<(), AppError> {
    if let Some(parent) = preview.proposal_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            AppError::runtime(format!(
                "patch proposal directory를 만들지 못했습니다: {} ({err})",
                parent.display()
            ))
        })?;
    }
    if preview.proposal_path.exists() {
        return Err(AppError::blocked(format!("patch proposal 저장 차단\n- 이유: immutable proposal artifact가 이미 존재합니다.\n- path: {}", preview.proposal_path.display())));
    }
    let body = proposal_domain::render_record(preview);
    crate::adapters::filesystem::atomic_write::atomic_replace_bytes(
        &preview.proposal_path,
        body.as_bytes(),
    )
}

pub(super) fn issue_approval_token() -> Result<String, AppError> {
    let mut bytes = [0_u8; APPROVAL_TOKEN_BYTES];
    fill_os_random(&mut bytes)?;
    Ok(approval_domain::token_from_entropy(&bytes))
}

#[cfg(unix)]
fn fill_os_random(bytes: &mut [u8]) -> Result<(), AppError> {
    File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(bytes))
        .map_err(|err| AppError::runtime(format!("OS CSPRNG nonce 발급 실패: {err}")))
}

#[cfg(windows)]
fn fill_os_random(bytes: &mut [u8]) -> Result<(), AppError> {
    type NtStatus = i32;
    #[link(name = "bcrypt")]
    extern "system" {
        fn BCryptGenRandom(
            algorithm: *mut std::ffi::c_void,
            buffer: *mut u8,
            length: u32,
            flags: u32,
        ) -> NtStatus;
    }
    const BCRYPT_USE_SYSTEM_PREFERRED_RNG: u32 = 0x00000002;
    // SAFETY: the OS writes exactly `bytes.len()` bytes to the live mutable buffer.
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            bytes.as_mut_ptr(),
            bytes.len() as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status < 0 {
        Err(AppError::runtime(format!(
            "OS CSPRNG nonce 발급 실패: NTSTATUS {status:#x}"
        )))
    } else {
        Ok(())
    }
}
