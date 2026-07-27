use std::env;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

const PROJECT_ROOT_ENV: &str = "RPOTATO_PROJECT_ROOT";
const DATA_HOME_ENV: &str = "RPOTATO_DATA_HOME";

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

pub static ENV_LOCK: TestEnvironmentLock = TestEnvironmentLock::new();

pub fn initialize_runtime_state(
) -> Result<crate::app::workflow_adapter::state::StateInit, crate::foundation::error::AppError> {
    crate::app::workflow_adapter::state::initialize()
}

pub fn record_runtime_event(
    event_type: &str,
    summary: &str,
    details: &str,
) -> Result<String, crate::foundation::error::AppError> {
    crate::app::workflow_adapter::state::record_event(event_type, summary, details)
}

pub struct TestEnvironmentLock {
    inner: Mutex<()>,
}

impl TestEnvironmentLock {
    pub const fn new() -> Self {
        Self {
            inner: Mutex::new(()),
        }
    }

    pub fn lock(&self) -> io::Result<TestEnvironmentGuard<'_>> {
        let mutex = match self.inner.lock() {
            Ok(mutex) => mutex,
            Err(poisoned) => poisoned.into_inner(),
        };
        let saved_project_root = env::var_os(PROJECT_ROOT_ENV);
        let saved_data_home = env::var_os(DATA_HOME_ENV);
        let temp_root = create_unique_temp_root()?;
        let project_root = temp_root.join("project");
        let data_home = temp_root.join("data");

        if let Err(err) = fs::create_dir(&project_root).and_then(|_| fs::create_dir(&data_home)) {
            let _ = fs::remove_dir_all(&temp_root);
            return Err(err);
        }

        env::set_var(PROJECT_ROOT_ENV, &project_root);
        env::set_var(DATA_HOME_ENV, &data_home);

        Ok(TestEnvironmentGuard {
            mutex,
            saved_project_root,
            saved_data_home,
            temp_root,
        })
    }
}

pub struct TestEnvironmentGuard<'a> {
    mutex: MutexGuard<'a, ()>,
    saved_project_root: Option<OsString>,
    saved_data_home: Option<OsString>,
    temp_root: PathBuf,
}

impl Drop for TestEnvironmentGuard<'_> {
    fn drop(&mut self) {
        restore_env(PROJECT_ROOT_ENV, self.saved_project_root.take());
        restore_env(DATA_HOME_ENV, self.saved_data_home.take());
        let _ = fs::remove_dir_all(&self.temp_root);
        let _ = &self.mutex;
    }
}

fn create_unique_temp_root() -> io::Result<PathBuf> {
    loop {
        let sequence = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = env::temp_dir().join(format!(
            "rpotato-test-env-{}-{sequence}",
            std::process::id()
        ));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }
}

fn restore_env(name: &str, value: Option<OsString>) {
    if let Some(value) = value {
        env::set_var(name, value);
    } else {
        env::remove_var(name);
    }
}

pub struct ScriptedTerminal {
    dimensions: std::collections::VecDeque<
        Result<(u16, u16), crate::runtime_core::terminal::TerminalFault>,
    >,
    lines: std::collections::VecDeque<
        Result<Option<String>, crate::runtime_core::terminal::TerminalFault>,
    >,
    secrets: std::collections::VecDeque<
        Result<Option<String>, crate::runtime_core::terminal::TerminalFault>,
    >,
    pub frames: Vec<String>,
    pub frame_fault: Option<crate::runtime_core::terminal::TerminalFault>,
    pub frame_fault_at: Option<(
        crate::runtime_core::terminal::FrameWriteBoundary,
        crate::runtime_core::terminal::TerminalFault,
    )>,
    pub input_events: std::collections::VecDeque<
        Result<
            crate::runtime_core::terminal::TerminalInputEvent,
            crate::runtime_core::terminal::TerminalFault,
        >,
    >,
}

impl ScriptedTerminal {
    pub fn new(lines: impl IntoIterator<Item = &'static str>) -> Self {
        Self {
            dimensions: std::iter::repeat_n(Ok((80, 24)), 64).collect(),
            lines: lines
                .into_iter()
                .map(|line| Ok(Some(line.to_string())))
                .chain(std::iter::once(Ok(None)))
                .collect(),
            secrets: std::collections::VecDeque::new(),
            frames: Vec::new(),
            frame_fault: None,
            frame_fault_at: None,
            input_events: std::collections::VecDeque::new(),
        }
    }
}

