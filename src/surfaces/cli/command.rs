pub(crate) use crate::runtime_core::inference::benchmark::report::BenchmarkReportFormat;
pub(crate) use crate::runtime_core::inference::resource::ModelTier;
pub(crate) use crate::runtime_core::knowledge::ontology::OntologyExportFormat;

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Help,
    Install(InstallCommand),
    Init,
    Run { request: String },
    Intent(IntentCommand),
    Doctor,
    Config,
    State(StateCommand),
    Session(SessionCommand),
    Team(TeamCommand),
    Subagent(SubagentCommand),
    Tui(TuiCommand),
    Cancel,
    Evidence(EvidenceCommand),
    Skill(SkillCommand),
    Policy(PolicyCommand),
    Hooks(HooksCommand),
    Patch(PatchCommand),
    Backend(BackendCommand),
    CacheStatus,
    Monitor(MonitorCommand),
    Ontology(OntologyCommand),
    Benchmark(BenchmarkCommand),
    Model(ModelCommand),
    Plugin(PluginCommand),
    Uninstall(UninstallCommand),
}

#[derive(Debug, PartialEq, Eq)]
pub enum InstallCommand {
    Standard,
    CleanDryRun,
    CleanConfirmed,
}

#[derive(Debug, PartialEq, Eq)]
pub enum MonitorCommand {
    Status,
    Models,
    Baseline,
    Optimize,
    Export { format: MonitorExportFormat },
    Prune { before_days: u64, dry_run: bool },
}

#[derive(Debug, PartialEq, Eq)]
pub enum BenchmarkCommand {
    Validate {
        path: String,
    },
    Record {
        fixture: String,
    },
    Run {
        fixture: String,
        prompt: String,
        max_tokens: Option<u32>,
    },
    Report {
        format: BenchmarkReportFormat,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum OntologyCommand {
    Status,
    Seed,
    Inspect,
    Context { query: String },
    Reread { pointer: String },
    Export { format: OntologyExportFormat },
    Import { path: String, dry_run: bool },
}

#[derive(Debug, PartialEq, Eq)]
pub enum StateCommand {
    Status,
    Reconcile,
    Resume,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SessionCommand {
    List,
    New,
    Resume { id: String },
}

#[derive(Debug, PartialEq, Eq)]
pub enum TeamCommand {
    Status,
    Plan {
        manifest_path: String,
    },
    Execute {
        team_id: String,
    },
    Reconcile {
        team_id: String,
    },
    Cancel {
        team_id: String,
    },
    Admit {
        lanes: u32,
        write_paths: Vec<String>,
        owned_write_paths: Vec<(u32, String)>,
        commands: Vec<String>,
    },
    Dispatch {
        lanes: u32,
        owned_write_paths: Vec<(u32, String)>,
        failed_lane: Option<u32>,
        failure_reason: Option<String>,
    },
    Governor {
        lanes: u32,
        context_tokens: u32,
        context_limit: Option<u32>,
        model_tier: ModelTier,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum SubagentCommand {
    Launch {
        role: String,
        task: String,
        tools: Vec<String>,
        read_paths: Vec<String>,
        write_paths: Vec<String>,
        timeout_ms: Option<u32>,
        max_tokens: Option<u32>,
    },
    Status {
        id: Option<String>,
    },
    Cancel {
        id: String,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum TuiCommand {
    Auto,
    Interactive,
    Monitor,
    Sessions,
    Transcript { session_id: String },
    Approvals,
    Diff { proposal_id: String },
    Evidence,
}

#[derive(Debug, PartialEq, Eq)]
pub enum EvidenceCommand {
    Validate { pointer: String },
}

#[derive(Debug, PartialEq, Eq)]
pub enum SkillCommand {
    List,
    Run { id: String, request: String },
}

#[derive(Debug, PartialEq, Eq)]
pub enum IntentCommand {
    Classify { request: String },
    Routes,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PolicyCommand {
    Schema,
    CheckCommand { command: String },
    CheckPath { mode: PolicyPathMode, path: String },
    Redact { text: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyPathMode {
    Read,
    Write,
}

#[derive(Debug, PartialEq, Eq)]
pub enum HooksCommand {
    List,
    ValidateResult { json: String },
}

#[derive(Debug, PartialEq, Eq)]
pub enum PatchCommand {
    Preview {
        path: String,
        find: String,
        replace: String,
    },
    Approve {
        proposal_id: String,
        token: String,
        dry_run: bool,
    },
    Verify {
        proposal_id: String,
        token: String,
    },
    TokenRotate {
        proposal_id: String,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum BackendCommand {
    Doctor,
    InstallPlan,
    Install,
    Start {
        model_path: Option<String>,
        ctx_size: Option<u32>,
    },
    Status,
    Stop,
    Cancel,
    VerifyArchive {
        path: String,
        sha256: String,
    },
    HealthCheck,
    Chat {
        prompt: String,
        max_tokens: Option<u32>,
        stream: bool,
        timeout_ms: Option<u32>,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum MonitorExportFormat {
    Jsonl,
    Csv,
    Html,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ModelCommand {
    List,
    Manifest,
    Inspect { id: String },
    Registry,
    Default,
    SetDefault { id: String },
    DownloadPlan { id: String },
    EvalPlan { id: String },
    BenchmarkPlan { id: String },
    FetchCandidate { id: String },
    VerifyFile { path: String, sha256: String },
    Promote { id: String, evidence: String },
    CleanupFailed { id: String, dry_run: bool },
    Install { id: String },
}

#[derive(Debug, PartialEq, Eq)]
pub enum PluginCommand {
    Import {
        source: PluginSource,
        path: String,
        dry_run: bool,
    },
    List,
    Inspect {
        id: String,
    },
    Validate {
        id: String,
    },
    Enable {
        id: String,
    },
    Disable {
        id: String,
    },
    Remove {
        id: String,
        purge_data: bool,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum UninstallCommand {
    Plan { purge_cache: bool, dry_run: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginSource {
    Codex,
    ClaudeCode,
}

impl PluginSource {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "codex" => Some(Self::Codex),
            "claude-code" => Some(Self::ClaudeCode),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::ClaudeCode => "claude-code",
        }
    }
}
