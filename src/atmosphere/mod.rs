//! Atmosphere profiles — coherent palettes of tile scatter, features, and entities.
//!
//! An [`AtmosphereProfile`] bundles weighted influences that activate when a
//! room carries matching tags. Multiple profiles can activate simultaneously;
//! their influences accumulate into a merged [`AtmospherePalette`] from which
//! the planner samples via weighted random selection.

pub mod apply;
pub mod profile;
