use crate::situation::SituationContext;

use super::map_intent::MapIntent;

#[derive(Debug)]
pub enum IntentBuildError {
    MissingRequiredBinding(String),
    InvalidSituation(String),
}

impl std::fmt::Display for IntentBuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingRequiredBinding(key) => write!(f, "missing required binding: {key}"),
            Self::InvalidSituation(msg) => write!(f, "invalid situation: {msg}"),
        }
    }
}

impl std::error::Error for IntentBuildError {}

/// Converts a SituationContext into a MapIntent.
pub trait IntentBuilder {
    fn build(&self, situation: &SituationContext) -> Result<MapIntent, IntentBuildError>;
}
