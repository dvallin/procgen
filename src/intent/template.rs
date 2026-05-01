use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphTemplateAsset {
    pub id: String,
    pub nodes: Vec<NodeTemplateAsset>,
    pub edges: Vec<EdgeTemplateAsset>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeTemplateAsset {
    pub id: String,
    pub role: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeTemplateAsset {
    pub from: String,
    pub to: String,
    pub kind: String,
    #[serde(default)]
    pub tags: Vec<String>,
}
