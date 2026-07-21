use std::io::IsTerminal;

pub(crate) const HELP: &str = "\
rpotato — local coding agent

사용법:
  rpotato                 기본 TUI 시작
  rpotato init            첫 실행 설정 다시 열기
  rpotato doctor          환경 진단
  rpotato install         binary 설치 및 PATH 등록
  rpotato uninstall --clean --yes
  rpotato --help

TUI 명령:
  /model [id]             모델 확인 또는 변경
  /compact                현재 대화 컨텍스트 압축
  /status                 모델·컨텍스트·backend·세션 상태
  /sessions               세션 목록
  /doctor                 환경 진단
  /more, /back            긴 응답 페이지 이동
  /clear                  현재 응답 지우기
  /help                   TUI 도움말
  /quit                   종료

자동화:
  rpotato run \"<request>\"
  rpotato resume [session-id]
  rpotato continue [session-id]
  rpotato cancel

고급 진단·호환 명령:
  rpotato debug --help

일반 사용자는 backend 경로, GGUF 경로, model registry 명령을 직접 다룰 필요가 없습니다.";

pub(crate) const ADVANCED_HELP: &str = "\
rpotato debug — 고급 진단·자동화·호환 명령

아래의 기존 직접 명령은 호환성을 위해 유지됩니다. rpotato debug <명령...>으로도
같이 실행할 수 있습니다.

rpotato

사용법:
  rpotato
  rpotato --help
  rpotato doctor
  rpotato install
  rpotato install --clean --dry-run
  rpotato install --clean --yes
  rpotato init
  rpotato run \"<request>\"
  rpotato intent classify \"<request>\"
  rpotato intent routes
  rpotato config
  rpotato state
  rpotato state reconcile
  rpotato state resume
  rpotato session list
  rpotato session history
  rpotato session resume <session-id>
  rpotato session new
  rpotato team status
  rpotato team plan --manifest <project-relative-json>
  rpotato team execute --team <team-id>
  rpotato team reconcile --team <team-id>
  rpotato team cancel --team <team-id>
  rpotato team admit --lanes <count> [--write <path>] [--write-owner <lane:path>] [--command <command>]
  rpotato team dispatch --lanes <count> --write-owner <lane:path> [--failed-lane <lane>] [--failure <reason>]
  rpotato team governor --lanes <count> --context-tokens <tokens> [--context-limit <tokens>] [--model-tier small|standard|large]
  rpotato subagent launch --role <role> --task <text> --tool <tool> --read <path> [--tool <tool>] [--read <path>] [--write <path>] [--timeout-ms <ms>] [--max-tokens <tokens>]
  rpotato subagent status [subagent-id]
  rpotato subagent cancel <subagent-id>
  rpotato resume [session-id]
  rpotato continue [session-id]
  rpotato tui
  rpotato tui interactive
  rpotato tui monitor
  rpotato tui sessions
  rpotato tui transcript <session-id>
  rpotato tui approvals
  rpotato tui diff <proposal-id>
  rpotato tui evidence
  rpotato cancel
  rpotato evidence validate <artifact-pointer>
  rpotato skill list
  rpotato skill run <id> \"<request>\"
  rpotato policy schema
  rpotato policy check-command <command>
  rpotato policy check-path --read <path>
  rpotato policy check-path --write <path>
  rpotato policy redact <text>
  rpotato hooks list
  rpotato hooks validate-result <json>
  rpotato patch preview --path <path> --find <text> --replace <text>
  rpotato patch approve <proposal-id> --token <token> [--dry-run]
  rpotato patch verify <proposal-id> --token <token>
  rpotato patch token-rotate <proposal-id>
  rpotato backend doctor
  rpotato backend install-plan
  rpotato backend install
  rpotato backend start [--model <path>] [--ctx-size <tokens>]
  rpotato backend status
  rpotato backend stop
  rpotato backend cancel
  rpotato backend verify-archive <path> --sha256 <hash>
  rpotato backend health-check
  rpotato backend chat --prompt <text> [--max-tokens <tokens>] [--stream] [--timeout-ms <ms>]
  rpotato cache status
  rpotato monitor status
  rpotato monitor models
  rpotato monitor baseline
  rpotato monitor optimize
  rpotato monitor export --format jsonl
  rpotato monitor export --format csv
  rpotato monitor export --format html
  rpotato monitor prune --before 30d --dry-run
  rpotato ontology status
  rpotato ontology seed
  rpotato ontology inspect
  rpotato ontology context --query <text>
  rpotato ontology reread <source-pointer>
  rpotato ontology export --format json
  rpotato ontology export --format jsonl
  rpotato ontology import --file <path> --dry-run
  rpotato benchmark validate <fixture.json>
  rpotato benchmark record --fixture <fixture.json>
  rpotato benchmark run --fixture <fixture.json> --prompt <artifact> [--max-tokens <tokens>]
  rpotato benchmark report --format jsonl
  rpotato model list
  rpotato model manifest
  rpotato model inspect <id>
  rpotato model registry
  rpotato model default [<id>]
  rpotato model download-plan <id>
  rpotato model eval-plan <id>
  rpotato model benchmark-plan <id>
  rpotato model fetch-candidate <id> --for-evaluation
  rpotato model verify-file <path> --sha256 <hash>
  rpotato model promote <id> --evidence <file>
  rpotato model cleanup-failed <id> --dry-run
  rpotato model install <id>
  rpotato plugin import --from codex <local-path> --dry-run
  rpotato plugin import --from claude-code <local-path> --dry-run
  rpotato plugin list
  rpotato plugin inspect <id>
  rpotato plugin validate <id>
  rpotato plugin enable <id>
  rpotato plugin disable <id>
  rpotato plugin remove <id> --keep-data
  rpotato plugin remove <id> --purge-data
  rpotato uninstall --keep-cache
  rpotato uninstall --purge-cache
  rpotato uninstall --dry-run --purge-cache
  rpotato uninstall --clean --dry-run
  rpotato uninstall --clean --yes

