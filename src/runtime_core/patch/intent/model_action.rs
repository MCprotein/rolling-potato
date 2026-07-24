//! Fail-closed parsing of model-reported action metadata.

use crate::runtime_core::knowledge::context::ContextPack;

use super::{ActionCandidate, ParsedModelAction};

pub(crate) fn parse_model_action(
    response: &str,
    runtime_candidate: &ActionCandidate,
    context_pack: &ContextPack,
) -> ParsedModelAction {
    let Some(fields) = parse_model_action_fields(response) else {
        if runtime_candidate.kind == "answer-only" {
            return fallback_model_action("runtime-owned-answer", runtime_candidate);
        }
        return parse_model_action_text(response, runtime_candidate, context_pack).unwrap_or_else(
            || fallback_model_action("missing-model-action-line", runtime_candidate),
        );
    };
    let raw_kind = field_value(&fields, &["kind"]).unwrap_or_default();
    let Some(parsed_kind) = normalize_model_action_kind(&raw_kind) else {
        return fallback_model_action("unknown-model-action-kind", runtime_candidate);
    };
    let raw_side_effects = field_value(&fields, &["side_effects", "allowed_side_effects"])
        .unwrap_or_else(|| runtime_candidate.allowed_side_effects.to_string());
    let side_effects = normalize_side_effects(&raw_side_effects);
    if side_effects != "none" {
        let mut blocked = fallback_model_action("blocked-side-effect-request", runtime_candidate);
        blocked.requested_side_effects = side_effects;
        return blocked;
    }
    if parsed_kind != runtime_candidate.kind {
        return fallback_model_action("mismatch-runtime-fallback", runtime_candidate);
    }

    let raw_source_pointers =
        field_value(&fields, &["source_pointers", "sources"]).unwrap_or_else(|| "none".to_string());
    let raw_next_gate = field_value(&fields, &["next_gate"])
        .unwrap_or_else(|| runtime_candidate.next_gate.to_string());

    ParsedModelAction {
        status: "parsed",
        kind: parsed_kind.to_string(),
        source_pointers: normalize_source_pointers(&raw_source_pointers, context_pack),
        next_gate: normalize_next_gate(&raw_next_gate, runtime_candidate),
        requested_side_effects: side_effects,
        executable_now: false,
        target_path: field_value(&fields, &["path", "target_path"]).unwrap_or_default(),
        find_text: decode_action_text(field_value(&fields, &["find_hex"]).as_deref()),
        replace_text: decode_action_text(field_value(&fields, &["replace_hex"]).as_deref()),
        verification_command: field_value(&fields, &["verification", "verification_command"])
            .unwrap_or_default(),
    }
}

fn parse_model_action_text(
    response: &str,
    runtime_candidate: &ActionCandidate,
    context_pack: &ContextPack,
) -> Option<ParsedModelAction> {
    let parsed_kind = normalize_model_action_kind(response)?;
    if parsed_kind != runtime_candidate.kind {
        return Some(fallback_model_action(
            "heuristic-runtime-fallback",
            runtime_candidate,
        ));
    }

    Some(ParsedModelAction {
        status: "heuristic-text",
        kind: parsed_kind.to_string(),
        source_pointers: source_pointers_from_text(response, context_pack),
        next_gate: runtime_candidate.next_gate.to_string(),
        requested_side_effects: runtime_candidate.allowed_side_effects.to_string(),
        executable_now: false,
        target_path: String::new(),
        find_text: String::new(),
        replace_text: String::new(),
        verification_command: String::new(),
    })
}

fn parse_model_action_fields(response: &str) -> Option<Vec<(String, String)>> {
    let line = response.lines().rev().find_map(model_action_body)?;
    let fields = line
        .split(';')
        .filter_map(|part| {
            let (key, value) = part.split_once('=')?;
            let key = key.trim().to_ascii_lowercase().replace('-', "_");
            let value = value.trim().to_string();
            if key.is_empty() {
                None
            } else {
                Some((key, value))
            }
        })
        .collect::<Vec<_>>();

    if fields.is_empty() {
        None
    } else {
        Some(fields)
    }
}

