use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::foundation::error::AppError;

pub struct RecoverableLease {
    _file: File,
    _owner_claim: OwnerClaim,
}

struct OwnerClaim {
    path: PathBuf,
    file: File,
    _namespace: OwnerNamespace,
}

struct OwnerNamespace {
    directory: PathBuf,
    _guard_path: PathBuf,
    _guard: File,
}

impl RecoverableLease {
    pub fn acquire(path: PathBuf, context: &str) -> Result<Self, AppError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                AppError::runtime(format!("{context} lock directory 실패: {err}"))
            })?;
        }
        let owner_namespace = OwnerNamespace::acquire(&path, context)?;
        remove_stale_owner_claims(&owner_namespace.directory, context)?;
        let owner_claim = OwnerClaim::create(owner_namespace, context)?;
        reject_non_regular_lock_path(&path, context)?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&path)
            .map_err(|err| AppError::runtime(format!("{context} lock 열기 실패: {err}")))?;
        validate_open_lock_identity(&path, &file, context)?;
        match file.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => {
                return Err(AppError::blocked(format!("{context} lock 차단")))
            }
            Err(std::fs::TryLockError::Error(err)) => {
                return Err(AppError::runtime(format!(
                    "{context} kernel lock 획득 실패: {err}"
                )))
            }
        }
        validate_open_lock_identity(&path, &file, context)?;
        let nonce = format!("{}-{}", std::process::id(), now_nanos());
        let body = format!("pid={}\nnonce={nonce}\n", std::process::id());
        file.set_len(0)
            .and_then(|_| file.seek(SeekFrom::Start(0)).map(|_| ()))
            .and_then(|_| file.write_all(body.as_bytes()))
            .and_then(|_| file.sync_all())
            .map_err(|err| AppError::runtime(format!("{context} lock 기록 실패: {err}")))?;
        validate_open_lock_identity(&path, &file, context)?;
        Ok(Self {
            _file: file,
            _owner_claim: owner_claim,
        })
    }

    pub fn acquire_with_wait(
        path: PathBuf,
        context: &str,
        timeout: Duration,
    ) -> Result<Self, AppError> {
        Self::acquire_with_wait_observing(path, context, timeout, || {})
    }

    #[cfg(test)]
    pub(crate) fn acquire_with_wait_after_first_block(
        path: PathBuf,
        context: &str,
        timeout: Duration,
        on_first_block: impl FnOnce(),
    ) -> Result<Self, AppError> {
        Self::acquire_with_wait_observing(path, context, timeout, on_first_block)
    }

    fn acquire_with_wait_observing(
        path: PathBuf,
        context: &str,
        timeout: Duration,
        on_first_block: impl FnOnce(),
    ) -> Result<Self, AppError> {
        let deadline = Instant::now() + timeout;
        let mut on_first_block = Some(on_first_block);
        loop {
            match Self::acquire(path.clone(), context) {
                Ok(lease) => return Ok(lease),
                Err(err)
                    if Instant::now() < deadline
                        && err.message.contains(&format!("{context} lock 차단")) =>
                {
                    if let Some(on_first_block) = on_first_block.take() {
                        on_first_block();
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(err) => return Err(err),
            }
        }
    }
}

impl OwnerClaim {
    fn create(namespace: OwnerNamespace, context: &str) -> Result<Self, AppError> {
        for sequence in 0..8_u8 {
            let nonce = format!("{}-{}-{sequence}", std::process::id(), now_nanos());
            let path = namespace.directory.join(format!("claim-{nonce}"));
            let mut options = OpenOptions::new();
            options.read(true).write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = match options.open(&path) {
                Ok(file) => file,
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(err) => {
                    return Err(AppError::runtime(format!(
                        "{context} owner claim 생성 실패: {err}"
                    )))
                }
            };
            match file.try_lock() {
                Ok(()) => {}
                Err(std::fs::TryLockError::WouldBlock) => {
                    return Err(AppError::blocked(format!(
                        "{context} lock 차단: owner claim kernel lock"
                    )))
                }
                Err(std::fs::TryLockError::Error(err)) => {
                    return Err(AppError::runtime(format!(
                        "{context} owner claim kernel lock 획득 실패: {err}"
                    )))
                }
            }
            file.write_all(format!("pid={}\nnonce={nonce}\n", std::process::id()).as_bytes())
                .and_then(|_| file.sync_all())
                .map_err(|err| {
                    AppError::runtime(format!("{context} owner claim 기록 실패: {err}"))
                })?;
            validate_open_lock_identity(&path, &file, context)?;
            return Ok(Self {
                path,
                file,
                _namespace: namespace,
            });
        }
        Err(AppError::blocked(format!(
            "{context} owner claim nonce 충돌"
        )))
    }
}

impl OwnerNamespace {
    fn acquire(lock_path: &Path, context: &str) -> Result<Self, AppError> {
        let directory = owner_claim_directory(lock_path, context)?;
        let (guard_path, guard) = open_owner_namespace_guard(&directory, context)?;
        validate_open_owner_namespace_identity(&guard_path, &guard, context)?;
        match guard.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => {
                return Err(AppError::blocked(format!(
                    "{context} lock 차단: active owner namespace"
                )))
            }
            Err(std::fs::TryLockError::Error(err)) => {
                return Err(AppError::runtime(format!(
                    "{context} owner namespace kernel lock 획득 실패: {err}"
                )))
            }
        }
        validate_open_owner_namespace_identity(&guard_path, &guard, context)?;
        Ok(Self {
            directory,
            _guard_path: guard_path,
            _guard: guard,
        })
    }
}

