use crate::entity::plan::EntityArchetypeId;
use crate::intent::graph::NodeRole;
use crate::situation::{SituationContext, SituationDirective};
use crate::tag::Tag;

/// Creates the narrative context for the rat-infested port cellar scenario.
pub fn build_cellar_situation() -> SituationContext {
    SituationContext::new(vec![
        Tag::from("tavern"),
        Tag::from("cellar"),
        Tag::from("dock_district"),
        Tag::from("infested"),
        Tag::from("vermin"),
    ])
    .with_binding("location", "port_cellar")
    .with_binding("theme", "vermin_cellar")
    .with_directive(SituationDirective::PinEntity {
        archetype: EntityArchetypeId::from("giant_rat"),
        name: "Skrag the Rat King".to_string(),
        target_role: NodeRole::Goal,
    })
}
