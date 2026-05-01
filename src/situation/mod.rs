//! World/narrative situation context — translates into MapIntent.

use crate::tag::Tag;
use std::collections::HashMap;

/// Narrative context that describes the world situation.
/// Does not prescribe map structure — the intent layer interprets these facts.
#[derive(Debug, Clone)]
pub struct SituationContext {
    pub tags: Vec<Tag>,
    pub bindings: HashMap<String, String>,
}

impl SituationContext {
    /// Create a new SituationContext with tags and no bindings.
    pub fn new(tags: Vec<Tag>) -> Self {
        Self {
            tags,
            bindings: HashMap::new(),
        }
    }

    /// Add a key-value binding.
    pub fn with_binding(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.bindings.insert(key.into(), value.into());
        self
    }
}
