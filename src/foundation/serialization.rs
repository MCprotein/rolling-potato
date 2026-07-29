//! Strict JSON parsing and canonical serialization facade.

#[path = "serialization/canonical.rs"]
mod canonical;
#[path = "serialization/error.rs"]
mod error;
#[path = "serialization/object.rs"]
mod object;

#[path = "serialization/parser.rs"]
mod parser;

#[path = "serialization/render.rs"]
mod render;
#[path = "serialization/types.rs"]
mod types;

pub use canonical::{
    canonical_u128, canonical_u64, parse_canonical_object, render_canonical_object,
};
#[allow(unused_imports)]
pub use object::{boolean, number, number_u128, parse_object, parse_object_exact_order, string};
pub use parser::parse_value;
pub(crate) use render::escape_string_content;
pub use render::render_compact;
#[allow(unused_imports)]
pub use types::{CanonicalObject, CanonicalValue, Object, Value};

#[cfg(test)]
#[path = "serialization/tests.rs"]
mod tests;
