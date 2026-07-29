//! Plugin manifest parsing and default-deny capability policy.

mod capabilities;
mod json;
mod parsing;
mod security;

pub(crate) use capabilities::{
    apply_claude_manifest_semantics, apply_manifest_risk_markers, blocked_permissions,
    blocked_permissions_from_json, capability_summary, capability_summary_from_json,
    display_capabilities, display_vec, finalize_permissions, finalize_unsupported,
    is_unsupported_plugin_asset, push_capability, push_unique, push_unsupported_capability,
    PluginCapability,
};
pub(crate) use json::{
    extract_json_string_array, extract_json_string_field, required_field, required_usize,
};
pub(crate) use parsing::{
    claude_instruction_unsupported, contains_claude_dynamic_shell, parse_claude_instruction,
    parse_codex_skill,
};
pub(crate) use security::{
    reject_path_traversal, reject_remote_or_marketplace, slug, validate_component_name,
    validate_plugin_id,
};
