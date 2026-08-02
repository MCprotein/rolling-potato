#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TuiModelReadiness {
    StaticVerified,
    LocalPromotionReady,
    EvaluationOnly,
}

impl TuiModelReadiness {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::StaticVerified => "manifest 검증 완료",
            Self::LocalPromotionReady => "이 호스트 실사용 검증 완료",
            Self::EvaluationOnly => "평가 전용 · 실사용 검증 미완료",
        }
    }

    pub(crate) fn is_runtime_ready(self) -> bool {
        matches!(self, Self::StaticVerified | Self::LocalPromotionReady)
    }
}

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
    pub(crate) evaluation_recommended: bool,
    pub(crate) readiness: TuiModelReadiness,
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
