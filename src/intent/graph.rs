use crate::spatial::plan::SpaceArchetype;
use crate::tag::Tag;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScenarioNodeId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeRole {
    Entry,
    Hub,
    Gate,
    Goal,
    Reward,
    Branch,
    Transition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeRole {
    Traversal,
    OptionalTraversal,
    RestrictedTraversal,
    SecretTraversal,
    VerticalTraversal,
}

#[derive(Debug, Clone)]
pub struct ScenarioNode {
    pub id: ScenarioNodeId,
    pub key: String,
    pub role: NodeRole,
    pub tags: Vec<Tag>,
    pub label: Option<String>,
    /// Optional archetype hint from vocabulary. When present, the spatial planner
    /// uses this instead of deriving archetype from role.
    pub archetype_hint: Option<SpaceArchetype>,
}

#[derive(Debug, Clone)]
pub struct ScenarioEdge {
    pub from: ScenarioNodeId,
    pub to: ScenarioNodeId,
    pub role: EdgeRole,
    pub tags: Vec<Tag>,
}

#[derive(Debug, Clone)]
pub struct ScenarioGraph {
    pub nodes: Vec<ScenarioNode>,
    pub edges: Vec<ScenarioEdge>,
}
