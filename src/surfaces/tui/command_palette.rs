use crate::runtime_core::terminal::TerminalSuggestion;

const COMMANDS: &[TerminalSuggestion] = &[
    TerminalSuggestion {
        command: "/model [id]",
        description: "모델 확인 및 변경",
    },
    TerminalSuggestion {
        command: "/compact",
        description: "현재 대화 컨텍스트 압축",
    },
    TerminalSuggestion {
        command: "/search <질문>",
        description: "인터넷 검색 후 출처와 함께 답변",
    },
    TerminalSuggestion {
        command: "/update",
        description: "최신 버전 확인 및 업데이트",
    },
    TerminalSuggestion {
        command: "/status",
        description: "모델·backend·세션 상태 새로고침",
    },
    TerminalSuggestion {
        command: "/sessions",
        description: "세션 목록 열기",
    },
    TerminalSuggestion {
        command: "/doctor",
        description: "환경 진단",
    },
    TerminalSuggestion {
        command: "/chat",
        description: "대화 화면 열기",
    },
    TerminalSuggestion {
        command: "/more",
        description: "긴 응답 다음 페이지",
    },
    TerminalSuggestion {
        command: "/back",
        description: "긴 응답 이전 페이지",
    },
    TerminalSuggestion {
        command: "/clear",
        description: "현재 대화 지우기",
    },
    TerminalSuggestion {
        command: "/help",
        description: "명령 도움말",
    },
    TerminalSuggestion {
        command: "/quit",
        description: "rpotato 종료",
    },
];

pub(crate) fn commands() -> &'static [TerminalSuggestion] {
    COMMANDS
}

pub(crate) fn help_notice() -> String {
    let mut lines = vec!["요청을 바로 입력하세요.".to_string()];
    lines.extend(
        COMMANDS
            .iter()
            .map(|entry| format!("- {}: {}", entry.command, entry.description)),
    );
    lines.push("고급 호환 명령: rpotato debug --help".to_string());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_and_help_share_one_command_registry() {
        let help = help_notice();
        for entry in commands() {
            assert!(help.contains(entry.command));
            assert!(help.contains(entry.description));
        }
    }
}
