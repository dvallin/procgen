//! ANSI-colored ASCII rendering for tile maps.
//!
//! Provides colored variants of the plain-text renderers in [`super::ascii`].
//! Entity, feature, and tile characters are wrapped in ANSI escape sequences
//! chosen to communicate danger, interactivity, and atmosphere at a glance.

use std::collections::HashMap;

use crate::entity::plan::{EntityArchetypeId, EntityPlan};
use crate::entity::registry::{EntityCategory, EntityRegistry};
use crate::feature::plan::FeaturePlan;
use crate::feature::registry::{FeatureCategory, FeatureRegistry, FeatureType};
use crate::geometry::geom::Point;
use crate::tag::Tag;
use crate::tile::map::TileMap;
use crate::tile::registry::{Tile, TileId, TileRegistry};

// ─── ANSI Constants ─────────────────────────────────────────────────────────

const RESET: &str = "\x1b[0m";
const BOLD_RED: &str = "\x1b[1;31m";
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const BLUE: &str = "\x1b[34m";
const MAGENTA: &str = "\x1b[35m";
const BOLD_CYAN: &str = "\x1b[1;36m";
const BOLD_YELLOW: &str = "\x1b[1;33m";
const BOLD_WHITE: &str = "\x1b[1;37m";
const DARK_GRAY: &str = "\x1b[90m";

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Wraps a character in ANSI color codes.
///
/// If `ansi_code` is empty, returns the bare character (no escape sequences).
fn colorize(ch: char, ansi_code: &str) -> String {
    if ansi_code.is_empty() {
        ch.to_string()
    } else {
        format!("{ansi_code}{ch}{RESET}")
    }
}

/// Wraps a string in ANSI color codes.
///
/// If `ansi_code` is empty, returns the string unchanged.
fn colorize_str(s: &str, ansi_code: &str) -> String {
    if ansi_code.is_empty() {
        s.to_string()
    } else {
        format!("{ansi_code}{s}{RESET}")
    }
}

/// Returns the ANSI escape code for an entity based on its category and tags.
///
/// Priority:
/// 1. `Boss` → Bold Red
/// 2. `Hazard` → Magenta
/// 3. `Creature` with "elite" tag → Yellow
/// 4. `Creature` (normal) → Red
/// 5. `NPC` → Green
/// 6. Unknown → no color
pub fn entity_color(registry: &EntityRegistry, archetype: &EntityArchetypeId) -> &'static str {
    match registry.get(archetype) {
        Some(props) => match props.category {
            EntityCategory::Boss => BOLD_RED,
            EntityCategory::Hazard => MAGENTA,
            EntityCategory::Creature => {
                if props.tags.contains(&Tag::from("elite")) {
                    YELLOW
                } else {
                    RED
                }
            }
            EntityCategory::NPC => GREEN,
        },
        None => "",
    }
}

/// Returns the ANSI escape code for a feature based on its category.
///
/// - `Trap` → Red (dangerous)
/// - `Interactable` → Bold Cyan (important interactive)
/// - `Container` → Yellow (loot)
/// - `Furniture` → Blue (structural)
/// - `Decoration` → Dark gray (cosmetic)
pub fn feature_color(registry: &FeatureRegistry, feature_type: &FeatureType) -> &'static str {
    match registry.category(feature_type) {
        Some(FeatureCategory::Trap) => RED,
        Some(FeatureCategory::Interactable) => BOLD_CYAN,
        Some(FeatureCategory::Container) => YELLOW,
        Some(FeatureCategory::Furniture) => BLUE,
        Some(FeatureCategory::Decoration) => DARK_GRAY,
        None => "",
    }
}

/// Returns the ANSI escape code for a built-in tile type.
///
/// - Void → no color (space char, invisible anyway)
/// - Wall → Dark gray
/// - Floor → no color (default)
/// - Door → Yellow
/// - LockedDoor → Bold Yellow
/// - Water → Blue
/// - Pit → Red (dark red)
/// - Stairs → Bold White
/// - Rubble → Dark gray
/// - Grass → Green
/// - Other → no color
pub fn tile_color(tile_id: TileId) -> &'static str {
    if tile_id == Tile::VOID {
        ""
    } else if tile_id == Tile::WALL {
        DARK_GRAY
    } else if tile_id == Tile::FLOOR {
        ""
    } else if tile_id == Tile::DOOR {
        YELLOW
    } else if tile_id == Tile::LOCKED_DOOR {
        BOLD_YELLOW
    } else if tile_id == Tile::WATER {
        BLUE
    } else if tile_id == Tile::PIT {
        RED
    } else if tile_id == Tile::STAIRS {
        BOLD_WHITE
    } else if tile_id == Tile::RUBBLE {
        DARK_GRAY
    } else if tile_id == Tile::GRASS {
        GREEN
    } else {
        ""
    }
}

// ─── Main Colored Renderer ──────────────────────────────────────────────────

