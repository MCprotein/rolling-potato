#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkillManifest {
    pub id: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    pub mode: &'static str,
    pub required_hooks: &'static [&'static str],
    pub allowed_tools: &'static [&'static str],
    pub context_requirements: &'static [&'static str],
    pub evidence_requirements: &'static [&'static str],
    pub stop_criteria: &'static [&'static str],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedSkillManifest {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub instructions: String,
    pub plugin_id: String,
    pub source_path: String,
    pub source_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedSkillManifest {
    Builtin(&'static SkillManifest),
    Imported(ImportedSkillManifest),
}

const IMPORTED_SKILL_TOOLS: &[&str] = &["read_file"];
const IMPORTED_SKILL_CONTEXT: &[&str] = &["repo_root"];
const IMPORTED_SKILL_EVIDENCE: &[&str] = &["plugin_capability_admission"];
const IMPORTED_SKILL_STOP: &[&str] = &["plugin_capability_completed", "korean_report_passed"];

impl ResolvedSkillManifest {
    pub fn id(&self) -> &str {
        match self {
            Self::Builtin(manifest) => manifest.id,
            Self::Imported(manifest) => &manifest.id,
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            Self::Builtin(manifest) => manifest.display_name,
            Self::Imported(manifest) => &manifest.display_name,
        }
    }

    pub fn description(&self) -> &str {
        match self {
            Self::Builtin(manifest) => manifest.description,
            Self::Imported(manifest) => &manifest.description,
        }
    }

    pub fn mode(&self) -> &'static str {
        match self {
            Self::Builtin(manifest) => manifest.mode,
            Self::Imported(_) => "read-only",
        }
    }

    pub fn required_hooks(&self) -> &'static [&'static str] {
        match self {
            Self::Builtin(manifest) => manifest.required_hooks,
            Self::Imported(_) => READ_ONLY_HOOKS,
        }
    }

    pub fn allowed_tools(&self) -> &'static [&'static str] {
        match self {
            Self::Builtin(manifest) => manifest.allowed_tools,
            Self::Imported(_) => IMPORTED_SKILL_TOOLS,
        }
    }

    pub fn context_requirements(&self) -> &'static [&'static str] {
        match self {
            Self::Builtin(manifest) => manifest.context_requirements,
            Self::Imported(_) => IMPORTED_SKILL_CONTEXT,
        }
    }

    pub fn evidence_requirements(&self) -> &'static [&'static str] {
        match self {
            Self::Builtin(manifest) => manifest.evidence_requirements,
            Self::Imported(_) => IMPORTED_SKILL_EVIDENCE,
        }
    }

    pub fn stop_criteria(&self) -> &'static [&'static str] {
        match self {
            Self::Builtin(manifest) => manifest.stop_criteria,
            Self::Imported(_) => IMPORTED_SKILL_STOP,
        }
    }

    pub fn instructions(&self) -> &str {
        match self {
            Self::Builtin(manifest) => manifest.description,
            Self::Imported(manifest) => &manifest.instructions,
        }
    }

    pub fn imported(&self) -> Option<&ImportedSkillManifest> {
        match self {
            Self::Builtin(_) => None,
            Self::Imported(manifest) => Some(manifest),
        }
    }
}

pub const READ_ONLY_HOOKS: &[&str] = &[
    "session_start",
    "user_request_received",
    "pre_context_pack",
    "post_context_pack",
    "pre_model_request",
    "post_model_response",
    "pre_action_parse",
    "post_action_parse",
    "pre_final_report",
    "stop_gate",
    "session_end",
];

pub const EXECUTE_HOOKS: &[&str] = &[
    "session_start",
    "user_request_received",
    "pre_context_pack",
    "post_context_pack",
    "pre_model_request",
    "post_model_response",
    "pre_action_parse",
    "post_action_parse",
    "pre_tool_call",
    "post_tool_result",
    "pre_patch_apply",
    "post_patch_apply",
    "pre_command_run",
    "post_command_run",
    "pre_final_report",
    "stop_gate",
    "session_end",
];
