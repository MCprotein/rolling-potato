#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StreamTermination {
    Completed,
    Cancelled,
    TimedOut,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum DecodedFinish {
    Stop,
    Length,
    #[default]
    UnknownOrMissing,
}

impl DecodedFinish {
    pub(crate) fn decode(raw: Option<&str>) -> Self {
        match raw {
            Some("stop") => Self::Stop,
            Some("length") => Self::Length,
            Some(_) | None => Self::UnknownOrMissing,
        }
    }

    pub(crate) fn report_label(self) -> &'static str {
        match self {
            Self::Stop => "stop",
            Self::Length => "length",
            Self::UnknownOrMissing => "unknown",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct StreamCompletion {
    pub(crate) content: String,
    pub(crate) decoded_finish: DecodedFinish,
    pub(crate) raw_finish_reason: Option<String>,
    pub(crate) finish_reason: String,
    pub(crate) prompt_tokens: Option<u32>,
    pub(crate) completion_tokens: Option<u32>,
    pub(crate) total_tokens: Option<u32>,
    pub(crate) first_token_latency_ms: Option<u128>,
    pub(crate) had_reasoning_trace: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StreamOutcome {
    pub(crate) termination: StreamTermination,
    pub(crate) completion: StreamCompletion,
}
