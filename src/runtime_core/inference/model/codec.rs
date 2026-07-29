//! Model registry, selection, and promotion evidence codec facade.

mod promotion;
mod registry;
mod render;
mod selection;

pub(crate) use promotion::parse_promotion_evidence;
pub(crate) use registry::parse_registry_entry;
pub(crate) use render::{
    render_default_selection, render_promotion_evidence, render_registry_entry,
    render_registry_entry_snapshot,
};
pub(crate) use selection::parse_default_selection;

#[cfg(test)]
mod tests;
