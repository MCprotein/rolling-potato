//! Skill manifest, registry, policy, and lifecycle facade.

mod builtin;
mod lifecycle;
mod manifest;
mod policy;

pub use builtin::{find_skill, BUILTIN_SKILLS};
pub use lifecycle::{SkillRuntimeState, SkillState};
pub use manifest::{ImportedSkillManifest, ResolvedSkillManifest};
pub use policy::{enforce_resolved_context, enforce_resolved_tool};

#[cfg(test)]
pub use manifest::{EXECUTE_HOOKS, READ_ONLY_HOOKS};