patch workflow 규칙:
  run이 만든 proposal은 verification plan을 미리 binding합니다.
  patch approve는 patch만 적용하고 patch verify는 별도 승인 후 command를 실행합니다.
  state resume은 pending approval에서 backend를 다시 호출하지 않습니다.
  verification command는 proposal에 binding되며 CLI에서 바꿀 수 없습니다.

현재 상태:
  install은 사용자 전용 binary와 PATH를 멱등 등록하고, clean install/uninstall은 dry-run/명시적 확인 및 active runtime 차단을 요구합니다.
  backend install은 source-backed manifest와 SHA-256 검증을 거친 뒤 관리형 release payload를 배치합니다.
  backend start/status/stop/chat/cancel은 managed sidecar lifecycle, SSE chat streaming, generation 취소를 다룹니다.
  team status는 최신 resource sample 기준의 read-only admission preview와 sequential fallback 결정을 표시합니다.
  team plan은 canonical team manifest를 active parent workflow에 binding하고 durable team-plan state를 기록합니다.
  team execute는 durable team plan의 모든 member를 resource pressure에 따라 병렬 또는 순차 실행합니다.
  team reconcile은 complete worker set과 evidence를 검증해 parent에 원자적으로 merge하고 stop gate를 통과시킵니다.
  team cancel은 durable marker를 기록해 active team worker 전체에 취소를 전파합니다.
  team admit은 dispatcher 진입 전 resource/policy/file-ownership admission gate를 강제하고 결과를 ledger에 기록합니다.
  team dispatch는 dispatch 직전 file ownership을 다시 강제하고 failed-worker continuation 상태를 ledger에 기록합니다.
  team governor는 dispatcher 진입 전 context/model budget clamp와 downgrade/escalation hint를 기록합니다.
  benchmark record는 metadata-only not-comparable run을 기록하고, benchmark run은 실행 중인 backend sidecar로 local measured run을 기록합니다.
  monitor optimize는 측정된 local metric과 benchmark evidence만으로 context/lane/fallback/model route hint를 추천합니다.
  ontology store는 project-local typed graph JSONL을 canonical runtime store로 두고, source-pointer-first compact context view와 원문 reread rule을 제공합니다.
  모델 registry install은 source-backed manifest와 local promotion evidence가 검증되기 전까지 차단되며, 검증용 artifact fetch는 --for-evaluation을 요구합니다.";

