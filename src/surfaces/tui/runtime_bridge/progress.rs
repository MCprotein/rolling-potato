use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TuiRequestProgress {
    Preparing,
    Deciding,
    Searching,
    Opening,
    Finding,
    Answering,
    LocalWork,
    Completed,
}

impl TuiRequestProgress {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Preparing => "준비 중",
            Self::Deciding => "처리 방법 결정 중",
            Self::Searching => "검색 중",
            Self::Opening => "검색 문서 읽는 중",
            Self::Finding => "문서 안에서 근거 찾는 중",
            Self::Answering => "답변 구성 중",
            Self::LocalWork => "로컬 작업 실행 중",
            Self::Completed => "완료",
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct TuiRequestProgressReporter {
    sender: Option<SyncSender<TuiRequestProgress>>,
}

impl TuiRequestProgressReporter {
    pub(crate) fn channel(capacity: usize) -> (Self, Receiver<TuiRequestProgress>) {
        let (sender, receiver) = mpsc::sync_channel(capacity);
        (
            Self {
                sender: Some(sender),
            },
            receiver,
        )
    }

    pub(crate) fn emit(&self, progress: TuiRequestProgress) {
        let Some(sender) = &self.sender else {
            return;
        };
        match sender.try_send(progress) {
            Ok(()) | Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_transport_never_waits_for_a_slow_tui() {
        let (reporter, receiver) = TuiRequestProgressReporter::channel(1);
        reporter.emit(TuiRequestProgress::Preparing);
        reporter.emit(TuiRequestProgress::Searching);

        assert_eq!(receiver.try_recv(), Ok(TuiRequestProgress::Preparing));
        assert!(receiver.try_recv().is_err());
    }
}
