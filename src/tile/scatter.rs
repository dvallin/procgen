//! Tile scatter: replaces floor tiles within rooms based on tag-driven rules.
//!
//! Each [`TileScatterRule`] maps a room tag to a target tile and a density
//! (fraction of floor tiles to replace). Scatter rules are produced by the
//! atmosphere system or constructed programmatically.

use std::collections::{HashMap, HashSet};

use rand::Rng;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::geometry::geom::{GeometryPlan, Point};
use crate::spatial::plan::{SpaceId, SpatialPlan};
use crate::tag::Tag;
use crate::tile::map::TileMap;
use crate::tile::registry::{Tile, TileId, TileRegistry};

/// A single scatter rule: if a room has `match_tag`, replace `density` fraction
/// of its floor tiles with `target_tile`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileScatterRule {
    /// Name of the tile to scatter (looked up in the registry).
    pub target_tile: String,
    /// Fraction of floor tiles to replace (0.0–1.0).
    pub density: f64,
    /// Room tag that activates this rule.
    pub match_tag: Tag,
}

/// Resolve a tile name to its [`TileId`] by scanning the registry.
///
/// Returns `None` if no tile with the given name is registered.
pub fn resolve_tile_name(registry: &TileRegistry, name: &str) -> Option<TileId> {
    for i in 0..registry.len() {
        let id = TileId(i as u16);
        if registry.name(id) == name {
            return Some(id);
        }
    }
    None
}

/// Apply tile scatter rules to a [`TileMap`].
///
/// For each placed space in the geometry plan, finds the corresponding
/// [`SpaceSpec`](crate::spatial::plan::SpaceSpec) in the spatial plan, collects
/// matching scatter rules based on the space's tags, and randomly replaces
/// floor tiles within the room interior with the target tile.
///
/// Only tiles that are currently [`Tile::FLOOR`] are replaced — doors, walls,
/// and other features are never overwritten.
pub fn apply_scatter(
    map: &mut TileMap,
    geometry: &GeometryPlan,
    spatial: &SpatialPlan,
    rules: &[TileScatterRule],
    registry: &TileRegistry,
    rng: &mut impl Rng,
    reserved_per_room: &HashMap<SpaceId, HashSet<Point>>,
) {
    for placed in &geometry.spaces {
        // Find the corresponding SpaceSpec in the spatial plan.
        let Some(spec) = spatial.spaces.iter().find(|s| s.id == placed.space_id) else {
            continue;
        };

        // Collect all matching scatter rules for this space's tags.
        let matching: Vec<&TileScatterRule> = rules
            .iter()
            .filter(|rule| spec.has_tag(&rule.match_tag))
            .collect();

        if matching.is_empty() {
            continue;
        }

        let empty_reserved = HashSet::new();
        let reserved = reserved_per_room
            .get(&placed.space_id)
            .unwrap_or(&empty_reserved);

        // Iterate the interior of the room's rect (skip walls on the border).
        let rect = placed.rect;
        let x_start = rect.x + 1;
        let x_end = rect.x + rect.w - 1;
        let y_start = rect.y + 1;
        let y_end = rect.y + rect.h - 1;

        for rule in &matching {
            // Resolve the target tile name to a TileId.
            let Some(target_id) = resolve_tile_name(registry, &rule.target_tile) else {
                debug!(
                    target_tile = %rule.target_tile,
                    "scatter rule references unknown tile, skipping"
                );
                continue;
            };

            let target_walkable = registry.is_walkable(target_id);
            let mut scattered_count = 0u32;

            for y in y_start..y_end {
                for x in x_start..x_end {
                    if map.get(x, y) != Some(Tile::FLOOR) {
                        continue;
                    }
                    if rng.r#gen::<f64>() >= rule.density {
                        continue;
                    }
                    // Never place non-walkable scatter on reserved door paths.
                    if !target_walkable && reserved.contains(&Point { x, y }) {
                        continue;
                    }
                    // For non-walkable target tiles, only scatter where all 4
                    // cardinal neighbors are walkable. This prevents creating
                    // bottlenecks that disconnect rooms.
                    if !target_walkable {
                        let p = Point { x, y };
                        let all_neighbors_walkable = p
                            .cardinals()
                            .iter()
                            .all(|n| map.get(n.x, n.y).map_or(false, |t| registry.is_walkable(t)));
                        if !all_neighbors_walkable {
                            continue;
                        }
                    }
                    map.set(x, y, target_id);
                    scattered_count += 1;
                }
            }

            debug!(
                space_id = placed.space_id.0,
                target_tile = %rule.target_tile,
                density = rule.density,
                scattered = scattered_count,
                label = ?placed.label,
                "tile scatter applied"
            );
        }
    }
}

