use crate::situation::SituationContext;
use crate::tag::Tag;

/// Creates the narrative context for the tavern cellar scenario.
pub fn build_tavern_situation() -> SituationContext {
    SituationContext::new(vec![
        Tag::from("tavern"),
        Tag::from("cellar"),
        Tag::from("smuggling"),
        Tag::from("dock_district"),
    ])
    .with_binding("location", "tavern_cellar")
    .with_binding("theme", "urban_underground")
}
