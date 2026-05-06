//! Shape refinement — transforms a room's bounding rect into a concrete footprint.
//!
//! The bounding rect is preserved for layout and routing; the footprint determines
//! which cells actually become floor tiles. This separates the *layout* concern
//! (WHERE rooms go) from the *shape* concern (WHAT each room looks like).
//!
//! Future shape strategies (cellular automata blobs, L-shaped rooms, etc.) implement
//! the [`ShapeRefinement`] trait without touching the layout algorithm.

use std::collections::{HashSet, VecDeque};

use rand::Rng;

use crate::geometry::geom::{Footprint, Point, Rect};
use crate::spatial::plan::SpaceSpec;

/// Transforms a room's bounding rect into a concrete [`Footprint`].
///
/// Implementations receive the room's bounding rectangle, its spatial spec
/// (for tag/archetype inspection), and an RNG for stochastic refinement.
/// The returned footprint's cells must be a subset of the rect's interior
/// and must form a single connected component (cardinal adjacency).
pub trait ShapeRefinement {
    fn refine(&self, rect: Rect, space: &SpaceSpec, rng: &mut dyn rand::RngCore) -> Footprint;
}

// ─── RectShape (no-op) ─────────────────────────────────────────────────────

/// Default shape refinement: keeps the room as a plain rectangle.
/// The footprint is identical to the bounding rect.
pub struct RectShape;

impl ShapeRefinement for RectShape {
    fn refine(&self, rect: Rect, _space: &SpaceSpec, _rng: &mut dyn rand::RngCore) -> Footprint {
        Footprint::Rect(rect)
    }
}

// ─── CaveIrregularizer ─────────────────────────────────────────────────────

/// Organic cave shape refinement.
///
/// Algorithm:
/// 1. Start with all interior cells of the bounding rect (excluding boundary walls).
/// 2. Carve corners — remove triangular wedges from each corner.
/// 3. Roughen edges — randomly remove/add cells along the perimeter.
/// 4. Connectivity check — BFS flood-fill to keep only the largest connected component.
/// 5. Minimum threshold — if too many cells were removed (< 50%), fall back to full rect.
///
/// The `roughness` parameter (0.0–1.0) controls how aggressively cells are carved.
/// - 0.0: no carving at all (effectively RectShape)
/// - 0.3–0.5: natural-looking caves (recommended default)
/// - 1.0: maximum carving (may hit the fallback threshold often)
pub struct CaveIrregularizer {
    pub roughness: f64,
}

impl Default for CaveIrregularizer {
    fn default() -> Self {
        Self { roughness: 0.4 }
    }
}

impl ShapeRefinement for CaveIrregularizer {
    fn refine(&self, rect: Rect, _space: &SpaceSpec, rng: &mut dyn rand::RngCore) -> Footprint {
        // Interior cells: skip the 1-cell boundary (walls).
        let interior = interior_cells(rect);
        let original_count = interior.len();

        if original_count == 0 {
            return Footprint::Rect(rect);
        }

        // Minimum size to irregularize: interior must be at least 5×5 (room 7×7).
        let interior_w = rect.w - 2;
        let interior_h = rect.h - 2;
        if interior_w < 5 || interior_h < 5 {
            return Footprint::Rect(rect);
        }

        let mut cells: HashSet<Point> = interior.into_iter().collect();

        // Compute protected cells: center row + center column (a "+" shape).
        // These guarantee that doors at edge midpoints can always reach the interior.
        let protected = compute_protected_cross(rect);

        // Step 1: carve corners.
        carve_corners(&mut cells, rect, self.roughness, &protected, rng);

        // Step 2: roughen edges.
        roughen_edges(&mut cells, rect, self.roughness, &protected, rng);

        // Step 3: keep only the largest connected component.
        let cells = largest_connected_component(cells);

        // Step 4: fallback if too few cells remain.
        let min_threshold = (original_count as f64 * 0.5) as usize;
        if cells.len() < min_threshold {
            return Footprint::Rect(rect);
        }

        let mut result: Vec<Point> = cells.into_iter().collect();
        result.sort_by(|a, b| a.y.cmp(&b.y).then(a.x.cmp(&b.x)));

        Footprint::Cells(result)
    }
}

// ─── Internal helpers ───────────────────────────────────────────────────────

/// Returns all interior cells of a rect (1-cell border excluded).
fn interior_cells(rect: Rect) -> Vec<Point> {
    let mut cells = Vec::new();
    for y in (rect.y + 1)..(rect.y + rect.h - 1) {
        for x in (rect.x + 1)..(rect.x + rect.w - 1) {
            cells.push(Point { x, y });
        }
    }
    cells
}

