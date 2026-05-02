//! Pattern selector — scores narrative patterns against situation tags.
//!
//! Each [`NarrativePattern`] declares weighted votes for situation tags.
//! The selector sums the weights of matching tags and picks the highest-scoring
//! pattern, using RNG to break ties.

use rand::Rng;

use crate::intent::pattern::NarrativePattern;
use crate::situation::SituationContext;
use crate::tag::Tag;

/// Compute the score of a single pattern against a set of situation tags.
///
/// For each `PatternVote` in the pattern, if the vote's tag appears in the
/// situation tags, the vote's weight is added to the total score.
/// A pattern with no matching votes scores 0.
pub fn score_pattern(pattern: &NarrativePattern, tags: &[Tag]) -> i32 {
    pattern
        .votes
        .iter()
        .filter_map(|vote| {
            if tags.iter().any(|t| t.0 == vote.tag) {
                Some(vote.weight)
            } else {
                None
            }
        })
        .sum()
}

/// Score all patterns against a situation's tags and return `(index, score)` pairs,
/// sorted by score descending.
pub fn rank_patterns(
    patterns: &[NarrativePattern],
    situation: &SituationContext,
) -> Vec<(usize, i32)> {
    let mut scored: Vec<(usize, i32)> = patterns
        .iter()
        .enumerate()
        .map(|(i, p)| (i, score_pattern(p, &situation.tags)))
        .collect();

    // Sort descending by score (stable sort preserves order for equal scores).
    scored.sort_by(|a, b| b.1.cmp(&a.1));
    scored
}

/// Error returned when pattern selection fails.
#[derive(Debug)]
pub enum PatternSelectError {
    /// No patterns available to select from.
    EmptyLibrary,
}

impl std::fmt::Display for PatternSelectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyLibrary => write!(f, "no narrative patterns available for selection"),
        }
    }
}

impl std::error::Error for PatternSelectError {}

