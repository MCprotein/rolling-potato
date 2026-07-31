//! Source-backed model behavior and local setup evidence.

use super::{ModelGenerationProfile, ModelSetupProfile, ModelThinkingControl, SourceClaim};

pub(super) const QWEN_4B_GENERATION: ModelGenerationProfile = ModelGenerationProfile {
    sampling: None,
    thinking_control: ModelThinkingControl::ChatTemplateEnableThinkingFalse {
        source: SourceClaim {
            claim: "Qwen3.5 documents instruct/non-thinking mode through enable_thinking=false.",
            source: "https://huggingface.co/Qwen/Qwen3.5-4B#instruct-or-non-thinking-mode",
            checked_at: "2026-07-30",
            status: "confirmed",
        },
    },
};

pub(super) const GEMMA_4B_GENERATION: ModelGenerationProfile = ModelGenerationProfile {
    sampling: None,
    thinking_control: ModelThinkingControl::ModelDefault,
};

pub(super) const QWEN_4B_SETUP: ModelSetupProfile = ModelSetupProfile {
    recommended: false,
    adoption: SourceClaim {
        claim: "실험적 선택; local v0.30.0 adoption smoke exact-response equality 실패",
        source: "docs/model-eval.md#current-local-execution-evidence",
        checked_at: "2026-07-11",
        status: "measured-locally",
    },
};

pub(super) const GEMMA_4B_SETUP: ModelSetupProfile = ModelSetupProfile {
    recommended: true,
    adoption: SourceClaim {
        claim: "로컬 adoption smoke 통과; 16 GB 적합성은 미확정",
        source: "docs/model-eval.md#current-local-execution-evidence",
        checked_at: "2026-07-11",
        status: "measured-locally",
    },
};
