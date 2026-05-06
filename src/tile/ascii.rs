use std::collections::HashMap;

use crate::entity::plan::{EntityArchetypeId, EntityPlan};
use crate::entity::registry::EntityRegistry;
use crate::feature::plan::FeaturePlan;
use crate::feature::registry::{FeatureRegistry, FeatureType};
use crate::geometry::geom::Point;
use crate::tile::map::TileMap;
use crate::tile::registry::{TileId, TileRegistry};

/// Core rendering: builds an ASCII string from a tile map, feature overlay, and entity overlay.
/// Tile characters come from `tile_registry`; feature characters from `feature_registry`.
/// Priority: entity > feature > tile.
fn render_impl(
    map: &TileMap,
    features: &FeaturePlan,
    entities: &EntityPlan,
    tile_registry: &TileRegistry,
    feature_registry: &FeatureRegistry,
    entity_registry: &EntityRegistry,
) -> String {
    // Build overlay: features first, then entities (entities win ties).
    let mut overlay: HashMap<Point, char> = HashMap::new();
    for placement in &features.features {
        let ch = feature_registry.ascii_char(&placement.feature_type);
        for &cell in &placement.cells {
            overlay.insert(cell, ch);
        }
    }
    for entity in &entities.entities {
        let ch = entity_registry.ascii_char(&entity.archetype);
        overlay.insert(entity.position, ch);
    }

    // Render
    let mut out = String::new();
    for y in 0..map.height as i32 {
        for x in 0..map.width as i32 {
            let pt = Point { x, y };
            let ch = if let Some(&oc) = overlay.get(&pt) {
                oc
            } else {
                tile_registry.ascii_char(map.get(x, y).unwrap_or(TileId(0)))
            };
            out.push(ch);
        }
        out.push('\n');
    }
    out
}

/// Renders a `TileMap` with feature and entity overlays using the provided registries.
///
/// Priority: entity > feature > tile base. If multiple entities/features
/// occupy the same cell, the last one in the plan wins.
pub fn render_ascii(
    map: &TileMap,
    features: &FeaturePlan,
    entities: &EntityPlan,
    tile_registry: &TileRegistry,
    feature_registry: &FeatureRegistry,
    entity_registry: &EntityRegistry,
) -> String {
    render_impl(
        map,
        features,
        entities,
        tile_registry,
        feature_registry,
        entity_registry,
    )
}

/// Renders a `TileMap` with feature and entity overlays using the default registries.
///
/// Convenience wrapper around [`render_ascii`] that uses
/// [`TileRegistry::default_registry`] and [`FeatureRegistry::default_registry`].
pub fn render_ascii_default(
    map: &TileMap,
    features: &FeaturePlan,
    entities: &EntityPlan,
) -> String {
    let tile_registry = TileRegistry::default_registry();
    let feature_registry = FeatureRegistry::default_registry();
    let entity_registry = EntityRegistry::default_registry();
    render_impl(
        map,
        features,
        entities,
        &tile_registry,
        &feature_registry,
        &entity_registry,
    )
}

/// Returns the ASCII character used to represent a given [`FeatureType`].
///
/// Uses the default feature registry for lookup. For custom registries,
/// use [`FeatureRegistry::ascii_char`] directly.
pub fn feature_char(feature_type: &FeatureType) -> char {
    let registry = FeatureRegistry::default_registry();
    registry.ascii_char(feature_type)
}

/// Returns the ASCII character used to represent an entity archetype.
///
/// Uses the default entity registry for lookup. For custom registries,
/// use [`EntityRegistry::ascii_char`] directly.
pub fn entity_char(archetype: &EntityArchetypeId) -> char {
    let registry = EntityRegistry::default_registry();
    registry.ascii_char(archetype)
}

