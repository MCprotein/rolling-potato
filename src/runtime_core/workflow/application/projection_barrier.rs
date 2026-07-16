//! Projection-lag admission policy for interrupted workflow recovery.

use std::path::{Path, PathBuf};

use crate::foundation::error::AppError;

pub(crate) trait ProjectionBarrierRecoveryPort {
    fn lag_exists(&self) -> bool;

    fn lag_temp_exists(&self) -> bool;

    fn target_is_converged(&self) -> Result<bool, AppError>;

    fn install_lag(&self) -> Result<PathBuf, AppError>;

    fn repair_required(&self, lag: &Path) -> AppError;

    fn resume_recovery(&mut self) -> Result<(), AppError>;
}

pub(crate) fn recover_through_projection_barrier(
    port: &mut impl ProjectionBarrierRecoveryPort,
) -> Result<(), AppError> {
    if !port.lag_exists() && (port.lag_temp_exists() || !port.target_is_converged()?) {
        let lag = port.install_lag()?;
        return Err(port.repair_required(&lag));
    }
    port.resume_recovery()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    struct FakeProjectionBarrierPort {
        lag_exists: bool,
        lag_temp_exists: bool,
        target_is_converged: bool,
        calls: RefCell<Vec<&'static str>>,
    }

    impl ProjectionBarrierRecoveryPort for FakeProjectionBarrierPort {
        fn lag_exists(&self) -> bool {
            self.calls.borrow_mut().push("lag-exists");
            self.lag_exists
        }

        fn lag_temp_exists(&self) -> bool {
            self.calls.borrow_mut().push("lag-temp-exists");
            self.lag_temp_exists
        }

        fn target_is_converged(&self) -> Result<bool, AppError> {
            self.calls.borrow_mut().push("target-is-converged");
            Ok(self.target_is_converged)
        }

        fn install_lag(&self) -> Result<PathBuf, AppError> {
            self.calls.borrow_mut().push("install-lag");
            Ok(PathBuf::from("projection-lag.json"))
        }

        fn repair_required(&self, _lag: &Path) -> AppError {
            self.calls.borrow_mut().push("repair-required");
            AppError::blocked("projection repair required")
        }

        fn resume_recovery(&mut self) -> Result<(), AppError> {
            self.calls.borrow_mut().push("resume-recovery");
            Ok(())
        }
    }

    #[test]
    fn preserves_uncertain_recovery_before_replay() {
        let mut port = FakeProjectionBarrierPort {
            lag_exists: false,
            lag_temp_exists: false,
            target_is_converged: false,
            calls: RefCell::new(Vec::new()),
        };

        assert!(recover_through_projection_barrier(&mut port).is_err());

        assert_eq!(
            *port.calls.borrow(),
            [
                "lag-exists",
                "lag-temp-exists",
                "target-is-converged",
                "install-lag",
                "repair-required",
            ]
        );
    }

    #[test]
    fn durable_marker_allows_idempotent_recovery_without_rechecking_target() {
        let mut port = FakeProjectionBarrierPort {
            lag_exists: true,
            lag_temp_exists: true,
            target_is_converged: false,
            calls: RefCell::new(Vec::new()),
        };

        recover_through_projection_barrier(&mut port).unwrap();

        assert_eq!(*port.calls.borrow(), ["lag-exists", "resume-recovery"]);
    }
}
