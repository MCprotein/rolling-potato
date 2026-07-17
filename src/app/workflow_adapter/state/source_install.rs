use super::*;

#[cfg(unix)]
use std::fs::File;

#[cfg(unix)]
mod fd_ops;
#[cfg(unix)]
use fd_ops::{dir_linkat, dir_unlinkat, mkdirat_directory, openat_file, unix_open_flags};

#[cfg(not(unix))]
pub(crate) fn install_prepared_source_bundle(
    _bundle: &transition::PreparedSourceBundle,
    _journal_path: &std::path::Path,
) -> Result<(), AppError> {
    Err(AppError::blocked(format!(
        "source install 차단\n- code: source-install.unsupported-platform\n- platform: {}\n- 지원 범위: v0.34.0 source installation은 Unix만 지원합니다.\n- 동작: journal/temp/guard/rollback/target 변경 없음",
        std::env::consts::OS
    )))
}

#[cfg(unix)]
pub(crate) fn install_prepared_source_bundle(
    bundle: &transition::PreparedSourceBundle,
    journal_path: &std::path::Path,
) -> Result<(), AppError> {
    let body = read_regular_file_bounded(
        journal_path,
        MAX_PREPARED_SOURCE_BUNDLE_BYTES,
        "prepared source journal",
    )?;
    if transition::parse_prepared_source_bundle(&body)? != *bundle {
        return Err(AppError::blocked(
            "prepared source journal/bundle binding 불일치",
        ));
    }
    recover_source_replace(journal_path)
}

#[cfg(unix)]
pub(crate) fn validate_prepared_source_parent(
    bundle: &transition::PreparedSourceBundle,
) -> Result<(), AppError> {
    let plan = bundle
        .source_install
        .as_ref()
        .ok_or_else(|| AppError::blocked("prepared source parent plan 누락"))?;
    PreparedSourceDir::open(plan)?;
    PreparedRollbackDir::preflight(plan)
}