/// Renders a `TileMap` with feature/entity overlays AND room letter annotations.
///
/// Each room's assigned letter is overlaid at its center point on the map.
/// Below the map, a legend is printed listing each room's letter, label, and role.
///
/// Format:
/// ```text
/// [A] Crypt Entrance (Entry) *
/// [B] Ossuary (Hub)
/// [C] Family Vault (Goal) !
/// ```
///
/// `*` marks the entry room, `!` marks the primary goal.
pub fn render_ascii_annotated(
    map: &TileMap,
    features: &crate::feature::plan::FeaturePlan,
    entities: &crate::entity::plan::EntityPlan,
    tile_registry: &TileRegistry,
    feature_registry: &FeatureRegistry,
    entity_registry: &EntityRegistry,
    annotations: &[crate::pipeline::RoomAnnotation],
) -> String {
    use std::collections::HashMap as OverlayMap;

    // Build overlay: features first, then entities (entities win ties).
    let mut overlay: OverlayMap<Point, char> = OverlayMap::new();
    for placement in &features.features {
        let ch = feature_registry.ascii_char(&placement.feature_type);
        for &cell in &placement.cells {
            overlay.insert(cell, ch);
        }
    }
    for entity in &entities.entities {
        let ch = entity_registry.ascii_char(&entity.archetype);
        overlay.insert(entity.position, ch);
    }

    // Room letter overlays take highest priority.
    for ann in annotations {
        overlay.insert(ann.center, ann.letter);
    }

    // Render map
    let mut out = String::new();
    for y in 0..map.height as i32 {
        for x in 0..map.width as i32 {
            let pt = Point { x, y };
            let ch = if let Some(&oc) = overlay.get(&pt) {
                oc
            } else {
                tile_registry.ascii_char(map.get(x, y).unwrap_or(TileId(0)))
            };
            out.push(ch);
        }
        out.push('\n');
    }

    // Legend
    out.push('\n');
    for ann in annotations {
        let label = ann.label.as_deref().unwrap_or("<unnamed>");
        let role = format!("{:?}", ann.role);
        let tension = format!("{}", ann.tension);
        let flags = match (ann.is_entry, ann.is_goal) {
            (true, _) => " *",
            (_, true) => " !",
            _ => "",
        };
        out.push_str(&format!(
            "[{}] {} ({}) [{}]{}",
            ann.letter, label, role, tension, flags
        ));
        out.push('\n');

        // List named entities in this room.
        for ne in &ann.named_entities {
            out.push_str(&format!("    >> {} ({})", ne.name, ne.archetype.0));
            out.push('\n');
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feature::plan::FeaturePlacement;
    use crate::geometry::geom::Point;
    use crate::spatial::plan::SpaceId;
    use crate::tile::map::TileMap;
    use crate::tile::registry::Tile;

    /// Helper: builds a 5×5 room with walls on the border, a door on the north
    /// edge at (2,0), and floor tiles inside.
    fn make_5x5_room() -> TileMap {
        let mut map = TileMap::new(5, 5);
        for y in 0..5i32 {
            for x in 0..5i32 {
                let tile = if x == 0 || x == 4 || y == 0 || y == 4 {
                    Tile::WALL
                } else {
                    Tile::FLOOR
                };
                map.set(x, y, tile);
            }
        }
        // Place a door on the north wall.
        map.set(2, 0, Tile::DOOR);
        map
    }

    /// Helper: builds a small `FeaturePlan` with an altar at (2,2) and a chest
    /// at (1,3).
    fn make_test_features() -> FeaturePlan {
        FeaturePlan {
            features: vec![
                FeaturePlacement {
                    feature_type: FeatureType::from("altar"),
                    anchor: Point { x: 2, y: 2 },
                    cells: vec![Point { x: 2, y: 2 }],
                    space_id: SpaceId(0),
                    tags: vec![],
                },
                FeaturePlacement {
                    feature_type: FeatureType::from("chest"),
                    anchor: Point { x: 1, y: 3 },
                    cells: vec![Point { x: 1, y: 3 }],
                    space_id: SpaceId(0),
                    tags: vec![],
                },
            ],
        }
    }

    #[test]
    fn render_base_map() {
        let map = make_5x5_room();
        let output = render_ascii_default(&map, &FeaturePlan::empty(), &EntityPlan::empty());
        let expected = "\
##+##\n\
#...#\n\
#...#\n\
#...#\n\
#####\n";
        assert_eq!(output, expected);
    }

    #[test]
    fn render_with_features_overlays_correctly() {
        let map = make_5x5_room();
        let features = make_test_features();
        let output = render_ascii_default(&map, &features, &EntityPlan::empty());
        let expected = "\
##+##\n\
#...#\n\
#.†.#\n\
#$..#\n\
#####\n";
        assert_eq!(output, expected);
    }

    #[test]
    fn render_without_features_has_no_overlay() {
        let map = make_5x5_room();
        let base = render_ascii_default(&map, &FeaturePlan::empty(), &EntityPlan::empty());
        let with_empty = render_ascii_default(&map, &FeaturePlan::empty(), &EntityPlan::empty());
        assert_eq!(
            base, with_empty,
            "empty FeaturePlan should match base render"
        );
    }

    #[test]
    fn feature_char_mapping_is_complete() {
        assert_eq!(feature_char(&FeatureType::from("altar")), '†');
        assert_eq!(feature_char(&FeatureType::from("sarcophagus")), 'S');
        assert_eq!(feature_char(&FeatureType::from("chest")), '$');
        assert_eq!(feature_char(&FeatureType::from("barrel")), 'o');
        assert_eq!(feature_char(&FeatureType::from("shelf")), '=');
        assert_eq!(feature_char(&FeatureType::from("table")), 'T');
        assert_eq!(feature_char(&FeatureType::from("trap")), '^');
        assert_eq!(feature_char(&FeatureType::from("torch")), '!');
    }

    #[test]
    fn cosmetic_decoration_chars() {
        assert_eq!(feature_char(&FeatureType::from("moss")), ',');
        assert_eq!(feature_char(&FeatureType::from("vines")), ',');
        assert_eq!(feature_char(&FeatureType::from("fungus")), ',');
        assert_eq!(feature_char(&FeatureType::from("stalagmite")), '\u{00a4}');
        assert_eq!(feature_char(&FeatureType::from("stalactite")), '\u{00a4}');
        assert_eq!(feature_char(&FeatureType::from("crystal")), '\u{00a4}');
        assert_eq!(feature_char(&FeatureType::from("torch")), '!');
        assert_eq!(feature_char(&FeatureType::from("banner")), '|');
    }

    #[test]
    fn multi_cell_feature_renders_all_cells() {
        let map = make_5x5_room();
        let features = FeaturePlan {
            features: vec![FeaturePlacement {
                feature_type: FeatureType::from("table"),
                anchor: Point { x: 1, y: 1 },
                cells: vec![Point { x: 1, y: 1 }, Point { x: 2, y: 1 }],
                space_id: SpaceId(0),
                tags: vec![],
            }],
        };
        let output = render_ascii_default(&map, &features, &EntityPlan::empty());
        let expected = "\
##+##\n\
#TT.#\n\
#...#\n\
#...#\n\
#####\n";
        assert_eq!(output, expected);
    }

    #[test]
    fn entity_char_mapping() {
        assert_eq!(entity_char(&EntityArchetypeId::from("rat")), 'r');
        assert_eq!(entity_char(&EntityArchetypeId::from("tavern_rat")), 'r');
        assert_eq!(entity_char(&EntityArchetypeId::from("skeleton")), 's');
        assert_eq!(
            entity_char(&EntityArchetypeId::from("skeleton_guardian")),
            'G'
        );
        assert_eq!(entity_char(&EntityArchetypeId::from("gate_guardian")), 'G');
        assert_eq!(entity_char(&EntityArchetypeId::from("smuggler")), '@');
        assert_eq!(entity_char(&EntityArchetypeId::from("chest_mimic")), 'M');
        // Unknown entities get '?' from the registry (not 'E' like the old substring matcher)
        assert_eq!(entity_char(&EntityArchetypeId::from("unknown_thing")), '?');
    }

    #[test]
    fn render_with_entities_shows_entities() {
        let map = make_5x5_room();
        let features = FeaturePlan::empty();
        let entities = EntityPlan {
            entities: vec![
                crate::entity::plan::EntityPlacement {
                    archetype: EntityArchetypeId::from("skeleton"),
                    name: None,
                    position: Point { x: 2, y: 2 },
                    space_id: SpaceId(0),
                    behavior_tags: vec![],
                    patrol_zone: None,
                },
                crate::entity::plan::EntityPlacement {
                    archetype: EntityArchetypeId::from("rat"),
                    name: None,
                    position: Point { x: 3, y: 3 },
                    space_id: SpaceId(0),
                    behavior_tags: vec![],
                    patrol_zone: None,
                },
            ],
        };
        let output = render_ascii_default(&map, &features, &entities);
        let expected = "\
##+##\n\
#...#\n\
#.s.#\n\
#..r#\n\
#####\n";
        assert_eq!(output, expected);
    }

    #[test]
    fn entities_override_features_in_render() {
        let map = make_5x5_room();
        let features = make_test_features(); // altar at (2,2), chest at (1,3)
        let entities = EntityPlan {
            entities: vec![crate::entity::plan::EntityPlacement {
                archetype: EntityArchetypeId::from("skeleton_guardian"),
                name: None,
                position: Point { x: 2, y: 2 }, // same cell as the altar
                space_id: SpaceId(0),
                behavior_tags: vec![],
                patrol_zone: None,
            }],
        };
        let output = render_ascii_default(&map, &features, &entities);
        // Entity 'G' should override the altar '†' at (2,2).
        // Chest '$' at (1,3) remains.
        let expected = "\
##+##\n\
#...#\n\
#.G.#\n\
#$..#\n\
#####\n";
        assert_eq!(output, expected);
    }

    #[test]
    fn empty_entity_plan_matches_features_only() {
        let map = make_5x5_room();
        let features = make_test_features();
        let entities = EntityPlan::empty();
        let with_entities = render_ascii_default(&map, &features, &entities);
        let features_only = render_ascii_default(&map, &features, &EntityPlan::empty());
        assert_eq!(
            with_entities, features_only,
            "empty EntityPlan should produce same output as features-only render"
        );
    }
}