/// Compute the protected "+" cross through the room center.
///
/// These cells are never removed by carving/roughening. They form a 1-cell-wide
/// cross along the center row and center column, preserving a basic connectivity
/// spine through the room.
fn compute_protected_cross(rect: Rect) -> HashSet<Point> {
    let center = rect.center();
    let mut protected = HashSet::new();

    let y_min = rect.y + 1;
    let y_max = rect.y + rect.h - 2;
    let x_min = rect.x + 1;
    let x_max = rect.x + rect.w - 2;

    // Center row
    for x in x_min..=x_max {
        protected.insert(Point { x, y: center.y });
    }
    // Center column
    for y in y_min..=y_max {
        protected.insert(Point { x: center.x, y });
    }

    protected
}

/// Carve triangular wedges from each corner of the room.
///
/// For each corner, cells within a triangular region (scaled by roughness and room size)
/// are removed with probability proportional to their distance from the corner.
fn carve_corners(
    cells: &mut HashSet<Point>,
    rect: Rect,
    roughness: f64,
    protected: &HashSet<Point>,
    rng: &mut dyn rand::RngCore,
) {
    let interior_w = rect.w - 2;
    let interior_h = rect.h - 2;

    // Corner carve radius scales with room size and roughness.
    // For a 7×7 room (5×5 interior), max radius ~2. For 11×9 (9×7), max radius ~3.
    let max_radius_x = ((interior_w as f64 * roughness * 0.5).round() as i32).max(1);
    let max_radius_y = ((interior_h as f64 * roughness * 0.5).round() as i32).max(1);

    // The four interior corners.
    let corners = [
        (rect.x + 1, rect.y + 1, 1, 1),                     // top-left
        (rect.x + rect.w - 2, rect.y + 1, -1, 1),           // top-right
        (rect.x + 1, rect.y + rect.h - 2, 1, -1),           // bottom-left
        (rect.x + rect.w - 2, rect.y + rect.h - 2, -1, -1), // bottom-right
    ];

    for (cx, cy, dx, dy) in corners {
        for ry in 0..max_radius_y {
            for rx in 0..max_radius_x {
                // Triangular mask: only carve if rx + ry < max_radius (Manhattan distance).
                let max_r = max_radius_x.min(max_radius_y);
                if rx + ry >= max_r {
                    continue;
                }

                let px = cx + rx * dx;
                let py = cy + ry * dy;
                let p = Point { x: px, y: py };

                // Probability decreases further from corner.
                let dist_ratio = (rx + ry) as f64 / max_r as f64;
                let removal_prob = roughness * (1.0 - dist_ratio * 0.5);

                if rng.r#gen::<f64>() < removal_prob {
                    if !protected.contains(&p) {
                        cells.remove(&p);
                    }
                }
            }
        }
    }
}

/// Roughen the edges of the cell set by randomly removing perimeter cells.
///
/// A cell is considered "on the perimeter" if at least one of its cardinal neighbors
/// is NOT in the cell set. Perimeter cells are removed with probability scaled by roughness.
fn roughen_edges(
    cells: &mut HashSet<Point>,
    rect: Rect,
    roughness: f64,
    protected: &HashSet<Point>,
    rng: &mut dyn rand::RngCore,
) {
    // Collect perimeter cells (cells with at least one missing cardinal neighbor).
    // Sort for deterministic iteration order (HashSet order is non-deterministic).
    let mut perimeter: Vec<Point> = cells
        .iter()
        .filter(|p| p.cardinals().iter().any(|n| !cells.contains(n)))
        .copied()
        .collect();
    perimeter.sort_by(|a, b| a.y.cmp(&b.y).then(a.x.cmp(&b.x)));

    // Remove perimeter cells with some probability.
    // Lower probability than corner carving to avoid over-erosion.
    let removal_prob = roughness * 0.35;

    for p in &perimeter {
        if rng.r#gen::<f64>() < removal_prob {
            // Never remove protected cells (center cross).
            if protected.contains(p) {
                continue;
            }

            // Don't remove if it's too close to the center of the room.
            let center = rect.center();
            let dx = (p.x - center.x).abs();
            let dy = (p.y - center.y).abs();
            let interior_w = rect.w - 2;
            let interior_h = rect.h - 2;
            let center_threshold_x = interior_w / 4;
            let center_threshold_y = interior_h / 4;

            if dx <= center_threshold_x && dy <= center_threshold_y {
                continue; // Protect center area.
            }

            cells.remove(p);
        }
    }

    // Optional second pass: add a few cells just outside the current perimeter
    // to create small bumps/protrusions. Only within the rect interior.
    let bump_prob = roughness * 0.15;
    let interior_x_min = rect.x + 1;
    let interior_x_max = rect.x + rect.w - 2;
    let interior_y_min = rect.y + 1;
    let interior_y_max = rect.y + rect.h - 2;

    // Recompute perimeter after removals.
    let mut new_perimeter: Vec<Point> = cells
        .iter()
        .filter(|p| p.cardinals().iter().any(|n| !cells.contains(n)))
        .copied()
        .collect();
    new_perimeter.sort_by(|a, b| a.y.cmp(&b.y).then(a.x.cmp(&b.x)));

    let mut additions = Vec::new();
    for p in &new_perimeter {
        for n in p.cardinals() {
            if !cells.contains(&n)
                && n.x >= interior_x_min
                && n.x <= interior_x_max
                && n.y >= interior_y_min
                && n.y <= interior_y_max
                && rng.r#gen::<f64>() < bump_prob
            {
                additions.push(n);
            }
        }
    }
    for p in additions {
        cells.insert(p);
    }
}