#[cfg(unix)]
pub(crate) fn validate_source_install_initial_admission(
    plan: &transition::SourceInstallV1,
) -> Result<(), AppError> {
    let Some(directory) = PreparedRollbackDir::open(plan, false)? else {
        return Ok(());
    };
    if directory.open_existing()?.is_some() {
        return Err(AppError::blocked(
            "source rollback create-new admission 차단: rollback path가 journal commit 전에 이미 존재합니다.",
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn validate_prepared_source_parent(
    _bundle: &transition::PreparedSourceBundle,
) -> Result<(), AppError> {
    Err(AppError::blocked(format!(
        "source install 차단\n- code: source-install.unsupported-platform\n- platform: {}",
        std::env::consts::OS
    )))
}

#[cfg(unix)]
struct PreparedSourceDir {
    handle: File,
    target: String,
    temporary: String,
    guard: String,
}

#[cfg(unix)]
impl PreparedSourceDir {
    fn open(plan: &transition::SourceInstallV1) -> Result<Self, AppError> {
        use std::os::unix::fs::MetadataExt;

        if plan.target.parent != plan.install_temp.parent
            || plan.target.parent != plan.guard_path.parent
        {
            return Err(AppError::blocked(
                "prepared source sibling parent binding 불일치",
            ));
        }
        let root = paths::project_root().canonicalize().map_err(|err| {
            AppError::blocked(format!(
                "prepared source project root canonicalize 실패: {err}"
            ))
        })?;
        let mut handle = File::open(&root).map_err(|err| {
            AppError::blocked(format!("prepared source project root open 실패: {err}"))
        })?;
        for component in plan
            .target
            .parent
            .split('/')
            .filter(|value| !value.is_empty())
        {
            handle = openat_file(
                &handle,
                component,
                unix_open_flags::READ_DIRECTORY_NOFOLLOW,
                0,
                "prepared source parent traversal",
            )?;
        }
        let metadata = handle.metadata().map_err(|err| {
            AppError::blocked(format!("prepared source parent metadata 실패: {err}"))
        })?;
        if !metadata.is_dir() || metadata.dev() != plan.unix_metadata.before_dev {
            return Err(AppError::blocked(
                "prepared source parent directory/filesystem binding 불일치",
            ));
        }
        Ok(Self {
            handle,
            target: plan.target.basename.clone(),
            temporary: plan.install_temp.basename.clone(),
            guard: plan.guard_path.basename.clone(),
        })
    }

    fn open_existing(&self, name: &str) -> Result<Option<File>, AppError> {
        match openat_file(
            &self.handle,
            name,
            unix_open_flags::READ_FILE_NOFOLLOW,
            0,
            "prepared source stage open",
        ) {
            Ok(file) => Ok(Some(file)),
            Err(error) if error.message.ends_with("(not found)") => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn create_new(&self, name: &str, mode: u32) -> Result<File, AppError> {
        openat_file(
            &self.handle,
            name,
            unix_open_flags::WRITE_CREATE_NEW_NOFOLLOW,
            mode,
            "prepared source create-new",
        )
    }

    fn stage_hash(&self, name: &str) -> Result<Option<String>, AppError> {
        let Some(mut file) = self.open_existing(name)? else {
            return Ok(None);
        };
        if !file
            .metadata()
            .map_err(|err| AppError::blocked(format!("source stage metadata 실패: {err}")))?
            .is_file()
        {
            return Err(AppError::blocked("source stage type 불일치"));
        }
        let bytes = read_open_file_bounded(
            &mut file,
            transition::MAX_SOURCE_BLOB_BYTES as u64,
            "source stage reread",
        )?;
        Ok(Some(sha256_bytes(&bytes)))
    }

    fn validate_original(
        &self,
        name: &str,
        plan: &transition::SourceInstallV1,
    ) -> Result<(), AppError> {
        use std::os::unix::fs::MetadataExt;
        let file = self
            .open_existing(name)?
            .ok_or_else(|| AppError::blocked("source original 누락"))?;
        if self.stage_hash(name)?.as_deref() != Some(plan.before_sha256.as_str()) {
            return Err(AppError::blocked("source stage hash/type 불일치"));
        }
        let metadata = file
            .metadata()
            .map_err(|err| AppError::blocked(format!("source original metadata 실패: {err}")))?;
        validate_source_metadata(&metadata, plan, false)?;
        let identity =
            transition::source_identity_v1(metadata.dev(), metadata.ino(), &plan.before_sha256)?;
        if plan.target.expected_identity.as_deref() != Some(identity.as_str()) {
            return Err(AppError::blocked(
                "source original expected identity 불일치",
            ));
        }
        Ok(())
    }

    fn validate_installed(
        &self,
        name: &str,
        plan: &transition::SourceInstallV1,
    ) -> Result<(), AppError> {
        if self.stage_hash(name)?.as_deref() != Some(plan.proposed_sha256.as_str()) {
            return Err(AppError::blocked("source stage hash/type 불일치"));
        }
        let file = self
            .open_existing(name)?
            .ok_or_else(|| AppError::blocked("source installed 누락"))?;
        let metadata = file
            .metadata()
            .map_err(|err| AppError::blocked(format!("source installed metadata 실패: {err}")))?;
        validate_source_metadata(&metadata, plan, true)
    }

    fn validate_original_pair(&self, plan: &transition::SourceInstallV1) -> Result<(), AppError> {
        use std::os::unix::fs::MetadataExt;
        self.validate_original(&self.target, plan)?;
        self.validate_original(&self.guard, plan)?;
        let target = self.open_existing(&self.target)?.expect("validated target");
        let guard = self.open_existing(&self.guard)?.expect("validated guard");
        let target_metadata = target
            .metadata()
            .map_err(|err| AppError::blocked(format!("source target identity 실패: {err}")))?;
        let guard_metadata = guard
            .metadata()
            .map_err(|err| AppError::blocked(format!("source guard identity 실패: {err}")))?;
        if target_metadata.dev() != guard_metadata.dev()
            || target_metadata.ino() != guard_metadata.ino()
        {
            return Err(AppError::blocked(
                "source target/guard inode identity 불일치",
            ));
        }
        Ok(())
    }

    fn validate_installed_pair(&self, plan: &transition::SourceInstallV1) -> Result<(), AppError> {
        use std::os::unix::fs::MetadataExt;
        self.validate_installed(&self.target, plan)?;
        self.validate_installed(&self.temporary, plan)?;
        let target = self.open_existing(&self.target)?.expect("validated target");
        let temporary = self
            .open_existing(&self.temporary)?
            .expect("validated temporary");
        let target_metadata = target
            .metadata()
            .map_err(|err| AppError::blocked(format!("installed source identity 실패: {err}")))?;
        let temporary_metadata = temporary
            .metadata()
            .map_err(|err| AppError::blocked(format!("install temp identity 실패: {err}")))?;
        if target_metadata.dev() != temporary_metadata.dev()
            || target_metadata.ino() != temporary_metadata.ino()
        {
            return Err(AppError::blocked(
                "installed target/temp inode identity 불일치",
            ));
        }
        Ok(())
    }

    fn link(&self, from: &str, to: &str) -> Result<(), AppError> {
        dir_linkat(&self.handle, from, to)
    }

    fn unlink(&self, name: &str) -> Result<(), AppError> {
        dir_unlinkat(&self.handle, name)
    }

    fn sync(&self) -> Result<(), AppError> {
        self.handle
            .sync_all()
            .map_err(|err| AppError::runtime(format!("source parent fsync 실패: {err}")))
    }
}

#[cfg(unix)]
struct PreparedRollbackDir {
    handle: File,
    rollback: String,
}

#[cfg(unix)]
impl PreparedRollbackDir {
    fn preflight(plan: &transition::SourceInstallV1) -> Result<(), AppError> {
        let _ = Self::open(plan, false)?;
        Ok(())
    }

    fn open(
        plan: &transition::SourceInstallV1,
        create_missing: bool,
    ) -> Result<Option<Self>, AppError> {
        let root = paths::project_root().canonicalize().map_err(|err| {
            AppError::blocked(format!(
                "prepared rollback project root canonicalize 실패: {err}"
            ))
        })?;
        let mut handle = File::open(&root).map_err(|err| {
            AppError::blocked(format!("prepared rollback project root open 실패: {err}"))
        })?;
        for component in plan
            .rollback_final
            .parent
            .split('/')
            .filter(|value| !value.is_empty())
        {
            match openat_file(
                &handle,
                component,
                unix_open_flags::READ_DIRECTORY_NOFOLLOW,
                0,
                "prepared rollback parent traversal",
            ) {
                Ok(next) => handle = next,
                Err(error) if error.message.ends_with("(not found)") && !create_missing => {
                    return Ok(None);
                }
                Err(error) if error.message.ends_with("(not found)") => {
                    mkdirat_directory(&handle, component, 0o700)?;
                    handle = openat_file(
                        &handle,
                        component,
                        unix_open_flags::READ_DIRECTORY_NOFOLLOW,
                        0,
                        "prepared rollback created parent open",
                    )?;
                }
                Err(error) => return Err(error),
            }
        }
        let metadata = handle.metadata().map_err(|err| {
            AppError::blocked(format!("prepared rollback parent metadata 실패: {err}"))
        })?;
        if !metadata.is_dir() {
            return Err(AppError::blocked("prepared rollback parent type 불일치"));
        }
        Ok(Some(Self {
            handle,
            rollback: plan.rollback_final.basename.clone(),
        }))
    }

    fn open_existing(&self) -> Result<Option<File>, AppError> {
        match openat_file(
            &self.handle,
            &self.rollback,
            unix_open_flags::READ_FILE_NOFOLLOW,
            0,
            "prepared rollback open",
        ) {
            Ok(file) => Ok(Some(file)),
            Err(error) if error.message.ends_with("(not found)") => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn create_new(&self) -> Result<File, AppError> {
        openat_file(
            &self.handle,
            &self.rollback,
            unix_open_flags::WRITE_CREATE_NEW_NOFOLLOW,
            0o600,
            "prepared rollback create-new",
        )
    }

    fn validate(&self, plan: &transition::SourceInstallV1) -> Result<(), AppError> {
        let mut file = self
            .open_existing()?
            .ok_or_else(|| AppError::blocked("source rollback 누락"))?;
        let metadata = file
            .metadata()
            .map_err(|err| AppError::blocked(format!("source rollback metadata 실패: {err}")))?;
        if !metadata.is_file() {
            return Err(AppError::blocked("source rollback type 불일치"));
        }
        let bytes =
            read_open_file_bounded(&mut file, plan.before_byte_length, "source rollback read")?;
        if sha256_bytes(&bytes) != plan.before_sha256
            || u64::try_from(bytes.len()).ok() != Some(plan.before_byte_length)
        {
            return Err(AppError::blocked("source rollback hash/length 불일치"));
        }
        Ok(())
    }

    fn sync(&self) -> Result<(), AppError> {
        self.handle
            .sync_all()
            .map_err(|err| AppError::runtime(format!("source rollback parent fsync 실패: {err}")))
    }
}

#[cfg(unix)]
pub(super) fn recover_source_replace(transaction_path: &std::path::Path) -> Result<(), AppError> {
    if !transaction_path.exists() {
        return Ok(());
    }
    let body = read_regular_file_bounded(
        transaction_path,
        MAX_PREPARED_SOURCE_BUNDLE_BYTES,
        "source recovery transaction",
    )?;
    let bundle = transition::parse_prepared_source_bundle(&body)?;
    let plan = bundle
        .source_install
        .as_ref()
        .ok_or_else(|| AppError::blocked("source transaction source_install_v1 누락"))?;
    let proposed_bytes = bundle
        .proposed_bytes
        .as_deref()
        .ok_or_else(|| AppError::blocked("source transaction proposed bytes 누락"))?;
    let source_dir = PreparedSourceDir::open(plan)?;
    let original_hash = plan.before_sha256.as_str();
    let replacement_hash = plan.proposed_sha256.as_str();

    let mut target_hash = source_dir.stage_hash(&source_dir.target)?;
    let rollback_dir = PreparedRollbackDir::open(plan, false)?;
    let rollback_exists = match rollback_dir.as_ref() {
        Some(directory) => directory.open_existing()?.is_some(),
        None => false,
    };
    if rollback_exists {
        rollback_dir
            .as_ref()
            .expect("checked rollback directory")
            .validate(plan)?;
    } else if target_hash.as_deref() == Some(original_hash) {
        source_dir.validate_original(&source_dir.target, plan)?;
        install_prepared_rollback(plan, &source_dir)?;
    } else {
        return Err(AppError::blocked(
            "source recovery rollback evidence가 누락되었습니다.",
        ));
    }
    if source_dir.stage_hash(&source_dir.temporary)?.is_none()
        && target_hash.as_deref() != Some(replacement_hash)
    {
        install_prepared_temp(plan, proposed_bytes.as_bytes(), &source_dir)?;
    }
    let guard_hash = source_dir.stage_hash(&source_dir.guard)?;
    let temporary_hash = source_dir.stage_hash(&source_dir.temporary)?;
    if temporary_hash
        .as_deref()
        .is_some_and(|hash| hash != replacement_hash)
        || guard_hash
            .as_deref()
            .is_some_and(|hash| hash != original_hash)
        || target_hash
            .as_deref()
            .is_some_and(|hash| hash != original_hash && hash != replacement_hash)
    {
        return Err(AppError::blocked(
            "source transaction recovery conflict; 외부 source를 덮어쓰지 않았습니다.",
        ));
    }

    if target_hash.as_deref() == Some(original_hash) && guard_hash.is_none() {
        if temporary_hash.as_deref() != Some(replacement_hash) {
            return Err(AppError::blocked("source transaction proposed temp 누락"));
        }
        source_dir.validate_original(&source_dir.target, plan)?;
        source_dir.link(&source_dir.target, &source_dir.guard)?;
        source_dir.validate_original_pair(plan)?;
        source_replace_fault("after-guard")?;
    }
    if source_dir.stage_hash(&source_dir.target)?.as_deref() == Some(original_hash) {
        source_dir.validate_original_pair(plan)?;
        source_dir.sync()?;
        source_dir.validate_original_pair(plan)?;
        source_dir.unlink(&source_dir.target)?;
    }
    if source_dir.stage_hash(&source_dir.target)?.is_none()
        && source_dir.stage_hash(&source_dir.guard)?.is_some()
    {
        source_dir.validate_original(&source_dir.guard, plan)?;
        if source_dir.stage_hash(&source_dir.temporary)?.as_deref() != Some(replacement_hash) {
            return Err(AppError::blocked("source recovery install temp 누락"));
        }
        source_dir.link(&source_dir.temporary, &source_dir.target)?;
        source_dir.sync()?;
        source_replace_fault("after-install")?;
    }
    target_hash = source_dir.stage_hash(&source_dir.target)?;
    if target_hash.as_deref() != Some(replacement_hash) {
        if target_hash.is_none() && source_dir.stage_hash(&source_dir.guard)?.is_none() {
            return Err(AppError::blocked("source transaction recovery bytes 누락"));
        }
        return Err(AppError::blocked("source transaction recovery bytes 누락"));
    }
    source_dir.validate_installed(&source_dir.target, plan)?;
    if source_dir.stage_hash(&source_dir.temporary)?.is_some() {
        source_dir.validate_installed_pair(plan)?;
    }
    if source_dir.stage_hash(&source_dir.temporary)?.is_some() {
        source_dir.unlink(&source_dir.temporary)?;
    }
    if source_dir.stage_hash(&source_dir.guard)?.is_some() {
        source_dir.unlink(&source_dir.guard)?;
    }
    source_dir.sync()
}

#[cfg(unix)]
fn install_prepared_temp(
    plan: &transition::SourceInstallV1,
    proposed: &[u8],
    source_dir: &PreparedSourceDir,
) -> Result<(), AppError> {
    if sha256_bytes(proposed) != plan.proposed_sha256
        || u64::try_from(proposed.len()).ok() != Some(plan.proposed_byte_length)
    {
        return Err(AppError::blocked(
            "source install temp proposed bytes binding 불일치",
        ));
    }
    let mut file = source_dir.create_new(&source_dir.temporary, 0o600)?;
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::PermissionsExt;
    unsafe extern "C" {
        fn fchown(fd: i32, owner: u32, group: u32) -> i32;
    }
    // SAFETY: `file` owns a valid open descriptor and the uid/gid were capability-checked
    // before the transition journal was committed.
    if unsafe {
        fchown(
            file.as_raw_fd(),
            plan.unix_metadata.install_uid,
            plan.unix_metadata.install_gid,
        )
    } != 0
    {
        return Err(AppError::runtime(format!(
            "source install ownership 적용 실패: {}",
            std::io::Error::last_os_error()
        )));
    }
    file.write_all(proposed)
        .map_err(|err| AppError::runtime(format!("source install temp write 실패: {err}")))?;
    file.set_permissions(fs::Permissions::from_mode(plan.unix_metadata.install_mode))
        .map_err(|err| AppError::runtime(format!("source install metadata 적용 실패: {err}")))?;
    file.sync_all()
        .map_err(|err| AppError::runtime(format!("source install temp fsync 실패: {err}")))?;
    drop(file);
    source_dir.validate_installed(&source_dir.temporary, plan)
}

#[cfg(unix)]
fn install_prepared_rollback(
    plan: &transition::SourceInstallV1,
    source_dir: &PreparedSourceDir,
) -> Result<(), AppError> {
    let rollback_dir = PreparedRollbackDir::open(plan, true)?
        .ok_or_else(|| AppError::blocked("source rollback parent 누락"))?;
    if rollback_dir.open_existing()?.is_some() {
        return rollback_dir.validate(plan);
    }
    let mut target = source_dir
        .open_existing(&source_dir.target)?
        .ok_or_else(|| AppError::blocked("source rollback original 누락"))?;
    let target_metadata = target
        .metadata()
        .map_err(|err| AppError::blocked(format!("source target metadata 실패: {err}")))?;
    let original = read_open_file_bounded(
        &mut target,
        plan.before_byte_length,
        "source rollback original",
    )?;
    if sha256_bytes(&original) != plan.before_sha256
        || u64::try_from(original.len()).ok() != Some(plan.before_byte_length)
    {
        return Err(AppError::blocked(
            "source rollback before blob binding 불일치",
        ));
    }
    let mut file = rollback_dir.create_new()?;
    file.set_permissions(target_metadata.permissions())
        .map_err(|err| AppError::runtime(format!("source rollback permission 적용 실패: {err}")))?;
    file.write_all(&original)
        .map_err(|err| AppError::runtime(format!("source rollback write 실패: {err}")))?;
    file.sync_all()
        .map_err(|err| AppError::runtime(format!("source rollback fsync 실패: {err}")))?;
    drop(file);
    rollback_dir.sync()?;
    rollback_dir.validate(plan)
}

#[cfg(unix)]
fn validate_source_metadata(
    metadata: &fs::Metadata,
    plan: &transition::SourceInstallV1,
    installed: bool,
) -> Result<(), AppError> {
    use std::os::unix::fs::MetadataExt;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(AppError::blocked("source stage type 불일치"));
    }
    let (mode, uid, gid, readonly) = if installed {
        (
            plan.unix_metadata.install_mode,
            plan.unix_metadata.install_uid,
            plan.unix_metadata.install_gid,
            plan.permissions.install_readonly,
        )
    } else {
        (
            plan.unix_metadata.before_mode,
            plan.unix_metadata.before_uid,
            plan.unix_metadata.before_gid,
            plan.permissions.before_readonly,
        )
    };
    if metadata.dev() != plan.unix_metadata.before_dev
        || metadata.mode() != mode
        || metadata.uid() != uid
        || metadata.gid() != gid
        || metadata.permissions().readonly() != readonly
    {
        return Err(AppError::blocked(
            "source stage metadata/parent binding 불일치",
        ));
    }
    if !installed
        && (metadata.dev() != plan.unix_metadata.before_dev
            || metadata.ino() != plan.unix_metadata.before_ino)
    {
        return Err(AppError::blocked("source original dev/ino binding 불일치"));
    }
    Ok(())
}

fn source_replace_fault(point: &str) -> Result<(), AppError> {
    if cfg!(debug_assertions)
        && std::env::var("RPOTATO_TEST_SOURCE_REPLACE_FAULT").as_deref() == Ok(point)
    {
        return Err(AppError::runtime(format!(
            "injected source replacement fault: {point}"
        )));
    }
    Ok(())
}
