use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BenchmarkFixture {
    pub(crate) path: PathBuf,
    pub(crate) sha256: String,
    pub(crate) fixture_id: String,
    pub(crate) benchmark_name: String,
    pub(crate) runtime_capability_under_test: String,
    pub(crate) model_vs_runtime_responsibility: String,
    pub(crate) expected_route: String,
    pub(crate) expected_policy_decision: String,
    pub(crate) expected_escalation_target: String,
    pub(crate) required_tools: Vec<String>,
    pub(crate) required_source_reads: Vec<String>,
    pub(crate) required_evidence_records: Vec<String>,
    pub(crate) abstention_required: bool,
    pub(crate) expected_failure_category: String,
    pub(crate) ontology_view: String,
    pub(crate) context_budget: u32,
    pub(crate) model_id: String,
    pub(crate) model_artifact_hash: String,
    pub(crate) quantization: String,
    pub(crate) backend_id: String,
    pub(crate) backend_version: String,
    pub(crate) dataset_ref: String,
    pub(crate) prompt_runtime_version: String,
    pub(crate) tool_policy_version: String,
    pub(crate) seed_policy: String,
    pub(crate) sampling_options: String,
    pub(crate) raw_artifact_retention_policy: String,
    pub(crate) expected_response_contains: Vec<String>,
    pub(crate) forbidden_response_contains: Vec<String>,
    pub(crate) minimum_score: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BenchmarkPromptArtifact {
    pub(crate) path: PathBuf,
    pub(crate) sha256: String,
    pub(crate) text: String,
    pub(crate) chars: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum FixtureJsonValue {
    String(String),
    U32(u32),
    Bool(bool),
    StringArray(Vec<String>),
}
