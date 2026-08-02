//! Cancellation-aware boundary around blocking web transport.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use crate::foundation::error::AppError;

const CANCELLATION_POLL_INTERVAL: Duration = Duration::from_millis(50);
const WEB_WORKER_COUNT: usize = 2;
const QUEUED_JOBS_PER_WORKER: usize = 1;

type Job = Box<dyn FnOnce() + Send + 'static>;

#[derive(Debug)]
pub(super) enum WebNetworkCallError {
    Cancelled(AppError),
    TimedOut,
    Saturated,
    Transport(AppError),
}

impl WebNetworkCallError {
    pub(super) fn into_app_error(self) -> AppError {
        match self {
            Self::Cancelled(error) | Self::Transport(error) => error,
            Self::TimedOut => elapsed_budget_error(),
            Self::Saturated => AppError::blocked(
                "웹 transport 작업이 이미 상한까지 실행 중입니다. 잠시 후 다시 시도하세요.",
            ),
        }
    }
}

struct WebWorkerPool {
    senders: Vec<SyncSender<Job>>,
    next_worker: AtomicUsize,
    stats: Arc<WorkerStats>,
}

struct WorkerStats {
    submitted: AtomicUsize,
    completed: AtomicUsize,
    active: AtomicUsize,
    peak_active: AtomicUsize,
}

impl WebWorkerPool {
    fn start() -> Result<Self, AppError> {
        let mut senders = Vec::with_capacity(WEB_WORKER_COUNT);
        let stats = Arc::new(WorkerStats {
            submitted: AtomicUsize::new(0),
            completed: AtomicUsize::new(0),
            active: AtomicUsize::new(0),
            peak_active: AtomicUsize::new(0),
        });
        for index in 0..WEB_WORKER_COUNT {
            let (sender, receiver) = mpsc::sync_channel::<Job>(QUEUED_JOBS_PER_WORKER);
            let worker_stats = Arc::clone(&stats);
            std::thread::Builder::new()
                .name(format!("rpotato-web-transport-{index}"))
                .spawn(move || {
                    while let Ok(job) = receiver.recv() {
                        let active = worker_stats.active.fetch_add(1, Ordering::AcqRel) + 1;
                        worker_stats.peak_active.fetch_max(active, Ordering::AcqRel);
                        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job));
                        worker_stats.active.fetch_sub(1, Ordering::AcqRel);
                        worker_stats.completed.fetch_add(1, Ordering::AcqRel);
                    }
                })
                .map_err(|_| AppError::runtime("웹 transport worker를 시작하지 못했습니다."))?;
            senders.push(sender);
        }
        Ok(Self {
            senders,
            next_worker: AtomicUsize::new(0),
            stats,
        })
    }

    fn submit(&self, job: Job) -> Result<(), WebNetworkCallError> {
        let start = self.next_worker.fetch_add(1, Ordering::Relaxed) % self.senders.len();
        let mut pending = Some(job);
        for offset in 0..self.senders.len() {
            let index = (start + offset) % self.senders.len();
            match self.senders[index].try_send(pending.take().expect("job is present")) {
                Ok(()) => {
                    self.stats.submitted.fetch_add(1, Ordering::AcqRel);
                    return Ok(());
                }
                Err(TrySendError::Full(job)) | Err(TrySendError::Disconnected(job)) => {
                    pending = Some(job);
                }
            }
        }
        Err(WebNetworkCallError::Saturated)
    }
}

fn worker_pool() -> Result<&'static WebWorkerPool, WebNetworkCallError> {
    static POOL: OnceLock<Result<WebWorkerPool, String>> = OnceLock::new();
    POOL.get_or_init(|| WebWorkerPool::start().map_err(|error| error.message))
        .as_ref()
        .map_err(|message| WebNetworkCallError::Transport(AppError::runtime(message.clone())))
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
    worker_pool()?.submit(Box::new(move || {
        let _ = sender.send(operation());
    }))?;

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
        Arc, Mutex, OnceLock,
    };

    fn test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    fn wait_for_completion(target: usize) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while worker_pool()
            .unwrap()
            .stats
            .completed
            .load(Ordering::Acquire)
            < target
        {
            assert!(
                Instant::now() < deadline,
                "managed web jobs did not complete"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn returns_transport_result_inside_budget() {
        let _guard = test_lock();
        let result = run(Duration::from_secs(1), &|| Ok(()), || Ok("done"));

        assert_eq!(result.unwrap(), "done");
    }

    #[test]
    fn cancellation_stops_waiting_while_managed_transport_completes() {
        let _guard = test_lock();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_flag = Arc::clone(&cancelled);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            worker_flag.store(true, Ordering::Release);
        });
        let pool = worker_pool().unwrap();
        let completed_before = pool.stats.completed.load(Ordering::Acquire);
        let submitted_before = pool.stats.submitted.load(Ordering::Acquire);
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
                std::thread::sleep(Duration::from_millis(80));
                Ok(())
            },
        )
        .unwrap_err();

        assert_eq!(error.into_app_error().message, "요청을 취소했습니다.");
        assert!(started.elapsed() < Duration::from_millis(70));
        assert_eq!(
            pool.stats.submitted.load(Ordering::Acquire),
            submitted_before + 1
        );
        wait_for_completion(completed_before + 1);
    }

    #[test]
    fn timed_out_calls_use_a_bounded_pool_and_finish_managed_jobs() {
        let _guard = test_lock();
        let pool = worker_pool().unwrap();
        wait_for_completion(pool.stats.submitted.load(Ordering::Acquire));
        let submitted_before = pool.stats.submitted.load(Ordering::Acquire);
        let completed_before = pool.stats.completed.load(Ordering::Acquire);
        let mut saturated = 0;

        for _ in 0..12 {
            let result = run(Duration::from_millis(1), &|| Ok(()), || {
                std::thread::sleep(Duration::from_millis(40));
                Ok(())
            });
            if matches!(result, Err(WebNetworkCallError::Saturated)) {
                saturated += 1;
            }
        }

        let admitted = pool
            .stats
            .submitted
            .load(Ordering::Acquire)
            .saturating_sub(submitted_before);
        assert!(admitted <= WEB_WORKER_COUNT * (QUEUED_JOBS_PER_WORKER + 1));
        assert!(saturated > 0);
        wait_for_completion(completed_before + admitted);
        assert!(pool.stats.peak_active.load(Ordering::Acquire) <= WEB_WORKER_COUNT);
        assert_eq!(pool.stats.active.load(Ordering::Acquire), 0);
    }
}
