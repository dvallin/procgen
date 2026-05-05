use crate::entity::plan::EntityArchetypeId;
use crate::intent::graph::NodeRole;
use crate::situation::{SituationContext, SituationDirective};
use crate::tag::Tag;

/// Creates the narrative context for the abandoned mine scenario.
/// Medium-scale (10–15 rooms) with main shaft (linear descent) and
/// branching galleries, testing pattern composition and force-directed layout.
pub fn build_mine_situation() -> SituationContext {
    SituationContext::new(vec![
        Tag::from("mine"),
        Tag::from("underground"),
        Tag::from("exploration"),
        Tag::from("sprawling"),
        Tag::from("abandoned"),
    ])
    .with_binding("location", "mine")
    .with_binding("theme", "abandoned_mine")
    .with_binding("expansion_budget", "2")
    .with_directive(SituationDirective::PinEntity {
        archetype: EntityArchetypeId::from("mine_foreman"),
        name: "Grimjaw the Foreman".to_string(),
        target_role: NodeRole::Goal,
    })
}
