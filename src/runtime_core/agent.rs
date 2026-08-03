//! Surface-neutral decisions for one model turn.
//!
//! The model may choose a bounded tool call or return a visible answer, but it
//! never selects an application adapter or performs a side effect directly.

use crate::foundation::error::AppError;
use crate::foundation::serialization as strict_json;

const MAX_TOOL_INPUT_CHARS: usize = 512;
const DECISION_CONTEXT: &str = "agent turn decision";
const MAX_FOLLOW_UP_MODEL_TURNS: u8 = 3;
const MAX_TOOL_CALLS: u8 = 3;

pub(crate) const TURN_DECISION_JSON_SCHEMA: &str = r#"{"type":"object","properties":{"decision":{"type":"string","enum":["answer","web_search","web_open","web_find","local_task"]},"input":{"type":"string","maxLength":512},"answer":{"type":"string"}},"required":["decision","input","answer"],"additionalProperties":false}"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AgentTurnDecision {
    Answer(String),
    Tool(AgentToolCall),
    ContinueLocal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentToolCall {
    pub(crate) name: AgentToolName,
    pub(crate) input: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentToolName {
    Search,
    Open,
    Find,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentLoopStop {
    FollowUpTurnBudget,
    RepeatedToolCall,
    ToolCallBudget,
}

#[derive(Debug, Default)]
pub(crate) struct BoundedAgentLoop {
    follow_up_model_turns: u8,
    tool_calls: u8,
    admitted_tool_calls: Vec<AgentToolCall>,
}

impl BoundedAgentLoop {
    pub(crate) fn begin_follow_up_model_turn(&mut self) -> Result<(), AgentLoopStop> {
        if self.follow_up_model_turns >= MAX_FOLLOW_UP_MODEL_TURNS {
            return Err(AgentLoopStop::FollowUpTurnBudget);
        }
        self.follow_up_model_turns += 1;
        Ok(())
    }

    pub(crate) fn admit_tool_call(
        &mut self,
        tool_call: AgentToolCall,
    ) -> Result<(), AgentLoopStop> {
        if self
            .admitted_tool_calls
            .iter()
            .any(|admitted| admitted == &tool_call)
        {
            return Err(AgentLoopStop::RepeatedToolCall);
        }
        if self.tool_calls >= MAX_TOOL_CALLS {
            return Err(AgentLoopStop::ToolCallBudget);
        }
        self.tool_calls += 1;
        self.admitted_tool_calls.push(tool_call);
        Ok(())
    }
}

pub(crate) fn parse_turn_decision(
    candidate: &str,
    allow_direct_answer: bool,
) -> Result<AgentTurnDecision, AppError> {
    let object = strict_json::parse_object(
        candidate.trim(),
        &["decision", "input", "answer"],
        DECISION_CONTEXT,
    )?;
    let decision = strict_json::string(&object, "decision", DECISION_CONTEXT)?;
    let input = strict_json::string(&object, "input", DECISION_CONTEXT)?;
    let answer = strict_json::string(&object, "answer", DECISION_CONTEXT)?;
    ensure_bounded(&input, MAX_TOOL_INPUT_CHARS, "input")?;
    ensure_visible_answer(&answer)?;

    match decision.as_str() {
        "answer" if allow_direct_answer && !answer.trim().is_empty() => {
            Ok(AgentTurnDecision::Answer(answer))
        }
        "answer" | "local_task" => Ok(AgentTurnDecision::ContinueLocal),
        "web_search" => tool(AgentToolName::Search, input),
        "web_open" => tool(AgentToolName::Open, input),
        "web_find" => tool(AgentToolName::Find, input),
        _ => Err(AppError::blocked(format!(
            "agent turn decision 차단\n- 이유: 지원하지 않는 decision입니다: {decision}"
        ))),
    }
}

fn tool(name: AgentToolName, input: String) -> Result<AgentTurnDecision, AppError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(AppError::blocked(
            "agent turn decision 차단\n- 이유: tool input이 비어 있습니다.",
        ));
    }
    Ok(AgentTurnDecision::Tool(AgentToolCall {
        name,
        input: input.to_string(),
    }))
}

fn ensure_bounded(value: &str, max_chars: usize, field: &str) -> Result<(), AppError> {
    if value.chars().count() > max_chars || value.chars().any(char::is_control) {
        return Err(AppError::blocked(format!(
            "agent turn decision 차단\n- 이유: {field}가 허용 범위를 벗어났습니다."
        )));
    }
    Ok(())
}

