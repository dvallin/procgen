//! World/narrative situation context — translates into MapIntent.

use crate::entity::plan::EntityArchetypeId;
use crate::intent::graph::NodeRole;
use crate::tag::Tag;
use std::collections::HashMap;

/// A directive that instructs the pipeline to perform a specific action.
///
/// Directives let a world engine (quest system, event system) inject
/// deterministic requirements into the procedural generation pipeline.
#[derive(Debug, Clone)]
pub enum SituationDirective {
    /// Pin a named entity into a room matching the target role.
    ///
    /// The entity planner guarantees this entity appears in the specified
    /// room, bypassing normal budget/weight logic.
    PinEntity {
        /// Entity archetype to spawn (e.g. "giant_rat").
        archetype: EntityArchetypeId,
        /// Unique display name for this entity (e.g. "Skrag the Rat King").
        name: String,
        /// The narrative role of the room where this entity must appear.
        target_role: NodeRole,
    },
}

/// Narrative context that describes the world situation.
/// Does not prescribe map structure — the intent layer interprets these facts.
#[derive(Debug, Clone)]
pub struct SituationContext {
    pub tags: Vec<Tag>,
    pub bindings: HashMap<String, String>,
    /// Directives for the pipeline — deterministic placement instructions.
    pub directives: Vec<SituationDirective>,
}

impl SituationContext {
    /// Create a new SituationContext with tags and no bindings.
    pub fn new(tags: Vec<Tag>) -> Self {
        Self {
            tags,
            bindings: HashMap::new(),
            directives: Vec::new(),
        }
    }

    /// Add a key-value binding.
    pub fn with_binding(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.bindings.insert(key.into(), value.into());
        self
    }

    /// Add a directive.
    pub fn with_directive(mut self, directive: SituationDirective) -> Self {
        self.directives.push(directive);
        self
    }
}
