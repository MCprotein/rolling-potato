use crate::runtime_core::collaboration::subagent_result::{
    EvidenceSourceBinding, SubagentResultV1,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSubagentResult {
    pub result: SubagentResultV1,
    pub result_artifact_id: String,
    pub result_artifact_hash: String,
    pub evidence_id: String,
    pub evidence_hash: String,
    pub(super) result_body: String,
    pub(super) evidence_sources: Vec<EvidenceSourceBinding>,
}
