//! Narrative pattern data model — parameterized graph templates.
//!
//! A NarrativePattern is a topological template that defines how spaces
//! relate to each other (Entry→Hub→Gate→Goal, etc.) without prescribing
//! geometry. Situation tags vote for patterns; the highest-scoring pattern
//! is instantiated into a ScenarioGraph.

use serde::{Deserialize, Serialize};

use crate::intent::graph::{EdgeRole, NodeRole};

/// A named slot in a pattern — becomes a ScenarioNode when instantiated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternSlot {
    /// Unique key within this pattern (e.g. "hub", "gate", "branch_1").
    pub key: String,
    /// The structural role this slot fulfills.
    pub role: NodeRole,
    /// If true, this slot may be omitted during instantiation (RNG choice).
    #[serde(default)]
    pub optional: bool,
}

/// An edge template in a pattern — becomes a ScenarioEdge when instantiated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternEdge {
    /// Key of the source slot.
    pub from: String,
    /// Key of the target slot.
    pub to: String,
    /// Traversal type for this connection.
    pub role: EdgeRole,
}

/// Tag-based vote: situation tags that favor selecting this pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternVote {
    /// The situation tag that triggers this vote.
    pub tag: String,
    /// Weight added when this tag is present (can be negative to penalize).
    pub weight: i32,
}

/// A complete narrative pattern — a parameterized graph template.
///
/// Patterns are topological (what connects to what), not geometric (where).
/// They are JSON-serializable and loaded as assets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NarrativePattern {
    /// Unique identifier for this pattern (e.g. "lock_and_key").
    pub id: String,
    /// Human-readable name (e.g. "Lock and Key").
    pub name: String,
    /// Brief description of the pattern's gameplay feel.
    #[serde(default)]
    pub description: String,
    /// The slots (nodes) in this pattern template.
    pub slots: Vec<PatternSlot>,
    /// The edges connecting slots.
    pub edges: Vec<PatternEdge>,
    /// Optional maximum depth override. When present, constraint inference
    /// emits a MaxDepth constraint with this value.
    #[serde(default)]
    pub max_depth: Option<u32>,
    /// Tag-based votes for pattern selection.
    #[serde(default)]
    pub votes: Vec<PatternVote>,
}
