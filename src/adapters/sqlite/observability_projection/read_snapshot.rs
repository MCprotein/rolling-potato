use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use rusqlite::{Connection, OpenFlags};

use super::{now_ms, sql_error};
use crate::adapters::filesystem::layout as paths;
use crate::foundation::error::AppError;

const READ_ONLY_PROJECTION_FILE_MAX_BYTES: u64 = 128 * 1024 * 1024;
static READ_ONLY_SNAPSHOT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(super) struct ReadOnlyProjection {
    connection: Option<Connection>,
    snapshot_dir: PathBuf,
}

impl Deref for ReadOnlyProjection {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        self.connection
            .as_ref()
            .expect("read-only projection connection remains live until drop")
    }
}

impl Drop for ReadOnlyProjection {
    fn drop(&mut self) {
        drop(self.connection.take());
        let _ = fs::remove_dir_all(&self.snapshot_dir);
    }
}

#[derive(PartialEq, Eq)]
struct StableProjectionFiles {
    database: Vec<u8>,
    wal: Option<Vec<u8>>,
}

pub(super) fn open_read_only() -> Result<ReadOnlyProjection, AppError> {
    let path = paths::observability_db_file();
    if !path.is_file() {
        return Err(AppError::blocked(format!(
            "observability read-only projection unavailable: {}",
            path.display()
        )));
    }
    open_read_only_path(&path)
}

pub(super) fn open_read_only_path(path: &std::path::Path) -> Result<ReadOnlyProjection, AppError> {
    let files = stable_projection_files(path)?;
    let snapshot_dir = create_read_only_snapshot_dir()?;
    let snapshot_path = snapshot_dir.join("observability.sqlite");
    if let Err(error) = write_private_snapshot_file(&snapshot_path, &files.database) {
        let _ = fs::remove_dir_all(&snapshot_dir);
        return Err(error);
    }
    if let Some(wal) = files.wal.as_ref() {
        if let Err(error) =
            write_private_snapshot_file(&companion_path(&snapshot_path, "-wal"), wal)
        {
            let _ = fs::remove_dir_all(&snapshot_dir);
            return Err(error);
        }
    }
    let connection = match Connection::open_with_flags(
        &snapshot_path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(connection) => connection,
        Err(error) => {
            let _ = fs::remove_dir_all(&snapshot_dir);
            return Err(sql_error("observability DB read-only snapshot open 실패")(
                error,
            ));
        }
    };
    if let Err(error) = connection.execute_batch("PRAGMA query_only = ON;") {
        drop(connection);
        let _ = fs::remove_dir_all(&snapshot_dir);
        return Err(sql_error("observability DB read-only snapshot 설정 실패")(
            error,
        ));
    }
    Ok(ReadOnlyProjection {
        connection: Some(connection),
        snapshot_dir,
    })
}

fn stable_projection_files(path: &std::path::Path) -> Result<StableProjectionFiles, AppError> {
    let journal = companion_path(path, "-journal");
    if fs::symlink_metadata(&journal).is_ok() {
        return Err(AppError::blocked(
            "observability read-only snapshot unavailable: rollback journal이 존재합니다.",
        ));
    }
    for _ in 0..3 {
        let first = capture_projection_files(path)?;
        std::thread::yield_now();
        let second = capture_projection_files(path)?;
        if first == second {
            return Ok(first);
        }
    }
    Err(AppError::blocked(
        "observability read-only snapshot unavailable: DB/WAL 세대가 안정되지 않았습니다.",
    ))
}

fn capture_projection_files(path: &std::path::Path) -> Result<StableProjectionFiles, AppError> {
    Ok(StableProjectionFiles {
        database: read_regular_snapshot_file(path, true)?.expect("required database checked"),
        wal: read_regular_snapshot_file(&companion_path(path, "-wal"), false)?,
    })
}

fn read_regular_snapshot_file(
    path: &std::path::Path,
    required: bool,
) -> Result<Option<Vec<u8>>, AppError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && !required => return Ok(None),
        Err(error) => {
            return Err(AppError::blocked(format!(
                "observability read-only snapshot metadata 실패: {} ({error})",
                path.display()
            )))
        }
    };
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(AppError::blocked(format!(
            "observability read-only snapshot regular-file binding 불일치: {}",
            path.display()
        )));
    }
    if metadata.len() > READ_ONLY_PROJECTION_FILE_MAX_BYTES {
        return Err(AppError::blocked(format!(
            "observability read-only snapshot byte budget 초과: {}",
            path.display()
        )));
    }
    let file = fs::File::open(path).map_err(|error| {
        AppError::blocked(format!(
            "observability read-only snapshot open 실패: {} ({error})",
            path.display()
        ))
    })?;
    let mut bytes = Vec::new();
    file.take(READ_ONLY_PROJECTION_FILE_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            AppError::blocked(format!(
                "observability read-only snapshot read 실패: {} ({error})",
                path.display()
            ))
        })?;
    if bytes.len() as u64 > READ_ONLY_PROJECTION_FILE_MAX_BYTES {
        return Err(AppError::blocked(format!(
            "observability read-only snapshot byte budget 초과: {}",
            path.display()
        )));
    }
    Ok(Some(bytes))
}

fn create_read_only_snapshot_dir() -> Result<PathBuf, AppError> {
    for _ in 0..8 {
        let sequence = READ_ONLY_SNAPSHOT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "rpotato-observability-read-{}-{}-{}",
            std::process::id(),
            now_ms(),
            sequence
        ));
        match fs::create_dir(&path) {
            Ok(()) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).map_err(
                        |error| {
                            AppError::runtime(format!(
                                "observability read-only snapshot directory mode 실패: {error}"
                            ))
                        },
                    )?;
                }
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(AppError::runtime(format!(
                    "observability read-only snapshot directory 생성 실패: {error}"
                )))
            }
        }
    }
    Err(AppError::runtime(
        "observability read-only snapshot directory 이름 충돌",
    ))
}

fn write_private_snapshot_file(path: &std::path::Path, bytes: &[u8]) -> Result<(), AppError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|error| {
        AppError::runtime(format!(
            "observability read-only snapshot file 생성 실패: {error}"
        ))
    })?;
    file.write_all(bytes).map_err(|error| {
        AppError::runtime(format!(
            "observability read-only snapshot file write 실패: {error}"
        ))
    })?;
    file.sync_all().map_err(|error| {
        AppError::runtime(format!(
            "observability read-only snapshot file fsync 실패: {error}"
        ))
    })
}

fn companion_path(path: &std::path::Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}
