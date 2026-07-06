use crate::backend;
use crate::cache;
use crate::cli::{
    BackendCommand, Command, EvidenceCommand, HooksCommand, IntentCommand, ModelCommand,
    MonitorCommand, PluginCommand, PolicyCommand, PolicyPathMode, SessionCommand, SkillCommand,
    StateCommand, UninstallCommand,
};
use crate::config;
use crate::evidence;
use crate::hooks;
use crate::intent;
use crate::model;
use crate::monitor;
use crate::plugin;
use crate::policy;
use crate::runtime;
use crate::skill;
use crate::state;
use crate::uninstall;

#[derive(Debug, PartialEq, Eq)]
pub struct AppError {
    pub code: u8,
    pub message: String,
}

impl AppError {
    pub fn runtime(message: impl Into<String>) -> Self {
        Self {
            code: 1,
            message: message.into(),
        }
    }

    pub fn usage(message: impl Into<String>) -> Self {
        Self {
            code: 2,
            message: message.into(),
        }
    }

    pub fn blocked(message: impl Into<String>) -> Self {
        Self {
            code: 3,
            message: message.into(),
        }
    }
}

pub fn run(args: impl IntoIterator<Item = String>) -> Result<(), AppError> {
    match crate::cli::parse(args)? {
        Command::Help => {
            println!("{}", crate::cli::HELP);
            Ok(())
        }
        Command::Init => {
            println!("{}", runtime::init_report()?);
            Ok(())
        }
        Command::Run { request } => {
            println!("{}", intent::run_report(&request)?);
            Ok(())
        }
        Command::Intent(IntentCommand::Classify { request }) => {
            println!("{}", intent::classify_report(&request)?);
            Ok(())
        }
        Command::Intent(IntentCommand::Routes) => {
            println!("{}", intent::routes_report());
            Ok(())
        }
        Command::Doctor => {
            println!("{}", runtime::doctor_report());
            Ok(())
        }
        Command::Config => {
            println!("{}", config::report());
            Ok(())
        }
        Command::State(StateCommand::Status) => {
            println!("{}", state::status_report()?);
            Ok(())
        }
        Command::State(StateCommand::Reconcile) => {
            println!("{}", state::reconcile_report()?);
            Ok(())
        }
        Command::State(StateCommand::Resume) => {
            println!("{}", state::resume_report()?);
            Ok(())
        }
        Command::Session(SessionCommand::List) => {
            println!("{}", state::session_list_report()?);
            Ok(())
        }
        Command::Session(SessionCommand::New) => {
            println!("{}", state::session_new_report()?);
            Ok(())
        }
        Command::Session(SessionCommand::Resume { id }) => {
            println!("{}", state::session_resume_report(&id)?);
            Ok(())
        }
        Command::Cancel => {
            println!("{}", state::cancel_report()?);
            Ok(())
        }
        Command::Evidence(EvidenceCommand::Validate { pointer }) => {
            println!("{}", evidence::validate_report(&pointer)?);
            Ok(())
        }
        Command::Skill(SkillCommand::List) => {
            println!("{}", skill::list_report());
            Ok(())
        }
        Command::Skill(SkillCommand::Run { id }) => {
            println!("{}", skill::run_report(&id)?);
            Ok(())
        }
        Command::Policy(PolicyCommand::Schema) => {
            println!("{}", policy::schema_report());
            Ok(())
        }
        Command::Policy(PolicyCommand::CheckCommand { command }) => {
            println!("{}", policy::check_command_report(&command)?);
            Ok(())
        }
        Command::Policy(PolicyCommand::CheckPath { mode, path }) => {
            let mode = match mode {
                PolicyPathMode::Read => policy::PathMode::Read,
                PolicyPathMode::Write => policy::PathMode::Write,
            };
            println!("{}", policy::check_path_report(mode, &path)?);
            Ok(())
        }
        Command::Policy(PolicyCommand::Redact { text }) => {
            println!("{}", policy::redact_report(&text));
            Ok(())
        }
        Command::Hooks(HooksCommand::List) => {
            println!("{}", hooks::list_report());
            Ok(())
        }
        Command::Hooks(HooksCommand::ValidateResult { json }) => {
            println!("{}", hooks::validate_result_report(&json)?);
            Ok(())
        }
        Command::Backend(BackendCommand::Doctor) => {
            println!("{}", backend::doctor_report());
            Ok(())
        }
        Command::Backend(BackendCommand::InstallPlan) => {
            println!("{}", backend::install_plan_report());
            Ok(())
        }
        Command::Backend(BackendCommand::Install) => {
            println!("{}", backend::install_report()?);
            Ok(())
        }
        Command::Backend(BackendCommand::Start {
            model_path,
            ctx_size,
        }) => {
            println!("{}", backend::start_report(&model_path, ctx_size)?);
            Ok(())
        }
        Command::Backend(BackendCommand::Status) => {
            println!("{}", backend::status_report()?);
            Ok(())
        }
        Command::Backend(BackendCommand::Stop) => {
            println!("{}", backend::stop_report()?);
            Ok(())
        }
        Command::Backend(BackendCommand::VerifyArchive { path, sha256 }) => {
            println!("{}", backend::verify_archive_report(&path, &sha256)?);
            Ok(())
        }
        Command::Backend(BackendCommand::HealthCheck) => {
            println!("{}", backend::health_check_report());
            Ok(())
        }
        Command::Backend(BackendCommand::Chat { prompt, max_tokens }) => {
            println!("{}", backend::chat_report(&prompt, max_tokens)?);
            Ok(())
        }
        Command::CacheStatus => {
            println!("{}", cache::status_report());
            Ok(())
        }
        Command::Monitor(MonitorCommand::Status) => {
            println!("{}", monitor::status_report()?);
            Ok(())
        }
        Command::Monitor(MonitorCommand::Models) => {
            println!("{}", monitor::models_report()?);
            Ok(())
        }
        Command::Monitor(MonitorCommand::Export { format }) => {
            print!("{}", monitor::export_report(format)?);
            Ok(())
        }
        Command::Monitor(MonitorCommand::Prune {
            before_days,
            dry_run,
        }) => {
            println!("{}", monitor::prune_report(before_days, dry_run)?);
            Ok(())
        }
        Command::Model(ModelCommand::List) => {
            println!("{}", model::list_report());
            Ok(())
        }
        Command::Model(ModelCommand::Manifest) => {
            println!("{}", model::manifest_report());
            Ok(())
        }
        Command::Model(ModelCommand::Inspect { id }) => {
            println!("{}", model::inspect_report(&id)?);
            Ok(())
        }
        Command::Model(ModelCommand::Registry) => {
            println!("{}", model::registry_report());
            Ok(())
        }
        Command::Model(ModelCommand::DownloadPlan { id }) => {
            println!("{}", model::download_plan_report(&id)?);
            Ok(())
        }
        Command::Model(ModelCommand::EvalPlan { id }) => {
            println!("{}", model::eval_plan_report(&id)?);
            Ok(())
        }
        Command::Model(ModelCommand::BenchmarkPlan { id }) => {
            println!("{}", model::benchmark_plan_report(&id)?);
            Ok(())
        }
        Command::Model(ModelCommand::FetchCandidate { id }) => {
            println!("{}", model::fetch_candidate_for_evaluation_report(&id)?);
            Ok(())
        }
        Command::Model(ModelCommand::VerifyFile { path, sha256 }) => {
            println!("{}", model::verify_file_report(&path, &sha256)?);
            Ok(())
        }
        Command::Model(ModelCommand::CleanupFailed { id, dry_run }) => {
            println!("{}", model::cleanup_failed_report(&id, dry_run)?);
            Ok(())
        }
        Command::Model(ModelCommand::Install { id }) => model::install_candidate(&id),
        Command::Plugin(PluginCommand::Import {
            source,
            path,
            dry_run,
        }) => {
            println!("{}", plugin::import_report(source, &path, dry_run)?);
            Ok(())
        }
        Command::Plugin(PluginCommand::List) => {
            println!("{}", plugin::list_report());
            Ok(())
        }
        Command::Plugin(PluginCommand::Inspect { id }) => {
            println!("{}", plugin::inspect_report(&id)?);
            Ok(())
        }
        Command::Plugin(PluginCommand::Validate { id }) => {
            println!("{}", plugin::validate_report(&id)?);
            Ok(())
        }
        Command::Plugin(PluginCommand::Enable { id }) => {
            println!("{}", plugin::set_enabled_report(&id, true)?);
            Ok(())
        }
        Command::Plugin(PluginCommand::Disable { id }) => {
            println!("{}", plugin::set_enabled_report(&id, false)?);
            Ok(())
        }
        Command::Plugin(PluginCommand::Remove { id, purge_data }) => {
            println!("{}", plugin::remove_report(&id, purge_data)?);
            Ok(())
        }
        Command::Uninstall(UninstallCommand::Plan {
            purge_cache,
            dry_run,
        }) => {
            println!("{}", uninstall::plan_report(purge_cache, dry_run));
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_command_returns_usage_error() {
        let err = run(["wat".to_string()]).unwrap_err();
        assert_eq!(err.code, 2);
        assert!(err.message.contains("알 수 없는 명령"));
    }

    #[test]
    fn unverified_model_install_is_blocked() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root =
            std::env::temp_dir().join(format!("rpotato-model-install-test-{}", std::process::id()));
        let project_root = root.join("project");
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);

        let err = run([
            "model".to_string(),
            "install".to_string(),
            "qwen3.5-4b".to_string(),
        ])
        .unwrap_err();

        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");

        assert_eq!(err.code, 3);
        assert!(err.message.contains("설치를 차단했습니다"));
        assert!(err.message.contains("verified 상태로 승격"));
    }

    #[test]
    fn remote_plugin_import_is_rejected() {
        let err = run([
            "plugin".to_string(),
            "import".to_string(),
            "--from".to_string(),
            "codex".to_string(),
            "https://example.com/plugin.git".to_string(),
            "--dry-run".to_string(),
        ])
        .unwrap_err();

        assert_eq!(err.code, 3);
        assert!(err.message.contains("remote URL"));
    }

    #[test]
    fn init_command_reports_layout_without_error() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!("rpotato-init-test-{}", std::process::id()));
        let project_root = root.join("project");
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);

        let result = run(["init".to_string()]);

        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn run_requires_active_backend_sidecar() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root =
            std::env::temp_dir().join(format!("rpotato-run-blocked-test-{}", std::process::id()));
        let project_root = root.join("project");
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);

        let err = run([
            "run".to_string(),
            "테스트".to_string(),
            "고쳐줘".to_string(),
        ])
        .unwrap_err();

        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");

        assert_eq!(err.code, 3);
        assert!(err.message.contains("backend chat 차단"));
        assert!(err.message.contains("sidecar record"));
    }
}
