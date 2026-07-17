use super::super::{read_open_file_bounded, sha256_bytes};
use super::fd_ops::{dir_linkat, dir_unlinkat, mkdirat_directory, openat_file, unix_open_flags};
use super::validate_source_metadata;
use crate::adapters::filesystem::layout as paths;
use crate::app::workflow_adapter::transition;
use crate::foundation::error::AppError;
use std::fs::File;

pub(super) struct PreparedSourceDir {
    handle: File,
    pub(super) target: String,
    pub(super) temporary: String,
    pub(super) guard: String,
}

impl PreparedSourceDir {
    pub(super) fn open(plan: &transition::SourceInstallV1) -> Result<Self, AppError> {
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

    pub(super) fn open_existing(&self, name: &str) -> Result<Option<File>, AppError> {
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

    pub(super) fn create_new(&self, name: &str, mode: u32) -> Result<File, AppError> {
        openat_file(
            &self.handle,
            name,
            unix_open_flags::WRITE_CREATE_NEW_NOFOLLOW,
            mode,
            "prepared source create-new",
        )
    }

    pub(super) fn stage_hash(&self, name: &str) -> Result<Option<String>, AppError> {
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

    pub(super) fn validate_original(
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

    pub(super) fn validate_installed(
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

    pub(super) fn validate_original_pair(
        &self,
        plan: &transition::SourceInstallV1,
    ) -> Result<(), AppError> {
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

    pub(super) fn validate_installed_pair(
        &self,
        plan: &transition::SourceInstallV1,
    ) -> Result<(), AppError> {
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

    pub(super) fn link(&self, from: &str, to: &str) -> Result<(), AppError> {
        dir_linkat(&self.handle, from, to)
    }

    pub(super) fn unlink(&self, name: &str) -> Result<(), AppError> {
        dir_unlinkat(&self.handle, name)
    }

    pub(super) fn sync(&self) -> Result<(), AppError> {
        self.handle
            .sync_all()
            .map_err(|err| AppError::runtime(format!("source parent fsync 실패: {err}")))
    }
}

pub(super) struct PreparedRollbackDir {
    handle: File,
    rollback: String,
}

impl PreparedRollbackDir {
    pub(super) fn preflight(plan: &transition::SourceInstallV1) -> Result<(), AppError> {
        let _ = Self::open(plan, false)?;
        Ok(())
    }

    pub(super) fn open(
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

    pub(super) fn open_existing(&self) -> Result<Option<File>, AppError> {
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

    pub(super) fn create_new(&self) -> Result<File, AppError> {
        openat_file(
            &self.handle,
            &self.rollback,
            unix_open_flags::WRITE_CREATE_NEW_NOFOLLOW,
            0o600,
            "prepared rollback create-new",
        )
    }

    pub(super) fn validate(&self, plan: &transition::SourceInstallV1) -> Result<(), AppError> {
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

    pub(super) fn sync(&self) -> Result<(), AppError> {
        self.handle
            .sync_all()
            .map_err(|err| AppError::runtime(format!("source rollback parent fsync 실패: {err}")))
    }
}
