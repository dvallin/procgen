use crate::situation::SituationContext;
use crate::tag::Tag;

/// Creates the narrative context for the noble crypt scenario.
pub fn build_crypt_situation() -> SituationContext {
    SituationContext::new(vec![
        Tag::from("noble_family"),
        Tag::from("sealed_crypt"),
        Tag::from("burial_ground"),
        Tag::from("locked_vault"),
    ])
    .with_binding("location", "crypt")
    .with_binding("theme", "undead_nobility")
}
