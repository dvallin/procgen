use crate::situation::SituationContext;
use crate::tag::Tag;

/// Creates the narrative context for the natural cave scenario.
pub fn build_cave_situation() -> SituationContext {
    SituationContext::new(vec![
        Tag::from("cave"),
        Tag::from("natural"),
        Tag::from("underground"),
        Tag::from("exploration"),
    ])
    .with_binding("location", "cave")
    .with_binding("theme", "natural_cave")
    .with_binding("expansion_budget", "1")
}
