//! Generic intent builder — orchestrates pattern selection, slot filling,
//! constraint inference, and MapIntent assembly from a SituationContext.
//!
//! This replaces hard-coded per-scenario builders with a single data-driven
//! builder that works for any situation, given patterns and vocabularies.

use rand::rngs::StdRng;
use tracing::{debug, info_span};

use crate::asset::load::{load_default_patterns, load_default_vocabularies};
use crate::intent::builder::{IntentBuildError, IntentBuilder};
use crate::intent::constraints::infer_constraints;
use crate::intent::filler::fill_pattern;
use crate::intent::map_intent::{MapIntent, MapScale, MotifId};
use crate::intent::pattern::NarrativePattern;
use crate::intent::selector::select_pattern;
use crate::intent::vocabulary::ThemeVocabulary;
use crate::situation::SituationContext;

/// A generic, data-driven intent builder.
///
/// Orchestrates the full intent generation pipeline:
/// 1. Select a narrative pattern based on situation tags.
/// 2. Look up the theme vocabulary from `bindings["theme"]`.
/// 3. Fill pattern slots with vocabulary entries.
/// 4. Infer structural constraints from the filled graph.
/// 5. Derive location_kind, scale, and motifs from vocabulary metadata.
/// 6. Assemble and return a complete `MapIntent`.
pub struct GenericIntentBuilder {
    /// Available narrative patterns.
    patterns: Vec<NarrativePattern>,
    /// Available theme vocabularies.
    vocabularies: Vec<ThemeVocabulary>,
}

impl GenericIntentBuilder {
    /// Create a new GenericIntentBuilder with the given patterns and vocabularies.
    pub fn new(patterns: Vec<NarrativePattern>, vocabularies: Vec<ThemeVocabulary>) -> Self {
        Self {
            patterns,
            vocabularies,
        }
    }

    /// Create a GenericIntentBuilder loaded from default assets.
    ///
    /// Uses `load_default_patterns()` and `load_default_vocabularies()` with
    /// embedded fallback.
    pub fn from_defaults() -> Result<Self, IntentBuildError> {
        let patterns = load_default_patterns().map_err(|e| {
            IntentBuildError::InvalidSituation(format!("failed to load patterns: {e}"))
        })?;
        let vocabularies = load_default_vocabularies().map_err(|e| {
            IntentBuildError::InvalidSituation(format!("failed to load vocabularies: {e}"))
        })?;
        Ok(Self::new(patterns, vocabularies))
    }

    /// Find a vocabulary by its ID and resolve any base inheritance.
    fn find_vocabulary(&self, theme_id: &str) -> Option<ThemeVocabulary> {
        let vocab = self.vocabularies.iter().find(|v| v.id == theme_id)?;
        vocab.resolve(&self.vocabularies)
    }
}

impl IntentBuilder for GenericIntentBuilder {
    fn build(
        &self,
        situation: &SituationContext,
        rng: &mut StdRng,
    ) -> Result<MapIntent, IntentBuildError> {
        let _span = info_span!("intent_building").entered();

        // 1. Select a pattern.
        //    If bindings["pattern"] is set, use it as an explicit override.
        //    Otherwise, fall back to tag-based weighted voting.
        let pattern = if let Some(pattern_id) = situation.bindings.get("pattern") {
            let p = self
                .patterns
                .iter()
                .find(|p| p.id == *pattern_id)
                .ok_or_else(|| {
                    IntentBuildError::InvalidSituation(format!(
                        "no pattern found with id '{pattern_id}'"
                    ))
                })?;
            debug!(pattern_id = %p.id, pattern_name = %p.name, "pattern override from binding");
            p
        } else {
            let p = select_pattern(&self.patterns, situation, rng).map_err(|e| {
                IntentBuildError::InvalidSituation(format!("pattern selection failed: {e}"))
            })?;
            debug!(pattern_id = %p.id, pattern_name = %p.name, "pattern selected via voting");
            p
        };

        // 2. Look up vocabulary from bindings["theme"].
        let theme_id = situation
            .bindings
            .get("theme")
            .ok_or_else(|| IntentBuildError::MissingRequiredBinding("theme".to_string()))?;

        let vocabulary = self.find_vocabulary(theme_id).ok_or_else(|| {
            IntentBuildError::InvalidSituation(format!(
                "no vocabulary found for theme '{theme_id}'"
            ))
        })?;
        debug!(vocabulary = %vocabulary.id, "vocabulary resolved");

        // 3. Fill pattern slots with vocabulary entries.
        let filled = fill_pattern(pattern, &vocabulary, rng)
            .map_err(|e| IntentBuildError::InvalidSituation(format!("slot filling failed: {e}")))?;
        debug!(
            nodes = filled.graph.nodes.len(),
            edges = filled.graph.edges.len(),
            "slots filled"
        );

        // 4. Infer structural constraints from the filled graph.
        let constraints = infer_constraints(&filled.graph, pattern.max_depth);
        debug!(constraints = constraints.len(), "constraints inferred");

        // 5. Derive metadata from vocabulary + filled graph.
        let location_kind = vocabulary.location_kind;
        let scale = scale_from_node_count(filled.graph.nodes.len());
        let motifs: Vec<_> = vocabulary
            .motifs
            .iter()
            .map(|m| MotifId::from(m.as_str()))
            .collect();

        // 6. Assemble MapIntent.
        debug!(location_kind = ?location_kind, scale = ?scale, motifs = motifs.len(), "intent assembled");
        Ok(MapIntent {
            location_kind,
            scale,
            tags: situation.tags.clone(),
            motifs,
            structural_graph: filled.graph,
            constraints,
        })
    }
}

