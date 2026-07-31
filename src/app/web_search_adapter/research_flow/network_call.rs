//! Cancellation-aware boundary around blocking web transport.

use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use crate::foundation::error::AppError;

const CANCELLATION_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Debug)]
pub(super) enum WebNetworkCallError {
    Cancelled(AppError),
    TimedOut,
    Transport(AppError),
}

impl WebNetworkCallError {
    pub(super) fn into_app_error(self) -> AppError {
        match self {
            Self::Cancelled(error) | Self::Transport(error) => error,
            Self::TimedOut => elapsed_budget_error(),
        }
    }
}

pub(super) fn run<T, F, C>(
    budget: Duration,
    cancellation_checkpoint: &C,
    operation: F,
) -> Result<T, WebNetworkCallError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
    C: Fn() -> Result<(), AppError> + ?Sized,
{
    cancellation_checkpoint().map_err(WebNetworkCallError::Cancelled)?;
    if budget.is_zero() {
        return Err(WebNetworkCallError::TimedOut);
    }

    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("rpotato-web-transport".to_string())
        .spawn(move || {
            let _ = sender.send(operation());
        })
        .map_err(|_| {
            WebNetworkCallError::Transport(AppError::runtime(
                "웹 transport 작업 thread를 시작하지 못했습니다.",
            ))
        })?;

    let started = Instant::now();
    loop {
        cancellation_checkpoint().map_err(WebNetworkCallError::Cancelled)?;
        let Some(remaining) = budget.checked_sub(started.elapsed()) else {
            return Err(WebNetworkCallError::TimedOut);
        };
        if remaining.is_zero() {
            return Err(WebNetworkCallError::TimedOut);
        }
        match receiver.recv_timeout(remaining.min(CANCELLATION_POLL_INTERVAL)) {
            Ok(result) => return result.map_err(WebNetworkCallError::Transport),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                return Err(WebNetworkCallError::Transport(AppError::runtime(
                    "웹 transport 작업이 결과 없이 종료되었습니다.",
                )));
            }
        }
    }
}

fn elapsed_budget_error() -> AppError {
    AppError::blocked("웹 리서치 시간 상한에 도달했습니다.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    #[test]
    fn returns_transport_result_inside_budget() {
        let result = run(Duration::from_secs(1), &|| Ok(()), || Ok("done"));

        assert_eq!(result.unwrap(), "done");
    }

    #[test]
    fn cancellation_stops_waiting_for_blocking_transport() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_flag = Arc::clone(&cancelled);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            worker_flag.store(true, Ordering::Release);
        });
        let started = Instant::now();

        let error = run(
            Duration::from_secs(1),
            &|| {
                if cancelled.load(Ordering::Acquire) {
                    Err(AppError::runtime("요청을 취소했습니다."))
                } else {
                    Ok(())
                }
            },
            || {
                std::thread::sleep(Duration::from_millis(250));
                Ok(())
            },
        )
        .unwrap_err();

        assert_eq!(error.into_app_error().message, "요청을 취소했습니다.");
        assert!(started.elapsed() < Duration::from_millis(180));
    }

    #[test]
    fn elapsed_budget_stops_waiting_for_blocking_transport() {
        let started = Instant::now();

        let error = run(Duration::from_millis(30), &|| Ok(()), || {
            std::thread::sleep(Duration::from_millis(250));
            Ok(())
        })
        .unwrap_err();

        assert_eq!(
            error.into_app_error().message,
            "웹 리서치 시간 상한에 도달했습니다."
        );
        assert!(started.elapsed() < Duration::from_millis(180));
    }
}