impl crate::runtime_core::terminal::TerminalIo for ScriptedTerminal {
    fn dimensions(&mut self) -> Result<(u16, u16), crate::runtime_core::terminal::TerminalFault> {
        self.dimensions.pop_front().unwrap_or(Ok((80, 24)))
    }

    fn read_line(
        &mut self,
    ) -> Result<Option<String>, crate::runtime_core::terminal::TerminalFault> {
        self.lines.pop_front().unwrap_or(Ok(None))
    }

    fn read_secret(
        &mut self,
    ) -> Result<Option<String>, crate::runtime_core::terminal::TerminalFault> {
        self.secrets.pop_front().unwrap_or(Ok(None))
    }

    fn read_input_with_suggestions(
        &mut self,
        _suggestions: &[crate::runtime_core::terminal::TerminalSuggestion],
    ) -> Result<
        crate::runtime_core::terminal::TerminalInputEvent,
        crate::runtime_core::terminal::TerminalFault,
    > {
        use crate::runtime_core::terminal::TerminalInputEvent;

        self.input_events.pop_front().unwrap_or_else(|| {
            self.read_line()
                .map(|line| line.map_or(TerminalInputEvent::End, TerminalInputEvent::Submit))
        })
    }

    fn write_frame(
        &mut self,
        frame: &str,
    ) -> Result<(), crate::runtime_core::terminal::TerminalFault> {
        if let Some(fault) = self.frame_fault.take() {
            return Err(fault);
        }
        self.frames.push(frame.to_string());
        Ok(())
    }

    fn write_frame_at(
        &mut self,
        frame: &str,
        boundary: crate::runtime_core::terminal::FrameWriteBoundary,
    ) -> Result<(), crate::runtime_core::terminal::TerminalFault> {
        if self
            .frame_fault_at
            .is_some_and(|(expected, _)| expected == boundary)
        {
            return Err(self
                .frame_fault_at
                .take()
                .expect("matching boundary fault")
                .1);
        }
        self.write_frame(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn each_acquisition_installs_a_fresh_clean_environment() {
        let first_root = {
            let _guard = ENV_LOCK.lock().unwrap();
            let project = PathBuf::from(env::var_os(PROJECT_ROOT_ENV).unwrap());
            let data = PathBuf::from(env::var_os(DATA_HOME_ENV).unwrap());
            assert!(project.is_dir());
            assert!(data.is_dir());
            fs::write(project.join("marker"), "test").unwrap();
            env::set_var(PROJECT_ROOT_ENV, "overridden-by-test");
            env::remove_var(DATA_HOME_ENV);
            project.parent().unwrap().to_path_buf()
        };
        assert!(!first_root.exists());

        let second_root = {
            let _guard = ENV_LOCK.lock().unwrap();
            let project = PathBuf::from(env::var_os(PROJECT_ROOT_ENV).unwrap());
            let data = PathBuf::from(env::var_os(DATA_HOME_ENV).unwrap());
            assert!(project.is_dir());
            assert!(data.is_dir());
            assert!(!project.join("marker").exists());
            project.parent().unwrap().to_path_buf()
        };
        assert_ne!(first_root, second_root);
        assert!(!second_root.exists());
    }

    #[test]
    fn poisoned_inner_mutex_is_recovered_as_success() {
        let _global_guard = ENV_LOCK.lock().unwrap();
        let outer_project = env::var_os(PROJECT_ROOT_ENV).unwrap();
        let lock = Arc::new(TestEnvironmentLock::new());
        let poisoner = Arc::clone(&lock);
        assert!(std::thread::spawn(move || {
            let _guard = poisoner.inner.lock().unwrap();
            panic!("poison test environment mutex");
        })
        .join()
        .is_err());

        let recovered = lock.lock().unwrap();
        assert_ne!(env::var_os(PROJECT_ROOT_ENV).unwrap(), outer_project);
        drop(recovered);
        assert_eq!(env::var_os(PROJECT_ROOT_ENV).unwrap(), outer_project);
    }
}
