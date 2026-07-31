//! Surface-neutral cooperative cancellation for one active request.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use crate::foundation::error::AppError;

const REQUEST_CANCELLED_MESSAGE: &str = "요청을 취소했습니다.";

#[derive(Clone, Debug, Default)]
pub(crate) struct RequestCancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl RequestCancellationToken {
    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub(crate) fn check(&self) -> Result<(), AppError> {
        if self.is_cancelled() {
            Err(AppError::runtime(REQUEST_CANCELLED_MESSAGE))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clones_observe_the_same_typed_cancellation_state() {
        let token = RequestCancellationToken::default();
        let worker_token = token.clone();

        token.cancel();

        assert!(worker_token.is_cancelled());
        assert_eq!(
            worker_token.check().unwrap_err().message,
            REQUEST_CANCELLED_MESSAGE
        );
    }
}
