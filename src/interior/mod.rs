//! Interior planning — spatial structure for rooms.
//!
//! Computes zones (center, wall-band, corners, door-path, open) and reserved
//! door-to-door paths for each room. This data is consumed by the feature
//! planner and tile scatter system to make placement decisions without
//! expensive per-feature BFS reachability checks.

pub mod builder;
pub mod paths;
pub mod plan;
pub mod template;
pub mod zones;
