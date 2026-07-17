use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::foundation::error::AppError;

mod identity;
use identity::{
    open_owner_namespace_guard, owner_claim_directory, reject_non_regular_lock_path,
    remove_stale_owner_claims, validate_open_lock_identity, validate_open_owner_namespace_identity,
};

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
                "adapters::filesystem::lease::tests::kernel_lease_is_released_when_owner_process_is_killed",
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