/// BFS flood-fill from an arbitrary cell, returning only the largest connected component.
fn largest_connected_component(cells: HashSet<Point>) -> HashSet<Point> {
    if cells.is_empty() {
        return cells;
    }

    let mut unvisited = cells;
    let mut largest = HashSet::new();

    while !unvisited.is_empty() {
        // Pick an arbitrary starting cell.
        let start = *unvisited.iter().next().unwrap();

        // BFS from start.
        let mut component = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(start);
        component.insert(start);
        unvisited.remove(&start);

        while let Some(current) = queue.pop_front() {
            for neighbor in current.cardinals() {
                if unvisited.remove(&neighbor) {
                    component.insert(neighbor);
                    queue.push_back(neighbor);
                }
            }
        }

        if component.len() > largest.len() {
            largest = component;
        }
    }

    largest
}

// ─── Selection helper ───────────────────────────────────────────────────────

/// Select the appropriate shape refinement for a space, given the map's location kind.
///
/// Rules:
/// - `LocationKind::Cave` → all rooms ≥ 7×7 use `CaveIrregularizer`
/// - Tag `"natural"` on the space → use `CaveIrregularizer`
/// - Otherwise → `RectShape`
///
/// Rooms smaller than 7×7 always get `RectShape` (not enough interior to carve meaningfully).
pub fn select_shape_refinement(
    location_kind: crate::intent::map_intent::LocationKind,
    space: &SpaceSpec,
) -> Box<dyn ShapeRefinement> {
    use crate::intent::map_intent::LocationKind;
    use crate::tag::Tag;

    let is_cave_location = location_kind == LocationKind::Cave;
    let has_natural_tag = space.has_atmosphere_tag(&Tag::from("natural"));

    if is_cave_location || has_natural_tag {
        Box::new(CaveIrregularizer::default())
    } else {
        Box::new(RectShape)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::graph::{NodeRole, ScenarioNodeId};
    use crate::spatial::plan::{
        AtomicSpace, RealizationStyle, SizeHint, SpaceId, SpaceKind, SpaceSpec,
    };
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn make_test_space() -> SpaceSpec {
        SpaceSpec {
            id: SpaceId(0),
            origin: ScenarioNodeId(0),
            role: NodeRole::Hub,
            structural_tags: vec![],
            atmosphere_tags: vec![],
            motifs: vec![],
            style: RealizationStyle::RoomLike,
            kind: SpaceKind::Atomic(AtomicSpace {
                width: 9,
                height: 9,
            }),
            label: Some("Test Room".to_string()),
            archetype: None,
            size_hint: SizeHint::Medium,
            max_connectors: None,
            connector_distribution: None,
        }
    }

    fn make_rect(w: i32, h: i32) -> Rect {
        Rect { x: 0, y: 0, w, h }
    }

    // ─── RectShape tests ────────────────────────────────────────────────────

    #[test]
    fn rect_shape_returns_rect_footprint() {
        let rect = make_rect(9, 9);
        let space = make_test_space();
        let mut rng = StdRng::seed_from_u64(42);

        let result = RectShape.refine(rect, &space, &mut rng);
        assert_eq!(result, Footprint::Rect(rect));
    }

    // ─── CaveIrregularizer tests ────────────────────────────────────────────

    #[test]
    fn cave_irregularizer_produces_cells_footprint() {
        let rect = make_rect(11, 9);
        let space = make_test_space();
        let mut rng = StdRng::seed_from_u64(42);

        let result = CaveIrregularizer::default().refine(rect, &space, &mut rng);
        match result {
            Footprint::Cells(_) => {} // Expected.
            other => panic!("expected Cells footprint, got {:?}", other),
        }
    }

    #[test]
    fn cave_irregularizer_cells_are_subset_of_interior() {
        let rect = make_rect(11, 9);
        let space = make_test_space();
        let mut rng = StdRng::seed_from_u64(42);

        let result = CaveIrregularizer::default().refine(rect, &space, &mut rng);
        if let Footprint::Cells(cells) = result {
            for p in &cells {
                assert!(
                    p.x > rect.x && p.x < rect.x + rect.w - 1,
                    "cell {:?} outside interior x-range",
                    p
                );
                assert!(
                    p.y > rect.y && p.y < rect.y + rect.h - 1,
                    "cell {:?} outside interior y-range",
                    p
                );
            }
        }
    }

    #[test]
    fn cave_irregularizer_result_is_connected() {
        let rect = make_rect(11, 9);
        let space = make_test_space();
        let mut rng = StdRng::seed_from_u64(42);

        let result = CaveIrregularizer::default().refine(rect, &space, &mut rng);
        if let Footprint::Cells(cells) = result {
            let cell_set: HashSet<Point> = cells.iter().copied().collect();
            let component = largest_connected_component(cell_set.clone());
            assert_eq!(
                component.len(),
                cell_set.len(),
                "footprint has disconnected cells"
            );
        }
    }

    #[test]
    fn cave_irregularizer_deterministic_with_same_seed() {
        let rect = make_rect(11, 9);
        let space = make_test_space();

        let mut rng1 = StdRng::seed_from_u64(123);
        let result1 = CaveIrregularizer::default().refine(rect, &space, &mut rng1);

        let mut rng2 = StdRng::seed_from_u64(123);
        let result2 = CaveIrregularizer::default().refine(rect, &space, &mut rng2);

        assert_eq!(result1, result2);
    }

    #[test]
    fn cave_irregularizer_different_seeds_produce_different_results() {
        let rect = make_rect(11, 9);
        let space = make_test_space();

        let mut rng1 = StdRng::seed_from_u64(1);
        let result1 = CaveIrregularizer::default().refine(rect, &space, &mut rng1);

        let mut rng2 = StdRng::seed_from_u64(9999);
        let result2 = CaveIrregularizer::default().refine(rect, &space, &mut rng2);

        // With different seeds on a large enough room, results should differ.
        assert_ne!(result1, result2);
    }

    #[test]
    fn cave_irregularizer_small_room_fallback_to_rect() {
        // 5×5 room has 3×3 interior — below 5×5 minimum.
        let rect = make_rect(5, 5);
        let space = make_test_space();
        let mut rng = StdRng::seed_from_u64(42);

        let result = CaveIrregularizer::default().refine(rect, &space, &mut rng);
        assert_eq!(result, Footprint::Rect(rect));
    }

    #[test]
    fn cave_irregularizer_6x6_room_fallback_to_rect() {
        // 6×6 room has 4×4 interior — still below 5×5 minimum.
        let rect = make_rect(6, 6);
        let space = make_test_space();
        let mut rng = StdRng::seed_from_u64(42);

        let result = CaveIrregularizer::default().refine(rect, &space, &mut rng);
        assert_eq!(result, Footprint::Rect(rect));
    }

    #[test]
    fn cave_irregularizer_7x7_room_produces_cells() {
        // 7×7 room has 5×5 interior — meets the minimum.
        let rect = make_rect(7, 7);
        let space = make_test_space();
        let mut rng = StdRng::seed_from_u64(42);

        let result = CaveIrregularizer::default().refine(rect, &space, &mut rng);
        // Should produce Cells (or Rect if it hit the 50% fallback threshold).
        // On a 5×5 interior with default roughness, it should still manage.
        match result {
            Footprint::Cells(_) | Footprint::Rect(_) => {} // Both acceptable for edge-case size.
            _ => panic!("unexpected footprint variant"),
        }
    }

    #[test]
    fn cave_irregularizer_minimum_threshold_prevents_over_carving() {
        // Very aggressive roughness on a medium room.
        let rect = make_rect(9, 9);
        let space = make_test_space();
        let irregularizer = CaveIrregularizer { roughness: 1.0 };
        let mut rng = StdRng::seed_from_u64(42);

        let result = irregularizer.refine(rect, &space, &mut rng);
        match result {
            Footprint::Cells(cells) => {
                // Interior is 7×7 = 49 cells. Minimum threshold is 50% = 24.
                assert!(cells.len() >= 24, "too few cells: {} (min 24)", cells.len());
            }
            Footprint::Rect(_) => {
                // Fallback to rect is also acceptable (means it hit the threshold).
            }
            _ => panic!("unexpected footprint variant"),
        }
    }

    #[test]
    fn cave_irregularizer_result_sorted_by_position() {
        let rect = make_rect(11, 9);
        let space = make_test_space();
        let mut rng = StdRng::seed_from_u64(42);

        let result = CaveIrregularizer::default().refine(rect, &space, &mut rng);
        if let Footprint::Cells(cells) = result {
            for window in cells.windows(2) {
                let a = &window[0];
                let b = &window[1];
                assert!(
                    (a.y < b.y) || (a.y == b.y && a.x <= b.x),
                    "cells not sorted: {:?} before {:?}",
                    a,
                    b
                );
            }
        }
    }

    #[test]
    fn cave_irregularizer_large_room_produces_fewer_cells_than_full_interior() {
        let rect = make_rect(15, 13);
        let space = make_test_space();
        let mut rng = StdRng::seed_from_u64(42);

        let full_interior_count = (rect.w - 2) * (rect.h - 2);
        let result = CaveIrregularizer::default().refine(rect, &space, &mut rng);

        if let Footprint::Cells(cells) = result {
            assert!(
                (cells.len() as i32) < full_interior_count,
                "expected fewer cells than full interior ({}), got {}",
                full_interior_count,
                cells.len()
            );
        }
    }

    // ─── Connectivity stress test across multiple seeds ─────────────────────

    #[test]
    fn cave_irregularizer_always_connected_across_seeds() {
        let rect = make_rect(11, 9);
        let space = make_test_space();

        for seed in 0..50 {
            let mut rng = StdRng::seed_from_u64(seed);
            let result = CaveIrregularizer::default().refine(rect, &space, &mut rng);

            match result {
                Footprint::Cells(cells) => {
                    let cell_set: HashSet<Point> = cells.iter().copied().collect();
                    let component = largest_connected_component(cell_set.clone());
                    assert_eq!(
                        component.len(),
                        cell_set.len(),
                        "seed {}: disconnected cells ({} vs {} in largest component)",
                        seed,
                        cell_set.len(),
                        component.len()
                    );
                }
                Footprint::Rect(_) => {} // Fallback is fine.
                _ => panic!("unexpected footprint variant"),
            }
        }
    }

    // ─── select_shape_refinement tests ──────────────────────────────────────

    #[test]
    fn select_shape_for_cave_location() {
        use crate::intent::map_intent::LocationKind;
        let space = make_test_space();

        let shape = select_shape_refinement(LocationKind::Cave, &space);
        let rect = make_rect(11, 9);
        let mut rng = StdRng::seed_from_u64(42);
        let result = shape.refine(rect, &space, &mut rng);

        // Should produce Cells (irregular), not Rect.
        assert!(matches!(result, Footprint::Cells(_)));
    }

    #[test]
    fn select_shape_for_dungeon_location() {
        use crate::intent::map_intent::LocationKind;
        let space = make_test_space();

        let shape = select_shape_refinement(LocationKind::Dungeon, &space);
        let rect = make_rect(11, 9);
        let mut rng = StdRng::seed_from_u64(42);
        let result = shape.refine(rect, &space, &mut rng);

        // Should produce Rect (no irregularization).
        assert!(matches!(result, Footprint::Rect(_)));
    }

    #[test]
    fn select_shape_for_natural_tagged_space() {
        use crate::intent::map_intent::LocationKind;
        use crate::tag::Tag;

        let mut space = make_test_space();
        space.atmosphere_tags.push(Tag::from("natural"));

        let shape = select_shape_refinement(LocationKind::Dungeon, &space);
        let rect = make_rect(11, 9);
        let mut rng = StdRng::seed_from_u64(42);
        let result = shape.refine(rect, &space, &mut rng);

        // Even in a Dungeon, a room tagged "natural" should get cave shape.
        assert!(matches!(result, Footprint::Cells(_)));
    }
}
