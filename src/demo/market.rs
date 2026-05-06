use crate::situation::SituationContext;
use crate::tag::Tag;

/// Creates the narrative context for the market district scenario.
pub fn build_market_situation() -> SituationContext {
    SituationContext::new(vec![
        Tag::from("marketplace"),
        Tag::from("urban"),
        Tag::from("dock_district"),
        Tag::from("busy"),
    ])
    .with_binding("location", "urban")
    .with_binding("theme", "market_district")
}