/// Apply scatter rules directly to a single room's interior.
///
/// This is the per-room variant used by the atmosphere system. Unlike
/// [`apply_scatter`] which iterates all rooms in a geometry plan, this
/// targets a single room identified by its rect.
pub fn scatter_room(
    map: &mut TileMap,
    rect: crate::geometry::geom::Rect,
    rules: &[TileScatterRule],
    registry: &TileRegistry,
    rng: &mut impl Rng,
    reserved: &HashSet<Point>,
) {
    let x_start = rect.x + 1;
    let x_end = rect.x + rect.w - 1;
    let y_start = rect.y + 1;
    let y_end = rect.y + rect.h - 1;

    for rule in rules {
        let Some(target_id) = resolve_tile_name(registry, &rule.target_tile) else {
            debug!(
                target_tile = %rule.target_tile,
                "scatter rule references unknown tile, skipping"
            );
            continue;
        };

        let target_walkable = registry.is_walkable(target_id);

        for y in y_start..y_end {
            for x in x_start..x_end {
                if map.get(x, y) != Some(Tile::FLOOR) {
                    continue;
                }
                if rng.r#gen::<f64>() >= rule.density {
                    continue;
                }
                // Never place non-walkable scatter on reserved door paths.
                if !target_walkable && reserved.contains(&Point { x, y }) {
                    continue;
                }
                if !target_walkable {
                    let p = Point { x, y };
                    let all_neighbors_walkable = p
                        .cardinals()
                        .iter()
                        .all(|n| map.get(n.x, n.y).map_or(false, |t| registry.is_walkable(t)));
                    if !all_neighbors_walkable {
                        continue;
                    }
                }
                map.set(x, y, target_id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::geom::{Footprint, PlacedSpace, Rect};
    use crate::intent::graph::{NodeRole, ScenarioNodeId};
    use crate::intent::map_intent::LocationKind;
    use crate::spatial::plan::{
        AtomicSpace, RealizationStyle, SizeHint, SpaceId, SpaceKind, SpaceSpec, SpatialPlan,
    };
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    /// Helper: create a 5×5 tile map with walls on border and floor inside.
    fn make_5x5_room_map() -> TileMap {
        let mut map = TileMap::new(5, 5);
        for y in 0..5 {
            for x in 0..5 {
                if x == 0 || x == 4 || y == 0 || y == 4 {
                    map.set(x, y, Tile::WALL);
                } else {
                    map.set(x, y, Tile::FLOOR);
                }
            }
        }
        map
    }

    /// Helper: create a minimal SpatialPlan with one space that has the given tags.
    fn make_spatial_plan(tags: Vec<Tag>) -> SpatialPlan {
        use crate::spatial::plan::classify_tags;
        let (structural, atmosphere) = classify_tags(&tags);
        SpatialPlan {
            spaces: vec![SpaceSpec {
                id: SpaceId(0),
                origin: ScenarioNodeId(0),
                role: NodeRole::Hub,
                structural_tags: structural,
                atmosphere_tags: atmosphere,
                motifs: vec![],
                style: RealizationStyle::RoomLike,
                kind: SpaceKind::Atomic(AtomicSpace {
                    width: 5,
                    height: 5,
                }),
                label: Some("test_room".into()),
                archetype: None,
                size_hint: SizeHint::Small,
            }],
            links: vec![],
            constraints: vec![],
            location_kind: LocationKind::Dungeon,
        }
    }

    /// Helper: create a GeometryPlan with one placed space at (0,0) 5×5.
    fn make_geometry_plan() -> GeometryPlan {
        let rect = Rect {
            x: 0,
            y: 0,
            w: 5,
            h: 5,
        };
        GeometryPlan {
            spaces: vec![PlacedSpace {
                space_id: SpaceId(0),
                rect,
                footprint: Footprint::Rect(rect),
                style: RealizationStyle::RoomLike,
                label: Some("test_room".into()),
            }],
            links: vec![],
        }
    }

    #[test]
    fn resolve_tile_name_finds_builtin_tiles() {
        let registry = TileRegistry::default_registry();
        assert_eq!(resolve_tile_name(&registry, "water"), Some(Tile::WATER));
        assert_eq!(resolve_tile_name(&registry, "rubble"), Some(Tile::RUBBLE));
        assert_eq!(resolve_tile_name(&registry, "grass"), Some(Tile::GRASS));
        assert_eq!(resolve_tile_name(&registry, "floor"), Some(Tile::FLOOR));
    }

    #[test]
    fn resolve_tile_name_returns_none_for_unknown() {
        let registry = TileRegistry::default_registry();
        assert_eq!(resolve_tile_name(&registry, "lava"), None);
        assert_eq!(resolve_tile_name(&registry, ""), None);
    }

    #[test]
    fn scatter_replaces_floor_tiles_with_water() {
        let mut map = make_5x5_room_map();
        let spatial = make_spatial_plan(vec![Tag::from("flooded")]);
        let geometry = make_geometry_plan();
        let registry = TileRegistry::default_registry();
        let rules = vec![TileScatterRule {
            target_tile: "water".into(),
            density: 0.5,
            match_tag: Tag::from("flooded"),
        }];

        let mut rng = StdRng::seed_from_u64(42);
        apply_scatter(
            &mut map,
            &geometry,
            &spatial,
            &rules,
            &registry,
            &mut rng,
            &HashMap::new(),
        );

        // The interior is 3×3 = 9 tiles. With density 0.5 we expect some to be water.
        let mut water_count = 0;
        let mut floor_count = 0;
        for y in 1..4 {
            for x in 1..4 {
                match map.get(x, y) {
                    Some(t) if t == Tile::WATER => water_count += 1,
                    Some(t) if t == Tile::FLOOR => floor_count += 1,
                    _ => {}
                }
            }
        }

        assert!(water_count > 0, "expected some water tiles to be scattered");
        assert!(
            floor_count > 0,
            "expected some floor tiles to remain (density < 1.0)"
        );
        assert_eq!(water_count + floor_count, 9, "interior should be 9 tiles");
    }

    #[test]
    fn scatter_does_not_affect_walls() {
        let mut map = make_5x5_room_map();
        let spatial = make_spatial_plan(vec![Tag::from("flooded")]);
        let geometry = make_geometry_plan();
        let registry = TileRegistry::default_registry();
        let rules = vec![TileScatterRule {
            target_tile: "water".into(),
            density: 1.0, // Replace ALL floor tiles.
            match_tag: Tag::from("flooded"),
        }];

        let mut rng = StdRng::seed_from_u64(0);
        apply_scatter(
            &mut map,
            &geometry,
            &spatial,
            &rules,
            &registry,
            &mut rng,
            &HashMap::new(),
        );

        // Walls on border should be untouched.
        for x in 0..5 {
            assert_eq!(map.get(x, 0), Some(Tile::WALL));
            assert_eq!(map.get(x, 4), Some(Tile::WALL));
        }
        for y in 0..5 {
            assert_eq!(map.get(0, y), Some(Tile::WALL));
            assert_eq!(map.get(4, y), Some(Tile::WALL));
        }
    }

    #[test]
    fn scatter_skips_rooms_without_matching_tags() {
        let mut map = make_5x5_room_map();
        let spatial = make_spatial_plan(vec![Tag::from("dry")]); // No matching tag.
        let geometry = make_geometry_plan();
        let registry = TileRegistry::default_registry();
        let rules = vec![TileScatterRule {
            target_tile: "water".into(),
            density: 1.0,
            match_tag: Tag::from("flooded"),
        }];

        let mut rng = StdRng::seed_from_u64(0);
        apply_scatter(
            &mut map,
            &geometry,
            &spatial,
            &rules,
            &registry,
            &mut rng,
            &HashMap::new(),
        );

        // All interior tiles should still be floor.
        for y in 1..4 {
            for x in 1..4 {
                assert_eq!(map.get(x, y), Some(Tile::FLOOR));
            }
        }
    }

    #[test]
    fn scatter_with_zero_density_changes_nothing() {
        let mut map = make_5x5_room_map();
        let spatial = make_spatial_plan(vec![Tag::from("flooded")]);
        let geometry = make_geometry_plan();
        let registry = TileRegistry::default_registry();
        let rules = vec![TileScatterRule {
            target_tile: "water".into(),
            density: 0.0,
            match_tag: Tag::from("flooded"),
        }];

        let mut rng = StdRng::seed_from_u64(0);
        apply_scatter(
            &mut map,
            &geometry,
            &spatial,
            &rules,
            &registry,
            &mut rng,
            &HashMap::new(),
        );

        for y in 1..4 {
            for x in 1..4 {
                assert_eq!(map.get(x, y), Some(Tile::FLOOR));
            }
        }
    }

    #[test]
    fn scatter_with_unknown_tile_name_skips_gracefully() {
        let mut map = make_5x5_room_map();
        let spatial = make_spatial_plan(vec![Tag::from("volcanic")]);
        let geometry = make_geometry_plan();
        let registry = TileRegistry::default_registry();
        let rules = vec![TileScatterRule {
            target_tile: "lava".into(), // Not in registry.
            density: 1.0,
            match_tag: Tag::from("volcanic"),
        }];

        let mut rng = StdRng::seed_from_u64(0);
        apply_scatter(
            &mut map,
            &geometry,
            &spatial,
            &rules,
            &registry,
            &mut rng,
            &HashMap::new(),
        );

        // Nothing should have changed.
        for y in 1..4 {
            for x in 1..4 {
                assert_eq!(map.get(x, y), Some(Tile::FLOOR));
            }
        }
    }
}
