use std::collections::HashMap;

use crate::tag::Tag;

use super::graph::*;
use super::template::*;

#[derive(Debug)]
pub enum InstantiateError {
    UnknownNodeRole(String),
    UnknownEdgeRole(String),
    UnknownNodeReference(String),
    DuplicateNodeId(String),
}

impl std::fmt::Display for InstantiateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownNodeRole(r) => write!(f, "unknown node role: {r}"),
            Self::UnknownEdgeRole(r) => write!(f, "unknown edge role: {r}"),
            Self::UnknownNodeReference(id) => write!(f, "unknown node reference: {id}"),
            Self::DuplicateNodeId(id) => write!(f, "duplicate node id: {id}"),
        }
    }
}

impl std::error::Error for InstantiateError {}

pub trait ScenarioTemplateInstantiator {
    fn instantiate(&self, asset: &GraphTemplateAsset) -> Result<ScenarioGraph, InstantiateError>;
}

#[derive(Default)]
pub struct SimpleScenarioInstantiator;

impl ScenarioTemplateInstantiator for SimpleScenarioInstantiator {
    fn instantiate(&self, asset: &GraphTemplateAsset) -> Result<ScenarioGraph, InstantiateError> {
        let mut ids = HashMap::<String, ScenarioNodeId>::new();
        let mut nodes = Vec::new();

        for (index, n) in asset.nodes.iter().enumerate() {
            if ids.contains_key(&n.id) {
                return Err(InstantiateError::DuplicateNodeId(n.id.clone()));
            }

            let node_id = ScenarioNodeId(index as u32);
            ids.insert(n.id.clone(), node_id);

            nodes.push(ScenarioNode {
                id: node_id,
                key: n.id.clone(),
                role: parse_node_role(&n.role)?,
                tags: n.tags.iter().map(|t| Tag::from(t.as_str())).collect(),
                label: n.label.clone(),
                archetype_hint: None,
            });
        }

        let mut edges = Vec::new();
        for e in &asset.edges {
            let from = *ids
                .get(&e.from)
                .ok_or_else(|| InstantiateError::UnknownNodeReference(e.from.clone()))?;
            let to = *ids
                .get(&e.to)
                .ok_or_else(|| InstantiateError::UnknownNodeReference(e.to.clone()))?;

            edges.push(ScenarioEdge {
                from,
                to,
                role: parse_edge_role(&e.kind)?,
                tags: e.tags.iter().map(|t| Tag::from(t.as_str())).collect(),
            });
        }

        Ok(ScenarioGraph { nodes, edges })
    }
}

fn parse_node_role(s: &str) -> Result<NodeRole, InstantiateError> {
    match s {
        "entry" => Ok(NodeRole::Entry),
        "hub" => Ok(NodeRole::Hub),
        "gate" => Ok(NodeRole::Gate),
        "goal" => Ok(NodeRole::Goal),
        "reward" => Ok(NodeRole::Reward),
        "branch" => Ok(NodeRole::Branch),
        "transition" => Ok(NodeRole::Transition),
        other => Err(InstantiateError::UnknownNodeRole(other.to_string())),
    }
}

fn parse_edge_role(s: &str) -> Result<EdgeRole, InstantiateError> {
    match s {
        "traversal" => Ok(EdgeRole::Traversal),
        "optional_traversal" => Ok(EdgeRole::OptionalTraversal),
        "restricted_traversal" => Ok(EdgeRole::RestrictedTraversal),
        "secret_traversal" => Ok(EdgeRole::SecretTraversal),
        "vertical_traversal" => Ok(EdgeRole::VerticalTraversal),
        other => Err(InstantiateError::UnknownEdgeRole(other.to_string())),
    }
}
