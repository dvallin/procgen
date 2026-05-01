//! Validation traits and implementations.

pub mod geometry;

/// Generic validator trait. Implementations check a generated artifact for issues.
pub trait Validator<T> {
    fn validate(&self, value: &T) -> ValidationResult;
}

#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub issues: Vec<ValidationIssue>,
}

impl ValidationResult {
    pub fn is_ok(&self) -> bool {
        self.issues.iter().all(|i| i.severity != Severity::Error)
    }
}

#[derive(Debug, Clone)]
pub struct ValidationIssue {
    pub severity: Severity,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}
