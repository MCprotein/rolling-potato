#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TuiModelOption {
    pub(crate) id: String,
    pub(crate) display_name: String,
    pub(crate) quantization: String,
    pub(crate) download_bytes: u64,
    pub(crate) model_cached: bool,
    pub(crate) vision_projector_bytes: Option<u64>,
    pub(crate) vision_projector_cached: bool,
    pub(crate) context_length: Option<u32>,
    pub(crate) ram: String,
    pub(crate) license: String,
    pub(crate) note: String,
    pub(crate) current: bool,
    pub(crate) recommended: bool,
}

impl TuiModelOption {
    pub(crate) fn model_artifact_label(&self) -> String {
        if self.model_cached {
            "local cache · 적용 시 SHA-256 검증".to_string()
        } else {
            format!("download {}", gib_label(self.download_bytes))
        }
    }

    pub(crate) fn vision_artifact_label(&self) -> String {
        match (self.vision_projector_bytes, self.vision_projector_cached) {
            (Some(_), true) => "on-demand · projector cache 준비됨".to_string(),
            (Some(bytes), false) => format!(
                "on-demand · 첫 이미지에서 projector {} 자동 준비",
                gib_label(bytes)
            ),
            (None, _) => "unsupported".to_string(),
        }
    }
}

fn gib_label(bytes: u64) -> String {
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    format!("{:.1} GiB", bytes as f64 / GIB)
}
