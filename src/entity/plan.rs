//! Data model for placed entities — the output of the entity planning stage.

use crate::geometry::geom::Point;
use crate::spatial::plan::SpaceId;
use crate::tag::Tag;

/// Identifies an entity archetype by name.
///
/// Kept as a stringly-typed ID for MVP — any scenario can introduce new entity
/// types (e.g. `"skeleton_guardian"`, `"tavern_rat"`) without growing an enum.
/// A proper archetype registry with stats/behavior is deferred to Phase 8.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EntityArchetypeId(pub String);

impl From<&str> for EntityArchetypeId {
    fn from(s: &str) -> Self {
        EntityArchetypeId(s.to_string())
    }
}

impl From<String> for EntityArchetypeId {
    fn from(s: String) -> Self {
        EntityArchetypeId(s)
    }
}

impl std::fmt::Display for EntityArchetypeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A single placed entity within a room.
#[derive(Debug, Clone)]
pub struct EntityPlacement {
    /// What kind of entity this is.
    pub archetype: EntityArchetypeId,
    /// The tile this entity occupies.
    pub position: Point,
    /// Which room this entity belongs to.
    pub space_id: SpaceId,
    /// Behavioral hints for downstream systems (e.g. "patrols", "stationary", "aggressive").
    pub behavior_tags: Vec<Tag>,
    /// Optional patrol zone — the set of walkable tiles this entity may roam.
    /// `None` for stationary entities (items, sentries).
    /// `Some(cells)` for patrolling mobs.
    pub patrol_zone: Option<Vec<Point>>,
}

/// The complete set of placed entities for a generated map.
#[derive(Debug, Clone)]
pub struct EntityPlan {
    pub entities: Vec<EntityPlacement>,
}

impl EntityPlan {
    /// Creates an empty entity plan.
    pub fn empty() -> Self {
        Self {
            entities: Vec::new(),
        }
    }

    /// Returns all entities placed in a specific room.
    pub fn entities_in_space(&self, space_id: SpaceId) -> Vec<&EntityPlacement> {
        self.entities
            .iter()
            .filter(|e| e.space_id == space_id)
            .collect()
    }
}

/// Errors that can occur during entity placement.
#[derive(Debug, Clone)]
pub enum EntityPlanError {
    /// A required entity could not be placed because the room has no suitable walkable tiles.
    NoWalkableTiles { space_id: SpaceId },
    /// The room exceeded its maximum entity density.
    DensityExceeded {
        space_id: SpaceId,
        max: u32,
        actual: u32,
    },
}

impl std::fmt::Display for EntityPlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoWalkableTiles { space_id } => {
                write!(
                    f,
                    "no walkable tiles available for required entity in space {:?}",
                    space_id
                )
            }
            Self::DensityExceeded {
                space_id,
                max,
                actual,
            } => {
                write!(
                    f,
                    "entity density exceeded in space {:?}: max {}, actual {}",
                    space_id, max, actual
                )
            }
        }
    }
}

impl std::error::Error for EntityPlanError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_plan_has_no_entities() {
        let plan = EntityPlan::empty();
        assert!(plan.entities.is_empty());
    }

    #[test]
    fn entities_in_space_filters_correctly() {
        let plan = EntityPlan {
            entities: vec![
                EntityPlacement {
                    archetype: EntityArchetypeId::from("skeleton"),
                    position: Point { x: 3, y: 3 },
                    space_id: SpaceId(0),
                    behavior_tags: vec![Tag::from("patrols")],
                    patrol_zone: Some(vec![
                        Point { x: 2, y: 3 },
                        Point { x: 3, y: 3 },
                        Point { x: 4, y: 3 },
                    ]),
                },
                EntityPlacement {
                    archetype: EntityArchetypeId::from("rat"),
                    position: Point { x: 7, y: 7 },
                    space_id: SpaceId(1),
                    behavior_tags: vec![],
                    patrol_zone: None,
                },
                EntityPlacement {
                    archetype: EntityArchetypeId::from("skeleton_guardian"),
                    position: Point { x: 4, y: 4 },
                    space_id: SpaceId(0),
                    behavior_tags: vec![Tag::from("stationary")],
                    patrol_zone: None,
                },
            ],
        };

        let space_0 = plan.entities_in_space(SpaceId(0));
        assert_eq!(space_0.len(), 2);
        assert_eq!(space_0[0].archetype, EntityArchetypeId::from("skeleton"));
        assert_eq!(
            space_0[1].archetype,
            EntityArchetypeId::from("skeleton_guardian")
        );

        let space_1 = plan.entities_in_space(SpaceId(1));
        assert_eq!(space_1.len(), 1);
        assert_eq!(space_1[0].archetype, EntityArchetypeId::from("rat"));

        let space_2 = plan.entities_in_space(SpaceId(99));
        assert!(space_2.is_empty());
    }

    #[test]
    fn entity_archetype_id_display() {
        let id = EntityArchetypeId::from("skeleton_guardian");
        assert_eq!(format!("{}", id), "skeleton_guardian");
    }

    #[test]
    fn entity_plan_error_display() {
        let err = EntityPlanError::NoWalkableTiles {
            space_id: SpaceId(3),
        };
        assert!(format!("{}", err).contains("space"));
        assert!(format!("{}", err).contains("walkable"));

        let err = EntityPlanError::DensityExceeded {
            space_id: SpaceId(1),
            max: 4,
            actual: 7,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("4"));
        assert!(msg.contains("7"));
        assert!(msg.contains("density"));
    }
}
