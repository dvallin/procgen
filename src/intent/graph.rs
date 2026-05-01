use crate::tag::Tag;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScenarioNodeId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeRole {
    Entry,
    Hub,
    Gate,
    Goal,
    Reward,
    Branch,
    Transition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