/// Renders a `TileMap` with feature and entity overlays using ANSI colors.
///
/// This is the colored equivalent of [`super::ascii::render_ascii`]. Priority
/// for overlapping elements: entity > feature > tile. Each character is
/// wrapped in the appropriate ANSI escape sequence for its type.
pub fn render_ascii_colored(
    map: &TileMap,
    features: &FeaturePlan,
    entities: &EntityPlan,
    tile_registry: &TileRegistry,
    feature_registry: &FeatureRegistry,
    entity_registry: &EntityRegistry,
) -> String {
    // Build entity overlay: archetype per position (last wins).
    let mut entity_overlay: HashMap<Point, &EntityArchetypeId> = HashMap::new();
    for entity in &entities.entities {
        entity_overlay.insert(entity.position, &entity.archetype);
    }

    // Build feature overlay: feature type per position (last wins).
    let mut feature_overlay: HashMap<Point, &FeatureType> = HashMap::new();
    for placement in &features.features {
        for &cell in &placement.cells {
            feature_overlay.insert(cell, &placement.feature_type);
        }
    }

    let mut out = String::new();
    for y in 0..map.height as i32 {
        for x in 0..map.width as i32 {
            let pt = Point { x, y };

            if let Some(archetype) = entity_overlay.get(&pt) {
                let ch = entity_registry.ascii_char(archetype);
                let color = entity_color(entity_registry, archetype);
                out.push_str(&colorize(ch, color));
            } else if let Some(ft) = feature_overlay.get(&pt) {
                let ch = feature_registry.ascii_char(ft);
                let color = feature_color(feature_registry, ft);
                out.push_str(&colorize(ch, color));
            } else {
                let tile_id = map.get(x, y).unwrap_or(TileId(0));
                let ch = tile_registry.ascii_char(tile_id);
                let color = tile_color(tile_id);
                out.push_str(&colorize(ch, color));
            }
        }
        out.push('\n');
    }
    out
}

// ─── Annotated Colored Renderer ─────────────────────────────────────────────

/// Renders a `TileMap` with colored overlays AND room letter annotations.
///
/// This is the colored equivalent of [`super::ascii::render_ascii_annotated`].
/// Room annotation letters are rendered in Bold White. The legend below the
/// map uses color coding:
/// - Entry marker `*` in green
/// - Goal marker `!` in red
/// - Tension levels colored: Low=green, Medium=yellow, High=red, Climax=bold red
pub fn render_ascii_annotated_colored(
    map: &TileMap,
    features: &FeaturePlan,
    entities: &EntityPlan,
    tile_registry: &TileRegistry,
    feature_registry: &FeatureRegistry,
    entity_registry: &EntityRegistry,
    annotations: &[crate::pipeline::RoomAnnotation],
) -> String {
    // Build entity overlay.
    let mut entity_overlay: HashMap<Point, &EntityArchetypeId> = HashMap::new();
    for entity in &entities.entities {
        entity_overlay.insert(entity.position, &entity.archetype);
    }

    // Build feature overlay.
    let mut feature_overlay: HashMap<Point, &FeatureType> = HashMap::new();
    for placement in &features.features {
        for &cell in &placement.cells {
            feature_overlay.insert(cell, &placement.feature_type);
        }
    }

    // Room letter overlays take highest priority.
    let mut annotation_overlay: HashMap<Point, char> = HashMap::new();
    for ann in annotations {
        annotation_overlay.insert(ann.center, ann.letter);
    }

    // Render map
    let mut out = String::new();
    for y in 0..map.height as i32 {
        for x in 0..map.width as i32 {
            let pt = Point { x, y };

            if let Some(&letter) = annotation_overlay.get(&pt) {
                // Room annotation letters: Bold White
                out.push_str(&colorize(letter, BOLD_WHITE));
            } else if let Some(archetype) = entity_overlay.get(&pt) {
                let ch = entity_registry.ascii_char(archetype);
                let color = entity_color(entity_registry, archetype);
                out.push_str(&colorize(ch, color));
            } else if let Some(ft) = feature_overlay.get(&pt) {
                let ch = feature_registry.ascii_char(ft);
                let color = feature_color(feature_registry, ft);
                out.push_str(&colorize(ch, color));
            } else {
                let tile_id = map.get(x, y).unwrap_or(TileId(0));
                let ch = tile_registry.ascii_char(tile_id);
                let color = tile_color(tile_id);
                out.push_str(&colorize(ch, color));
            }
        }
        out.push('\n');
    }

    // Legend
    out.push('\n');
    for ann in annotations {
        let label = ann.label.as_deref().unwrap_or("<unnamed>");
        let role = format!("{:?}", ann.role);
        let tension_color = tension_level_color(ann.tension);
        let tension_str = colorize_str(&format!("{}", ann.tension), tension_color);

        let flags = match (ann.is_entry, ann.is_goal) {
            (true, _) => format!(" {}", colorize_str("*", GREEN)),
            (_, true) => format!(" {}", colorize_str("!", RED)),
            _ => String::new(),
        };

        out.push_str(&format!(
            "[{}] {} ({}) [{}]{}",
            colorize(ann.letter, BOLD_WHITE),
            label,
            role,
            tension_str,
            flags,
        ));
        out.push('\n');

        // List named entities in this room.
        for ne in &ann.named_entities {
            let ne_color = entity_color(entity_registry, &ne.archetype);
            let name_colored = colorize_str(&ne.name, ne_color);
            out.push_str(&format!("    >> {} ({})", name_colored, ne.archetype.0));
            out.push('\n');
        }
    }

    out
}

/// Returns the ANSI color code for a tension level in the legend.
fn tension_level_color(tension: crate::tension::TensionLevel) -> &'static str {
    use crate::tension::TensionLevel;
    match tension {
        TensionLevel::Low => GREEN,
        TensionLevel::Medium => YELLOW,
        TensionLevel::High => RED,
        TensionLevel::Climax => BOLD_RED,
    }
}