const RESET: &str = "\x1b[0m";
const BOLD_CYAN: &str = "\x1b[1;36m";
const BOLD_BLUE: &str = "\x1b[1;34m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";
const DIM: &str = "\x1b[2m";

pub(crate) fn emit_report(report: &str) {
    println!("{}", style_report(report, color_enabled()));
}

fn color_enabled() -> bool {
    std::io::stdout().is_terminal()
        && std::env::var_os("NO_COLOR").is_none()
        && std::env::var_os("TERM").as_deref() != Some(std::ffi::OsStr::new("dumb"))
}

pub(crate) fn style_report(report: &str, color: bool) -> String {
    if !color {
        return report.to_string();
    }

    let first_content = report.lines().position(|line| !line.trim().is_empty());
    report
        .lines()
        .enumerate()
        .map(|(index, line)| style_line(line, Some(index) == first_content))
        .collect::<Vec<_>>()
        .join("\n")
}

fn style_line(line: &str, first_content: bool) -> String {
    if line.trim().is_empty() {
        return String::new();
    }
    if first_content {
        return format!("{BOLD_CYAN}{line}{RESET}");
    }

    let trimmed = line.trim();
    if !line.starts_with(char::is_whitespace) && trimmed.ends_with(':') {
        return format!("{BOLD_BLUE}{line}{RESET}");
    }

    let normalized = trimmed.to_ascii_lowercase();
    let semantic = if contains_any(
        &normalized,
        &[
            "failed", "failure", "error", "blocked", "stale", "실패", "차단", "오류",
        ],
    ) {
        Some(RED)
    } else if contains_any(
        &normalized,
        &["warning", "waiting", "degraded", "pending", "경고", "대기"],
    ) {
        Some(YELLOW)
    } else if contains_any(
        &normalized,
        &[
            "ready",
            "running",
            "completed",
            "healthy",
            "verified",
            "passed",
            "준비",
            "실행 중",
            "완료",
            "정상",
            "검증됨",
        ],
    ) {
        Some(GREEN)
    } else if normalized.starts_with("hint:")
        || normalized.starts_with("next:")
        || normalized.starts_with("참고:")
    {
        Some(DIM)
    } else {
        None
    };

    semantic
        .map(|style| format!("{style}{line}{RESET}"))
        .unwrap_or_else(|| line.to_string())
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::{
        style_report, ADVANCED_HELP, BOLD_BLUE, BOLD_CYAN, GREEN, HELP, RED, RESET, YELLOW,
    };

    #[test]
    fn public_help_keeps_granular_commands_out_of_the_primary_surface() {
        assert!(HELP.contains("rpotato                 기본 TUI 시작"));
        assert!(HELP.contains("rpotato debug --help"));
        assert!(!HELP.contains("rpotato backend start"));
        assert!(ADVANCED_HELP.contains("rpotato backend start"));
    }

    #[test]
    fn plain_report_is_byte_stable_when_color_is_disabled() {
        let report = "rpotato doctor\n\n상태:\n- backend: ready\n";
        assert_eq!(style_report(report, false), report);
    }

    #[test]
    fn color_report_marks_structure_and_semantic_states() {
        let report = "rpotato doctor\n\n상태:\n- backend: ready\n- gate: waiting\n- run: failed";
        let styled = style_report(report, true);

        assert!(styled.contains(&format!("{BOLD_CYAN}rpotato doctor{RESET}")));
        assert!(styled.contains(&format!("{BOLD_BLUE}상태:{RESET}")));
        assert!(styled.contains(&format!("{GREEN}- backend: ready{RESET}")));
        assert!(styled.contains(&format!("{YELLOW}- gate: waiting{RESET}")));
        assert!(styled.contains(&format!("{RED}- run: failed{RESET}")));
    }
}
