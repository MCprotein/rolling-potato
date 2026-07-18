//! Cross-process exclusion for runtime start and destructive clean transitions.

use std::path::{Path, PathBuf};

use crate::adapters::filesystem::{layout, lease::RecoverableLease};
use crate::foundation::error::AppError;

pub(crate) fn acquire(context: &str) -> Result<RecoverableLease, AppError> {
    RecoverableLease::acquire(lock_path_for_app_data(&layout::app_data_root()), context)
}

fn lock_path_for_app_data(app_data: &Path) -> PathBuf {
    let leaf = app_data
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("rpotato");
    app_data.with_file_name(format!(".{leaf}.runtime-mutation.lock"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn runtime_mutation_lock_lives_outside_cleaned_app_data_and_excludes_peers() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "rpotato-runtime-mutation-{}-{nonce}",
            std::process::id()
        ));
        let app_data = root.join("rpotato");
        let lock_path = lock_path_for_app_data(&app_data);

        assert_eq!(
            lock_path.parent(),
            app_data.parent(),
            "clean deletion must not remove its own synchronization primitive"
        );
        assert!(!lock_path.starts_with(&app_data));

        let first = RecoverableLease::acquire(lock_path.clone(), "runtime mutation test").unwrap();
        let blocked = match RecoverableLease::acquire(lock_path.clone(), "runtime mutation test") {
            Ok(_) => panic!("second runtime mutation lease acquired"),
            Err(err) => err,
        };
        assert_eq!(blocked.code, 3);
        drop(first);
        let second = RecoverableLease::acquire(lock_path.clone(), "runtime mutation test").unwrap();
        drop(second);
        let _ = fs::remove_dir_all(root);
    }
}
