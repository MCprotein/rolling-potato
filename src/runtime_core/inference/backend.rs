use std::path::PathBuf;

pub(crate) mod admission;
pub(crate) mod lifecycle;

pub(crate) const MAX_CHAT_TIMEOUT_MS: u32 = 300_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BackendChatImage {
    pub(crate) display_name: String,
    pub(crate) mime_type: String,
    pub(crate) sha256: String,
    pub(crate) bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BackendChatInput {
    pub(crate) text: String,
    pub(crate) images: Vec<BackendChatImage>,
}

impl BackendChatInput {
    pub(crate) fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            images: Vec::new(),
        }
    }
}

pub(crate) trait BackendAdapter {
    fn id(&self) -> &'static str;
    fn binary_name(&self) -> &'static str;
    fn managed_binary_path(&self) -> PathBuf;
    fn default_host(&self) -> &'static str;
    fn default_port(&self) -> u16;
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BackendChatSampling {
    pub(crate) temperature: f64,
    pub(crate) top_p: f64,
}

impl BackendChatSampling {
    pub(crate) fn ledger_label(&self) -> String {
        format!("temperature-{}_top-p-{}", self.temperature, self.top_p)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BackendChatRun {
    pub(crate) backend_id: String,
    pub(crate) backend_version: String,
    pub(crate) pid: u32,
    pub(crate) model_id: String,
    pub(crate) model_path: PathBuf,
    pub(crate) model_artifact_hash: String,
    pub(crate) ctx_size: Option<u32>,
    pub(crate) prompt_chars: usize,
    pub(crate) response_chars: usize,
    pub(crate) requested_max_tokens: u32,
    pub(crate) effective_max_tokens: u32,
    pub(crate) sampling: BackendChatSampling,
    pub(crate) finish_reason: String,
    pub(crate) guard_status: &'static str,
    pub(crate) prompt_tokens: Option<u32>,
    pub(crate) completion_tokens: Option<u32>,
    pub(crate) total_tokens: Option<u32>,
    pub(crate) elapsed_ms: u128,
    pub(crate) first_token_latency_ms: Option<u128>,
    pub(crate) streaming_display: bool,
    pub(crate) ledger_event: String,
    pub(crate) resource_governor_admission: String,
    pub(crate) resource_governor_token_action: String,
    pub(crate) resource_governor_reason: &'static str,
    pub(crate) resource_governor_hint: &'static str,
    pub(crate) resource_governor_sample_event: String,
    pub(crate) resource_pressure: String,
    pub(crate) resource_cpu_percent: Option<f64>,
    pub(crate) resource_average_rss_bytes: Option<u64>,
    pub(crate) resource_peak_rss_bytes: Option<u64>,
    pub(crate) resource_disk_bytes: Option<u64>,
    pub(crate) resource_sample_event: String,
    pub(crate) response: String,
}