/// Infer map scale from the number of nodes in the filled graph.
fn scale_from_node_count(count: usize) -> MapScale {
    match count {
        0..=4 => MapScale::Tiny,
        5..=7 => MapScale::Small,
        8..=14 => MapScale::Medium,
        15..=24 => MapScale::Large,
        _ => MapScale::Huge,
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::graph::{EdgeRole, NodeRole};
    use crate::intent::map_intent::{IntentConstraint, LocationKind};
    use crate::tag::Tag;
    use rand::SeedableRng;

    fn crypt_situation() -> SituationContext {
        SituationContext::new(vec![
            Tag::from("noble_family"),
            Tag::from("sealed_crypt"),
            Tag::from("burial_ground"),
            Tag::from("locked_vault"),
        ])
        .with_binding("location", "crypt")
        .with_binding("theme", "undead_nobility")
    }

    fn tavern_situation() -> SituationContext {
        SituationContext::new(vec![
            Tag::from("tavern"),
            Tag::from("cellar"),
            Tag::from("smuggling"),
            Tag::from("dock_district"),
        ])
        .with_binding("location", "tavern_cellar")
        .with_binding("theme", "urban_underground")
    }

    #[test]
    fn generic_builder_produces_valid_intent_for_crypt() {
        let builder = GenericIntentBuilder::from_defaults().unwrap();
        let situation = crypt_situation();
        let mut rng = StdRng::seed_from_u64(42);

        let intent = builder.build(&situation, &mut rng).unwrap();

        // Should pick lock_and_key pattern (sealed_crypt +2, locked_vault +3).
        assert_eq!(intent.location_kind, LocationKind::Dungeon);
        assert!(!intent.structural_graph.nodes.is_empty());
        assert!(!intent.structural_graph.edges.is_empty());

        // Should have an Entry node.
        assert!(
            intent
                .structural_graph
                .nodes
                .iter()
                .any(|n| n.role == NodeRole::Entry)
        );

        // Should have motifs from undead_nobility vocabulary.
        let motif_strs: Vec<&str> = intent.motifs.iter().map(|m| m.0.as_str()).collect();
        assert!(motif_strs.contains(&"gothic"));
    }

    #[test]
    fn generic_builder_produces_valid_intent_for_tavern() {
        let builder = GenericIntentBuilder::from_defaults().unwrap();
        let situation = tavern_situation();
        let mut rng = StdRng::seed_from_u64(42);

        let intent = builder.build(&situation, &mut rng).unwrap();

        // Should use urban_underground vocabulary.
        assert_eq!(intent.location_kind, LocationKind::Building);

        // Should have motifs from urban_underground.
        let motif_strs: Vec<&str> = intent.motifs.iter().map(|m| m.0.as_str()).collect();
        assert!(motif_strs.contains(&"timber") || motif_strs.contains(&"damp"));
    }

    #[test]
    fn crypt_intent_has_must_gate_constraint() {
        let builder = GenericIntentBuilder::from_defaults().unwrap();
        let situation = crypt_situation();
        let mut rng = StdRng::seed_from_u64(42);

        let intent = builder.build(&situation, &mut rng).unwrap();

        // lock_and_key has Gate→Goal via RestrictedTraversal.
        let has_must_gate = intent
            .constraints
            .iter()
            .any(|c| matches!(c, IntentConstraint::MustGate { .. }));
        assert!(
            has_must_gate,
            "crypt intent should have MustGate constraint, got: {:?}",
            intent.constraints
        );
    }

    #[test]
    fn missing_theme_binding_errors() {
        let builder = GenericIntentBuilder::from_defaults().unwrap();
        let situation = SituationContext::new(vec![Tag::from("locked_vault")])
            .with_binding("location", "crypt");
        // No "theme" binding!
        let mut rng = StdRng::seed_from_u64(42);

        let result = builder.build(&situation, &mut rng);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("theme"));
    }

    #[test]
    fn unknown_theme_errors() {
        let builder = GenericIntentBuilder::from_defaults().unwrap();
        let situation = SituationContext::new(vec![Tag::from("locked_vault")])
            .with_binding("theme", "nonexistent_theme");
        let mut rng = StdRng::seed_from_u64(42);

        let result = builder.build(&situation, &mut rng);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("nonexistent_theme"));
    }

    #[test]
    fn deterministic_with_same_seed() {
        let builder = GenericIntentBuilder::from_defaults().unwrap();
        let situation = crypt_situation();

        let mut rng1 = StdRng::seed_from_u64(99);
        let intent1 = builder.build(&situation, &mut rng1).unwrap();

        let mut rng2 = StdRng::seed_from_u64(99);
        let intent2 = builder.build(&situation, &mut rng2).unwrap();

        assert_eq!(
            intent1.structural_graph.nodes.len(),
            intent2.structural_graph.nodes.len()
        );
        for (n1, n2) in intent1
            .structural_graph
            .nodes
            .iter()
            .zip(intent2.structural_graph.nodes.iter())
        {
            assert_eq!(n1.key, n2.key);
            assert_eq!(n1.role, n2.role);
            assert_eq!(n1.label, n2.label);
        }
    }

    #[test]
    fn different_seeds_can_produce_variety() {
        let builder = GenericIntentBuilder::from_defaults().unwrap();
        let situation = crypt_situation();

        let mut labels_seen = std::collections::HashSet::new();
        for seed in 0..50 {
            let mut rng = StdRng::seed_from_u64(seed);
            let intent = builder.build(&situation, &mut rng).unwrap();
            if let Some(hub) = intent
                .structural_graph
                .nodes
                .iter()
                .find(|n| n.role == NodeRole::Hub)
            {
                labels_seen.insert(hub.label.clone());
            }
        }
        // undead_nobility has 2 Hub entries, so should see variety.
        assert!(
            labels_seen.len() >= 2,
            "expected variety in hub labels, got: {:?}",
            labels_seen
        );
    }

    #[test]
    fn scale_inferred_from_node_count() {
        assert_eq!(scale_from_node_count(3), MapScale::Tiny);
        assert_eq!(scale_from_node_count(4), MapScale::Tiny);
        assert_eq!(scale_from_node_count(5), MapScale::Small);
        assert_eq!(scale_from_node_count(7), MapScale::Small);
        assert_eq!(scale_from_node_count(8), MapScale::Medium);
        assert_eq!(scale_from_node_count(14), MapScale::Medium);
        assert_eq!(scale_from_node_count(15), MapScale::Large);
        assert_eq!(scale_from_node_count(25), MapScale::Huge);
    }

    #[test]
    fn generic_builder_graph_edges_are_valid() {
        let builder = GenericIntentBuilder::from_defaults().unwrap();
        let situation = crypt_situation();
        let mut rng = StdRng::seed_from_u64(42);

        let intent = builder.build(&situation, &mut rng).unwrap();
        let graph = &intent.structural_graph;

        let node_ids: Vec<_> = graph.nodes.iter().map(|n| n.id).collect();
        for edge in &graph.edges {
            assert!(
                node_ids.contains(&edge.from),
                "edge from {:?} references missing node",
                edge.from
            );
            assert!(
                node_ids.contains(&edge.to),
                "edge to {:?} references missing node",
                edge.to
            );
        }
    }

    #[test]
    fn generic_builder_all_nodes_have_archetype_hints() {
        let builder = GenericIntentBuilder::from_defaults().unwrap();
        let situation = crypt_situation();
        let mut rng = StdRng::seed_from_u64(42);

        let intent = builder.build(&situation, &mut rng).unwrap();

        for node in &intent.structural_graph.nodes {
            assert!(
                node.archetype_hint.is_some(),
                "node '{}' should have archetype_hint from vocabulary",
                node.key
            );
        }
    }

    #[test]
    fn generic_builder_crypt_has_restricted_traversal() {
        let builder = GenericIntentBuilder::from_defaults().unwrap();
        let situation = crypt_situation();
        let mut rng = StdRng::seed_from_u64(42);

        let intent = builder.build(&situation, &mut rng).unwrap();

        // lock_and_key pattern has RestrictedTraversal from gate to goal.
        let has_restricted = intent
            .structural_graph
            .edges
            .iter()
            .any(|e| e.role == EdgeRole::RestrictedTraversal);
        assert!(
            has_restricted,
            "crypt should have a RestrictedTraversal edge"
        );
    }

    #[test]
    fn explicit_pattern_override_selects_specified_pattern() {
        let builder = GenericIntentBuilder::from_defaults().unwrap();
        // Use crypt theme but force linear_descent pattern (instead of lock_and_key).
        let situation =
            SituationContext::new(vec![Tag::from("noble_family"), Tag::from("sealed_crypt")])
                .with_binding("theme", "undead_nobility")
                .with_binding("pattern", "linear_descent");
        let mut rng = StdRng::seed_from_u64(42);

        let intent = builder.build(&situation, &mut rng).unwrap();

        // linear_descent has Transition nodes (passage_1, passage_2) — lock_and_key does not.
        let transition_count = intent
            .structural_graph
            .nodes
            .iter()
            .filter(|n| n.role == NodeRole::Transition)
            .count();
        assert!(
            transition_count >= 2,
            "linear_descent should have at least 2 Transition nodes, got {}",
            transition_count
        );
    }

    #[test]
    fn explicit_pattern_override_with_vermin_cellar() {
        let builder = GenericIntentBuilder::from_defaults().unwrap();
        // Use vermin_cellar theme but force linear_descent (normally it would pick hub_and_spoke).
        let situation = SituationContext::new(vec![
            Tag::from("tavern"),
            Tag::from("cellar"),
            Tag::from("infested"),
        ])
        .with_binding("theme", "vermin_cellar")
        .with_binding("pattern", "linear_descent");
        let mut rng = StdRng::seed_from_u64(42);

        let intent = builder.build(&situation, &mut rng).unwrap();

        // Should still use vermin_cellar vocabulary (Building location).
        assert_eq!(intent.location_kind, LocationKind::Building);

        // Should have the linear chain structure.
        let transition_count = intent
            .structural_graph
            .nodes
            .iter()
            .filter(|n| n.role == NodeRole::Transition)
            .count();
        assert!(
            transition_count >= 2,
            "linear_descent forced on vermin_cellar should have Transition nodes, got {}",
            transition_count
        );

        // Should still have vermin-themed labels from vocabulary.
        let labels: Vec<&str> = intent
            .structural_graph
            .nodes
            .iter()
            .filter_map(|n| n.label.as_deref())
            .collect();
        let has_vermin_label = labels.iter().any(|l| {
            l.contains("Cellar")
                || l.contains("Rat")
                || l.contains("Gnawed")
                || l.contains("Pantry")
                || l.contains("Grain")
                || l.contains("Drain")
                || l.contains("Warren")
                || l.contains("Den")
                || l.contains("Hatch")
                || l.contains("Kitchen")
        });
        assert!(
            has_vermin_label,
            "expected vermin-themed labels, got: {:?}",
            labels
        );
    }

    #[test]
    fn unknown_pattern_override_errors() {
        let builder = GenericIntentBuilder::from_defaults().unwrap();
        let situation = SituationContext::new(vec![Tag::from("locked_vault")])
            .with_binding("theme", "undead_nobility")
            .with_binding("pattern", "nonexistent_pattern");
        let mut rng = StdRng::seed_from_u64(42);

        let result = builder.build(&situation, &mut rng);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("nonexistent_pattern"),
            "error should mention the bad pattern id: {}",
            err
        );
    }
}