fn ensure_visible_answer(value: &str) -> Result<(), AppError> {
    if value
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Err(AppError::blocked(
            "agent turn decision 차단\n- 이유: answer에 표시할 수 없는 control character가 있습니다.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_answer_and_bounded_tool_calls() {
        assert_eq!(
            parse_turn_decision(
                r#"{"decision":"answer","input":"","answer":"대한민국의 수도는 서울입니다."}"#,
                true
            )
            .unwrap(),
            AgentTurnDecision::Answer("대한민국의 수도는 서울입니다.".to_string())
        );
        assert_eq!(
            parse_turn_decision(
                r#"{"decision":"web_search","input":"2026 월드컵 우승 국가","answer":""}"#,
                true
            )
            .unwrap(),
            AgentTurnDecision::Tool(AgentToolCall {
                name: AgentToolName::Search,
                input: "2026 월드컵 우승 국가".to_string(),
            })
        );
    }

    #[test]
    fn local_task_and_disabled_direct_answer_continue_in_the_runtime() {
        assert_eq!(
            parse_turn_decision(r#"{"decision":"local_task","input":"","answer":""}"#, true)
                .unwrap(),
            AgentTurnDecision::ContinueLocal
        );
        assert_eq!(
            parse_turn_decision(
                r#"{"decision":"answer","input":"","answer":"직접 답변"}"#,
                false
            )
            .unwrap(),
            AgentTurnDecision::ContinueLocal
        );
    }

    #[test]
    fn rejects_unknown_fields_actions_empty_tools_and_oversized_input() {
        for candidate in [
            r#"{"decision":"shell","input":"curl example.com","answer":""}"#,
            r#"{"decision":"web_search","input":"","answer":""}"#,
            r#"{"decision":"answer","input":"","answer":"답","secret":"leak"}"#,
        ] {
            assert!(parse_turn_decision(candidate, true).is_err(), "{candidate}");
        }
        let oversized = format!(
            r#"{{"decision":"web_search","input":"{}","answer":""}}"#,
            "x".repeat(MAX_TOOL_INPUT_CHARS + 1)
        );
        assert!(parse_turn_decision(&oversized, true).is_err());
        assert!(TURN_DECISION_JSON_SCHEMA.contains(r#""answer":{"type":"string"}"#));
        assert!(!TURN_DECISION_JSON_SCHEMA.contains(r#""answer":{"type":"string","maxLength":"#));
    }

    #[test]
    fn visible_answer_length_is_owned_by_generation_and_protocol_capacity() {
        let answer = format!("긴 답변 시작\n{}\n긴 답변 끝", "가".repeat(32 * 1024));
        let candidate = format!(
            r#"{{"decision":"answer","input":"","answer":"{}"}}"#,
            answer.replace('\n', r"\n")
        );

        assert_eq!(
            parse_turn_decision(&candidate, true).unwrap(),
            AgentTurnDecision::Answer(answer)
        );
    }

    #[test]
    fn bounded_loop_allows_progress_but_stops_repetition_and_runaway_turns() {
        let search = AgentToolCall {
            name: AgentToolName::Search,
            input: "Rust stable".to_string(),
        };
        let mut repeated = BoundedAgentLoop::default();
        assert_eq!(repeated.admit_tool_call(search.clone()), Ok(()));
        assert_eq!(
            repeated.admit_tool_call(search),
            Err(AgentLoopStop::RepeatedToolCall)
        );

        let mut non_consecutive = BoundedAgentLoop::default();
        let search = AgentToolCall {
            name: AgentToolName::Search,
            input: "Rust stable".to_string(),
        };
        assert_eq!(non_consecutive.admit_tool_call(search.clone()), Ok(()));
        assert_eq!(
            non_consecutive.admit_tool_call(AgentToolCall {
                name: AgentToolName::Open,
                input: "https://example.com/rust".to_string(),
            }),
            Ok(())
        );
        assert_eq!(
            non_consecutive.admit_tool_call(search),
            Err(AgentLoopStop::RepeatedToolCall)
        );

        let mut turns = BoundedAgentLoop::default();
        for _ in 0..MAX_FOLLOW_UP_MODEL_TURNS {
            assert_eq!(turns.begin_follow_up_model_turn(), Ok(()));
        }
        assert_eq!(
            turns.begin_follow_up_model_turn(),
            Err(AgentLoopStop::FollowUpTurnBudget)
        );

        let mut tools = BoundedAgentLoop::default();
        for index in 0..MAX_TOOL_CALLS {
            assert_eq!(
                tools.admit_tool_call(AgentToolCall {
                    name: AgentToolName::Search,
                    input: format!("query-{index}"),
                }),
                Ok(())
            );
        }
        assert_eq!(
            tools.admit_tool_call(AgentToolCall {
                name: AgentToolName::Open,
                input: "https://example.com".to_string(),
            }),
            Err(AgentLoopStop::ToolCallBudget)
        );
    }
}
