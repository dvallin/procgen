use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Tag(pub String);

impl From<&str> for Tag {
    fn from(s: &str) -> Self {
        Tag(s.to_string())
    }
}

impl From<String> for Tag {
    fn from(s: String) -> Self {
        Tag(s)
    }
}

impl std::fmt::Display for Tag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