/// Select the best-matching pattern for a situation, using RNG for tiebreaking.
///
/// Returns a reference to the winning pattern. If multiple patterns share the
/// highest score, one is chosen uniformly at random using the provided RNG.
///
/// # Errors
///
/// Returns [`PatternSelectError::EmptyLibrary`] if `patterns` is empty.
pub fn select_pattern<'a, R: Rng>(
    patterns: &'a [NarrativePattern],
    situation: &SituationContext,
    rng: &mut R,
) -> Result<&'a NarrativePattern, PatternSelectError> {
    if patterns.is_empty() {
        return Err(PatternSelectError::EmptyLibrary);
    }

    let ranked = rank_patterns(patterns, situation);
    let best_score = ranked[0].1;

    // Collect all patterns tied for the top score.
    let tied: Vec<usize> = ranked
        .iter()
        .take_while(|(_, score)| *score == best_score)
        .map(|(idx, _)| *idx)
        .collect();

    // Pick one uniformly at random among the tied candidates.
    let winner_idx = if tied.len() == 1 {
        tied[0]
    } else {
        let pick = rng.gen_range(0..tied.len());
        tied[pick]
    };

    Ok(&patterns[winner_idx])
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::load::load_default_patterns;
    use crate::tag::Tag;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn situation_with_tags(tags: &[&str]) -> SituationContext {
        SituationContext::new(tags.iter().map(|t| Tag::from(*t)).collect())
    }

    #[test]
    fn score_pattern_sums_matching_votes() {
        let patterns = load_default_patterns().unwrap();
        let lak = patterns.iter().find(|p| p.id == "lock_and_key").unwrap();

        // locked_vault (+3) + sealed_crypt (+2) = 5
        let tags = vec![Tag::from("locked_vault"), Tag::from("sealed_crypt")];
        assert_eq!(score_pattern(lak, &tags), 5);
    }

    #[test]
    fn score_pattern_ignores_non_matching_tags() {
        let patterns = load_default_patterns().unwrap();
        let lak = patterns.iter().find(|p| p.id == "lock_and_key").unwrap();

        let tags = vec![Tag::from("tavern"), Tag::from("cellar")];
        assert_eq!(score_pattern(lak, &tags), 0);
    }

    #[test]
    fn score_pattern_empty_tags_gives_zero() {
        let patterns = load_default_patterns().unwrap();
        let lak = patterns.iter().find(|p| p.id == "lock_and_key").unwrap();
        assert_eq!(score_pattern(lak, &[]), 0);
    }

    #[test]
    fn rank_patterns_orders_descending() {
        let patterns = load_default_patterns().unwrap();

        // locked_vault favors lock_and_key (+3)
        let situation = situation_with_tags(&["locked_vault"]);
        let ranked = rank_patterns(&patterns, &situation);

        // First should be lock_and_key with score 3.
        let (first_idx, first_score) = ranked[0];
        assert_eq!(patterns[first_idx].id, "lock_and_key");
        assert_eq!(first_score, 3);
    }

    #[test]
    fn select_pattern_picks_best_scorer() {
        let patterns = load_default_patterns().unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        // locked_vault → lock_and_key (+3), no other pattern gets points.
        let situation = situation_with_tags(&["locked_vault"]);
        let winner = select_pattern(&patterns, &situation, &mut rng).unwrap();
        assert_eq!(winner.id, "lock_and_key");
    }

    #[test]
    fn select_pattern_tavern_tags() {
        let patterns = load_default_patterns().unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        // tavern (+2) + dock_district (+1) → hub_and_spoke gets 3.
        let situation = situation_with_tags(&["tavern", "dock_district"]);
        let winner = select_pattern(&patterns, &situation, &mut rng).unwrap();
        assert_eq!(winner.id, "hub_and_spoke");
    }

    #[test]
    fn select_pattern_trapped_tags() {
        let patterns = load_default_patterns().unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        // trapped (+3) → gauntlet wins.
        let situation = situation_with_tags(&["trapped"]);
        let winner = select_pattern(&patterns, &situation, &mut rng).unwrap();
        assert_eq!(winner.id, "gauntlet");
    }

    #[test]
    fn select_pattern_exploration_tags() {
        let patterns = load_default_patterns().unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        // exploration (+3) → branching_exploration wins.
        let situation = situation_with_tags(&["exploration"]);
        let winner = select_pattern(&patterns, &situation, &mut rng).unwrap();
        assert_eq!(winner.id, "branching_exploration");
    }

    #[test]
    fn select_pattern_descent_tags() {
        let patterns = load_default_patterns().unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        // descent (+3) → linear_descent wins.
        let situation = situation_with_tags(&["descent"]);
        let winner = select_pattern(&patterns, &situation, &mut rng).unwrap();
        assert_eq!(winner.id, "linear_descent");
    }

    #[test]
    fn select_pattern_empty_library_errors() {
        let mut rng = StdRng::seed_from_u64(42);
        let situation = situation_with_tags(&["locked_vault"]);
        let result = select_pattern(&[], &situation, &mut rng);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PatternSelectError::EmptyLibrary
        ));
    }

    #[test]
    fn select_pattern_no_matching_tags_still_picks_one() {
        let patterns = load_default_patterns().unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        // Tags that match nothing — all patterns score 0, so one is chosen at random.
        let situation = situation_with_tags(&["completely_unknown_tag"]);
        let winner = select_pattern(&patterns, &situation, &mut rng).unwrap();
        // Should return *some* pattern (any of the 5 is valid).
        assert!(!winner.id.is_empty());
    }

    #[test]
    fn select_pattern_tiebreaking_is_deterministic_with_same_seed() {
        let patterns = load_default_patterns().unwrap();

        // All patterns score 0 → 5-way tie → RNG decides.
        let situation = situation_with_tags(&["completely_unknown_tag"]);

        let mut rng1 = StdRng::seed_from_u64(99);
        let winner1 = select_pattern(&patterns, &situation, &mut rng1).unwrap();

        let mut rng2 = StdRng::seed_from_u64(99);
        let winner2 = select_pattern(&patterns, &situation, &mut rng2).unwrap();

        assert_eq!(winner1.id, winner2.id);
    }

    #[test]
    fn select_pattern_different_seeds_can_break_ties_differently() {
        let patterns = load_default_patterns().unwrap();

        // All patterns score 0 → 5-way tie.
        let situation = situation_with_tags(&["completely_unknown_tag"]);

        // Try many seeds — at least two different outcomes should appear.
        let mut results = std::collections::HashSet::new();
        for seed in 0..100 {
            let mut rng = StdRng::seed_from_u64(seed);
            let winner = select_pattern(&patterns, &situation, &mut rng).unwrap();
            results.insert(winner.id.clone());
        }
        // With 5 options and 100 seeds, we should see at least 2 different choices.
        assert!(
            results.len() >= 2,
            "expected tiebreaking to produce variety, got: {:?}",
            results
        );
    }

    #[test]
    fn guarded_tag_disambiguates_lock_and_key_vs_gauntlet() {
        let patterns = load_default_patterns().unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        // "guarded" votes for lock_and_key (+1) AND gauntlet (+2).
        // gauntlet should win.
        let situation = situation_with_tags(&["guarded"]);
        let winner = select_pattern(&patterns, &situation, &mut rng).unwrap();
        assert_eq!(winner.id, "gauntlet");
    }

    #[test]
    fn combined_tags_sum_correctly() {
        let patterns = load_default_patterns().unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        // locked_vault (+3 lock_and_key) + trapped (+3 gauntlet) + guarded (+1 lak, +2 gauntlet)
        // lock_and_key: 3 + 1 = 4
        // gauntlet: 3 + 2 = 5
        // gauntlet should win.
        let situation = situation_with_tags(&["locked_vault", "trapped", "guarded"]);
        let winner = select_pattern(&patterns, &situation, &mut rng).unwrap();
        assert_eq!(winner.id, "gauntlet");
    }
}