pub(crate) fn model_action_body(line: &str) -> Option<&str> {
    let trimmed = line
        .trim()
        .strip_prefix("- ")
        .unwrap_or_else(|| line.trim())
        .trim()
        .trim_matches('`');
    if let Some((prefix, body)) = trimmed.split_once(':') {
        let normalized_prefix = prefix.trim().to_ascii_lowercase();
        if normalized_prefix == "model action" || prefix.trim() == "모델액션" {
            return Some(body.trim());
        }
    }
    None
}

fn field_value(fields: &[(String, String)], names: &[&str]) -> Option<String> {
    fields
        .iter()
        .find(|(key, _)| names.iter().any(|name| key == name))
        .map(|(_, value)| value.clone())
}

fn normalize_model_action_kind(value: &str) -> Option<&'static str> {
    let lower = value.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return None;
    }
    if lower == "patch-proposal" || lower.contains("patch") || value.contains("패치") {
        Some("patch-proposal")
    } else if lower == "inspect-sources"
        || lower.contains("inspect")
        || lower.contains("source")
        || value.contains("소스")
        || value.contains("원본")
    {
        Some("inspect-sources")
    } else if lower == "generated-artifact-plan"
        || lower.contains("artifact")
        || lower.contains("generate")
        || value.contains("문서")
        || value.contains("생성")
    {
        Some("generated-artifact-plan")
    } else if lower == "answer-only" || lower.contains("answer") || value.contains("답변") {
        Some("answer-only")
    } else {
        None
    }
}

fn normalize_source_pointers(value: &str, context_pack: &ContextPack) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("none")
        || trimmed == "없음"
        || trimmed == "-"
    {
        return "none".to_string();
    }

    let verified = trimmed
        .split(',')
        .map(str::trim)
        .filter(|pointer| {
            context_pack
                .source_pointers
                .iter()
                .any(|source| source.stable_ref == *pointer)
        })
        .take(4)
        .map(str::to_string)
        .collect::<Vec<_>>();

    if verified.is_empty() {
        "unverified".to_string()
    } else {
        verified.join(", ")
    }
}

fn source_pointers_from_text(response: &str, context_pack: &ContextPack) -> String {
    let pointers = context_pack
        .source_pointers
        .iter()
        .filter(|source| response.contains(&source.stable_ref))
        .take(4)
        .map(|source| source.stable_ref.clone())
        .collect::<Vec<_>>();

    if pointers.is_empty() {
        "none".to_string()
    } else {
        pointers.join(", ")
    }
}

fn normalize_next_gate(value: &str, runtime_candidate: &ActionCandidate) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "-" {
        runtime_candidate.next_gate.to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalize_side_effects(value: &str) -> String {
    let trimmed = value.trim().trim_end_matches('.');
    if trimmed.eq_ignore_ascii_case("none") || trimmed == "없음" || trimmed == "-" {
        "none".to_string()
    } else {
        trimmed.to_string()
    }
}

fn fallback_model_action(
    status: &'static str,
    runtime_candidate: &ActionCandidate,
) -> ParsedModelAction {
    ParsedModelAction {
        status,
        kind: runtime_candidate.kind.to_string(),
        source_pointers: "none".to_string(),
        next_gate: runtime_candidate.next_gate.to_string(),
        requested_side_effects: runtime_candidate.allowed_side_effects.to_string(),
        executable_now: false,
        target_path: String::new(),
        find_text: String::new(),
        replace_text: String::new(),
        verification_command: String::new(),
    }
}

fn decode_action_text(value: Option<&str>) -> String {
    let Some(value) = value else {
        return String::new();
    };
    if !value.len().is_multiple_of(2) {
        return String::new();
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    for pair in value.as_bytes().chunks_exact(2) {
        let Ok(hex) = std::str::from_utf8(pair) else {
            return String::new();
        };
        let Ok(byte) = u8::from_str_radix(hex, 16) else {
            return String::new();
        };
        bytes.push(byte);
    }
    String::from_utf8(bytes).unwrap_or_default()
}