impl Drop for OwnerClaim {
    fn drop(&mut self) {
        if validate_open_lock_identity(&self.path, &self.file, "owner claim cleanup").is_ok() {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn remove_stale_owner_claims(directory: &Path, context: &str) -> Result<(), AppError> {
    const OWNER_SCAN_LIMIT: usize = 128;
    let mut matched = 0_usize;
    for entry in fs::read_dir(directory)
        .map_err(|err| AppError::runtime(format!("{context} owner claim scan 실패: {err}")))?
    {
        let entry = entry
            .map_err(|err| AppError::runtime(format!("{context} owner claim entry 실패: {err}")))?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !name.starts_with("claim-") {
            return Err(AppError::blocked(format!(
                "{context} owner claim namespace 불일치; 증거를 보존했습니다."
            )));
        }
        matched = matched.saturating_add(1);
        if matched > OWNER_SCAN_LIMIT {
            return Err(AppError::blocked(format!(
                "{context} owner claim scan budget 초과; 증거를 보존했습니다."
            )));
        }
        let owner_path = entry.path();
        reject_non_regular_lock_path(&owner_path, context)?;
        let owner = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&owner_path)
            .map_err(|err| {
                if err.kind() == std::io::ErrorKind::NotFound {
                    AppError::blocked(format!(
                        "{context} lock 차단: owner claim changed during scan"
                    ))
                } else {
                    AppError::blocked(format!("{context} owner claim 열기 실패: {err}"))
                }
            })?;
        validate_open_owner_claim_identity(&owner_path, &owner, context)?;
        match owner.try_lock() {
            Err(std::fs::TryLockError::WouldBlock) => {
                return Err(AppError::blocked(format!(
                    "{context} lock 차단: active owner claim"
                )))
            }
            Err(std::fs::TryLockError::Error(err)) => {
                return Err(AppError::runtime(format!(
                    "{context} owner claim 검사 실패: {err}"
                )))
            }
            Ok(()) => {
                validate_open_owner_claim_identity(&owner_path, &owner, context)?;
                drop(owner);
                match fs::remove_file(&owner_path) {
                    Ok(()) => {}
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                    Err(err) => {
                        return Err(AppError::blocked(format!(
                            "{context} stale owner claim 정리 실패: {err}"
                        )))
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
fn open_owner_namespace_guard(
    directory: &Path,
    context: &str,
) -> Result<(PathBuf, File), AppError> {
    let guard = File::open(directory)
        .map_err(|err| AppError::runtime(format!("{context} owner namespace 열기 실패: {err}")))?;
    Ok((directory.to_path_buf(), guard))
}

#[cfg(not(unix))]
fn open_owner_namespace_guard(
    directory: &Path,
    context: &str,
) -> Result<(PathBuf, File), AppError> {
    let guard_path = directory
        .parent()
        .ok_or_else(|| AppError::runtime(format!("{context} owner namespace parent 누락")))?
        .join("namespace.lock");
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    let guard = options
        .open(&guard_path)
        .map_err(|err| AppError::runtime(format!("{context} owner namespace 열기 실패: {err}")))?;
    Ok((guard_path, guard))
}

#[cfg(unix)]
fn validate_open_owner_namespace_identity(
    path: &Path,
    file: &File,
    context: &str,
) -> Result<(), AppError> {
    use std::os::unix::fs::MetadataExt;

    let path_metadata = fs::symlink_metadata(path).map_err(|err| {
        AppError::blocked(format!("{context} owner namespace 경로 재검증 실패: {err}"))
    })?;
    let file_metadata = file.metadata().map_err(|err| {
        AppError::blocked(format!("{context} owner namespace handle 검증 실패: {err}"))
    })?;
    if path_metadata.file_type().is_symlink()
        || !path_metadata.is_dir()
        || !file_metadata.is_dir()
        || path_metadata.dev() != file_metadata.dev()
        || path_metadata.ino() != file_metadata.ino()
    {
        return Err(AppError::blocked(format!(
            "{context} owner namespace path/handle identity 불일치; 증거를 보존했습니다."
        )));
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_open_owner_namespace_identity(
    path: &Path,
    file: &File,
    context: &str,
) -> Result<(), AppError> {
    validate_open_lock_identity(path, file, context)
}

fn validate_open_owner_claim_identity(
    path: &Path,
    file: &File,
    context: &str,
) -> Result<(), AppError> {
    match validate_open_lock_identity(path, file, context) {
        Ok(()) => Ok(()),
        Err(error) => match fs::symlink_metadata(path) {
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Err(AppError::blocked(
                format!("{context} lock 차단: owner claim changed during scan"),
            )),
            _ => Err(error),
        },
    }
}

fn owner_claim_directory(lock_path: &Path, context: &str) -> Result<PathBuf, AppError> {
    let parent = lock_path
        .parent()
        .ok_or_else(|| AppError::runtime(format!("{context} lock parent 누락")))?;
    let parent = fs::canonicalize(parent).map_err(|err| {
        AppError::runtime(format!("{context} lock parent canonicalize 실패: {err}"))
    })?;
    let file_name = lock_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| AppError::blocked(format!("{context} lock filename 불일치")))?;
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in parent
        .as_os_str()
        .as_encoded_bytes()
        .iter()
        .copied()
        .chain([0])
        .chain(file_name.as_bytes().iter().copied())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let root = std::env::temp_dir().join(format!("rpotato-lease-owner-claims-{hash:016x}"));
    let directory = root.join("claims");
    fs::create_dir_all(&directory).map_err(|err| {
        AppError::runtime(format!("{context} owner claim directory 생성 실패: {err}"))
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).map_err(|err| {
            AppError::runtime(format!("{context} owner claim root 권한 설정 실패: {err}"))
        })?;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).map_err(|err| {
            AppError::runtime(format!(
                "{context} owner claim directory 권한 설정 실패: {err}"
            ))
        })?;
    }
    for path in [&root, &directory] {
        let metadata = fs::symlink_metadata(path).map_err(|err| {
            AppError::blocked(format!("{context} owner claim directory 검증 실패: {err}"))
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(AppError::blocked(format!(
                "{context} owner claim directory type 불일치"
            )));
        }
    }
    Ok(directory)
}

fn reject_non_regular_lock_path(path: &Path, context: &str) -> Result<(), AppError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.file_type().is_file() => {
            Err(AppError::blocked(format!(
                "{context} lock type 불일치; 증거를 보존했습니다."
            )))
        }
        Ok(_) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(AppError::blocked(format!(
            "{context} lock metadata 실패: {err}"
        ))),
    }
}

#[cfg(unix)]
fn validate_open_lock_identity(path: &Path, file: &File, context: &str) -> Result<(), AppError> {
    use std::os::unix::fs::MetadataExt;

    let path_metadata = fs::symlink_metadata(path)
        .map_err(|err| AppError::blocked(format!("{context} lock 경로 재검증 실패: {err}")))?;
    let file_metadata = file
        .metadata()
        .map_err(|err| AppError::blocked(format!("{context} lock handle 검증 실패: {err}")))?;
    if path_metadata.file_type().is_symlink()
        || !path_metadata.file_type().is_file()
        || path_metadata.dev() != file_metadata.dev()
        || path_metadata.ino() != file_metadata.ino()
    {
        return Err(AppError::blocked(format!(
            "{context} lock path/handle identity 불일치; 증거를 보존했습니다."
        )));
    }
    Ok(())
}

#[cfg(windows)]
fn validate_open_lock_identity(path: &Path, file: &File, context: &str) -> Result<(), AppError> {
    let path_metadata = fs::symlink_metadata(path)
        .map_err(|err| AppError::blocked(format!("{context} lock 경로 재검증 실패: {err}")))?;
    let same_file = crate::windows_file::path_refers_to_open_file(path, file)
        .map_err(|err| AppError::blocked(format!("{context} lock handle 검증 실패: {err}")))?;
    if path_metadata.file_type().is_symlink() || !path_metadata.file_type().is_file() || !same_file
    {
        return Err(AppError::blocked(format!(
            "{context} lock path/handle identity 불일치; 증거를 보존했습니다."
        )));
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn validate_open_lock_identity(path: &Path, file: &File, context: &str) -> Result<(), AppError> {
    let path_metadata = fs::symlink_metadata(path)
        .map_err(|err| AppError::blocked(format!("{context} lock 경로 재검증 실패: {err}")))?;
    let file_metadata = file
        .metadata()
        .map_err(|err| AppError::blocked(format!("{context} lock handle 검증 실패: {err}")))?;
    if path_metadata.file_type().is_symlink()
        || !path_metadata.file_type().is_file()
        || path_metadata.len() != file_metadata.len()
    {
        return Err(AppError::blocked(format!(
            "{context} lock path/handle identity 불일치; 증거를 보존했습니다."
        )));
    }
    Ok(())
}

fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{mpsc, Arc, Barrier};

    #[test]
    fn kernel_lease_excludes_live_owner_and_reuses_persistent_lock_file() {
        let root = std::env::temp_dir().join(format!(
            "rpotato-kernel-lease-{}-{}",
            std::process::id(),
            now_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("lease.lock");
        let first = RecoverableLease::acquire(path.clone(), "test").unwrap();
        assert!(RecoverableLease::acquire(path.clone(), "test").is_err());
        drop(first);
        assert!(path.exists());
        let second = RecoverableLease::acquire(path.clone(), "test").unwrap();
        drop(second);
        assert!(path.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn concurrent_kernel_lease_has_exactly_one_winner() {
        let root = std::env::temp_dir().join(format!(
            "rpotato-kernel-lease-race-{}-{}",
            std::process::id(),
            now_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("lease.lock");
        let start = Arc::new(Barrier::new(9));
        let finish = Arc::new(Barrier::new(9));
        let (sender, receiver) = mpsc::channel();
        let mut workers = Vec::new();
        for _ in 0..8 {
            let path = path.clone();
            let start = Arc::clone(&start);
            let finish = Arc::clone(&finish);
            let sender = sender.clone();
            workers.push(std::thread::spawn(move || {
                start.wait();
                let lease = RecoverableLease::acquire(path, "race").ok();
                sender.send(lease.is_some()).unwrap();
                finish.wait();
                drop(lease);
            }));
        }
        start.wait();
        let winners = (0..8)
            .map(|_| receiver.recv().unwrap())
            .filter(|won| *won)
            .count();
        assert_eq!(winners, 1);
        finish.wait();
        for worker in workers {
            worker.join().unwrap();
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn replacing_persistent_lock_path_cannot_create_a_second_live_owner() {
        let root = std::env::temp_dir().join(format!(
            "rpotato-kernel-lease-replaced-{}-{}",
            std::process::id(),
            now_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("lease.lock");
        let displaced = root.join("lease.lock.displaced");
        let first = RecoverableLease::acquire(path.clone(), "replacement").unwrap();
        fs::rename(&path, &displaced).unwrap();
        fs::write(&path, b"replacement inode").unwrap();

        let blocked = match RecoverableLease::acquire(path.clone(), "replacement") {
            Ok(_) => panic!("replacement inode acquired while owner claim was live"),
            Err(error) => error,
        };
        assert!(blocked.message.contains("active owner namespace"));

        drop(first);
        let second = RecoverableLease::acquire(path.clone(), "replacement").unwrap();
        drop(second);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn namespace_guard_blocks_when_live_claim_path_is_displaced() {
        let root = std::env::temp_dir().join(format!(
            "rpotato-kernel-lease-hidden-claim-{}-{}",
            std::process::id(),
            now_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("lease.lock");
        let displaced_lock = root.join("lease.lock.displaced");
        let first = RecoverableLease::acquire(path.clone(), "hidden-claim").unwrap();
        let claims = owner_claim_directory(&path, "hidden-claim").unwrap();
        let live_claim = fs::read_dir(&claims)
            .unwrap()
            .map(Result::unwrap)
            .find(|entry| entry.file_name().to_string_lossy().starts_with("claim-"))
            .expect("live owner claim이 필요합니다.")
            .path();
        let hidden_claim = claims.parent().unwrap().join("hidden-live-claim");
        fs::rename(&live_claim, &hidden_claim).unwrap();
        fs::rename(&path, &displaced_lock).unwrap();
        fs::write(&path, b"replacement inode").unwrap();

        let blocked = match RecoverableLease::acquire(path.clone(), "hidden-claim") {
            Ok(_) => panic!("replacement inode acquired after live claim displacement"),
            Err(error) => error,
        };
        assert!(blocked.message.contains("active owner namespace"));

        drop(first);
        fs::remove_file(hidden_claim).unwrap();
        let second = RecoverableLease::acquire(path.clone(), "hidden-claim").unwrap();
        drop(second);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn kernel_lease_is_released_when_owner_process_is_killed() {
        use std::process::Command;

        if let Some(path) = std::env::var_os("RPOTATO_TEST_KERNEL_LEASE_HELPER") {
            let path = PathBuf::from(path);
            let ready = path.with_extension("ready");
            let _lease = RecoverableLease::acquire(path, "helper").unwrap();
            fs::write(ready, b"ready").unwrap();
            std::thread::sleep(Duration::from_secs(30));
            return;
        }

        let root = std::env::temp_dir().join(format!(
            "rpotato-kernel-lease-kill-{}-{}",
            std::process::id(),
            now_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("lease.lock");
        let ready = path.with_extension("ready");
        let mut owner = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "lease::tests::kernel_lease_is_released_when_owner_process_is_killed",
                "--nocapture",
            ])
            .env("RPOTATO_TEST_KERNEL_LEASE_HELPER", &path)
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !ready.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(ready.exists());
        assert!(RecoverableLease::acquire(path.clone(), "test").is_err());
        owner.kill().unwrap();
        owner.wait().unwrap();
        let reclaimed = RecoverableLease::acquire(path, "test").unwrap();
        drop(reclaimed);
        let _ = fs::remove_dir_all(root);
    }
}
