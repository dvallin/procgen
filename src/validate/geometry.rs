//! Geometry validation: checks a `GeometryPlan` for spatial correctness.
//!
//! Implemented checks:
//! - **Overlap**: no two placed spaces have overlapping footprints.
//! - **Minimum spacing**: placed spaces maintain a configurable gap.
//! - **Link endpoint validity**: corridor start/end lie on room boundaries.
//! - **Link connectivity**: every link references existing spaces.
//! - **Corridor–room intersection**: corridor segments don't pass through
//!   rooms they aren't connected to.
//! - **Corridor–corridor overlap**: detects shared cells between different
//!   corridors (warning) and shared endpoint cells (error — door overwrite risk).

use std::collections::HashSet;

use crate::geometry::geom::{GeometryPlan, PlacedLink, PlacedSpace};
use crate::validate::{Severity, ValidationIssue, ValidationResult, Validator};

/// Configuration for geometry validation thresholds.
#[derive(Debug, Clone)]
pub struct GeometryValidatorConfig {
    /// Minimum gap (in tiles) required between any two placed spaces.
    /// A value of 0 means they may touch but not overlap.
    /// A value of 1 means at least 1 tile of empty space between them.
    pub min_spacing: i32,
}

impl Default for GeometryValidatorConfig {
    fn default() -> Self {
        Self { min_spacing: 1 }
    }
}

/// Validates a `GeometryPlan` for spatial correctness.
#[derive(Default)]
pub struct GeometryValidator {
    pub config: GeometryValidatorConfig,
}

impl GeometryValidator {
    pub fn new(config: GeometryValidatorConfig) -> Self {
        Self { config }
    }
}

impl Validator<GeometryPlan> for GeometryValidator {
    fn validate(&self, plan: &GeometryPlan) -> ValidationResult {
        let mut issues = Vec::new();

        check_overlaps(&plan.spaces, &mut issues);
        check_minimum_spacing(&plan.spaces, self.config.min_spacing, &mut issues);
        check_link_connectivity(plan, &mut issues);
        check_link_endpoint_validity(plan, &mut issues);
        check_corridor_room_intersection(plan, &mut issues);
        check_corridor_corridor_overlap(plan, &mut issues);

        ValidationResult { issues }
    }
}

// --- Individual check functions (public for targeted unit testing) ---

/// Check that no two placed spaces have overlapping bounding rects.
pub fn check_overlaps(spaces: &[PlacedSpace], issues: &mut Vec<ValidationIssue>) {
    for i in 0..spaces.len() {
        for j in (i + 1)..spaces.len() {
            let a = &spaces[i];
            let b = &spaces[j];
            let rect_a = a.footprint.bounding_rect();
            let rect_b = b.footprint.bounding_rect();
            if rect_a.overlaps(&rect_b) {
                issues.push(ValidationIssue {
                    severity: Severity::Error,
                    message: format!(
                        "Spaces {:?} and {:?} overlap: ({},{} {}x{}) vs ({},{} {}x{})",
                        a.space_id,
                        b.space_id,
                        rect_a.x,
                        rect_a.y,
                        rect_a.w,
                        rect_a.h,
                        rect_b.x,
                        rect_b.y,
                        rect_b.w,
                        rect_b.h,
                    ),
                });
            }
        }
    }
}

/// Check that placed spaces maintain at least `min_spacing` tiles of gap.
/// Uses `Rect::inflate` + `Rect::overlaps` to detect insufficient gaps.
pub fn check_minimum_spacing(
    spaces: &[PlacedSpace],
    min_spacing: i32,
    issues: &mut Vec<ValidationIssue>,
) {
    if min_spacing <= 0 {
        return;
    }
    for i in 0..spaces.len() {
        for j in (i + 1)..spaces.len() {
            let a = &spaces[i];
            let b = &spaces[j];
            let rect_a = a.footprint.bounding_rect();
            let rect_b = b.footprint.bounding_rect();
            if rect_a.inflate(min_spacing).overlaps(&rect_b) {
                let gap = rect_a.gap_to(&rect_b);
                issues.push(ValidationIssue {
                    severity: Severity::Warning,
                    message: format!(
                        "Spaces {:?} and {:?} are too close: gap={}, required={}",
                        a.space_id, b.space_id, gap, min_spacing,
                    ),
                });
            }
        }
    }
}

/// Check that every link's start/end points reference spaces in the plan
/// (i.e. lie on some space's boundary).
pub fn check_link_connectivity(plan: &GeometryPlan, issues: &mut Vec<ValidationIssue>) {
    let space_ids: HashSet<_> = plan.spaces.iter().map(|s| s.space_id).collect();

    for (idx, link) in plan.links.iter().enumerate() {
        if link.points.len() < 2 {
            issues.push(ValidationIssue {
                severity: Severity::Error,
                message: format!(
                    "Link {} has fewer than 2 points ({} points)",
                    idx,
                    link.points.len()
                ),
            });
            continue;
        }

        let start = link.points.first().unwrap();
        let end = link.points.last().unwrap();

        let from_space = plan
            .spaces
            .iter()
            .find(|s| s.footprint.bounding_rect().point_on_boundary(start));
        let to_space = plan
            .spaces
            .iter()
            .find(|s| s.footprint.bounding_rect().point_on_boundary(end));

        if from_space.is_none() {
            issues.push(ValidationIssue {
                severity: Severity::Error,
                message: format!(
                    "Link {} start ({},{}) is not on any space boundary",
                    idx, start.x, start.y,
                ),
            });
        }
        if to_space.is_none() {
            issues.push(ValidationIssue {
                severity: Severity::Error,
                message: format!(
                    "Link {} end ({},{}) is not on any space boundary",
                    idx, end.x, end.y,
                ),
            });
        }

        // Sanity: the spaces found should actually be in the plan.
        if let (Some(from), Some(to)) = (from_space, to_space)
            && (!space_ids.contains(&from.space_id) || !space_ids.contains(&to.space_id))
        {
            issues.push(ValidationIssue {
                severity: Severity::Error,
                message: format!("Link {} connects spaces not in the plan", idx),
            });
        }
    }
}

/// Check that link endpoints lie exactly on the boundary of a placed space.
pub fn check_link_endpoint_validity(plan: &GeometryPlan, issues: &mut Vec<ValidationIssue>) {
    for (idx, link) in plan.links.iter().enumerate() {
        if link.points.len() < 2 {
            continue; // Already caught by connectivity check.
        }

        let start = &link.points[0];
        let end = &link.points[link.points.len() - 1];

        let start_on_boundary = plan
            .spaces
            .iter()
            .filter(|s| s.footprint.bounding_rect().point_on_boundary(start))
            .count();
        if start_on_boundary == 0 {
            issues.push(ValidationIssue {
                severity: Severity::Error,
                message: format!(
                    "Link {} start ({},{}) is not on any room boundary",
                    idx, start.x, start.y,
                ),
            });
        }

        let end_on_boundary = plan
            .spaces
            .iter()
            .filter(|s| s.footprint.bounding_rect().point_on_boundary(end))
            .count();
        if end_on_boundary == 0 {
            issues.push(ValidationIssue {
                severity: Severity::Error,
                message: format!(
                    "Link {} end ({},{}) is not on any room boundary",
                    idx, end.x, end.y,
                ),
            });
        }
    }
}

/// Check that corridor segments don't pass through the interior of rooms
/// they aren't connected to.
/// Check for corridor-corridor overlaps: cells shared by two different links.
/// Reports a Warning for shared corridor cells (visual merging) and an Error
/// when a corridor segment passes through another corridor's endpoint (door overwrite risk).
pub fn check_corridor_corridor_overlap(plan: &GeometryPlan, issues: &mut Vec<ValidationIssue>) {
    if plan.links.len() < 2 {
        return;
    }

    // Collect all cells for each corridor, and track endpoints separately.
    let corridor_cells: Vec<HashSet<(i32, i32)>> =
        plan.links.iter().map(collect_corridor_cells).collect();

    let corridor_endpoints: Vec<HashSet<(i32, i32)>> = plan
        .links
        .iter()
        .map(|link| {
            let mut eps = HashSet::new();
            if let Some(p) = link.points.first() {
                eps.insert((p.x, p.y));
            }
            if let Some(p) = link.points.last() {
                eps.insert((p.x, p.y));
            }
            eps
        })
        .collect();

    for i in 0..plan.links.len() {
        for j in (i + 1)..plan.links.len() {
            let shared: Vec<_> = corridor_cells[i]
                .intersection(&corridor_cells[j])
                .copied()
                .collect();

            if shared.is_empty() {
                continue;
            }

            // Check if any shared cell is an endpoint of either corridor
            // (door overwrite risk).
            let mut endpoint_conflicts = Vec::new();
            let mut interior_overlaps = Vec::new();

            for &cell in &shared {
                if corridor_endpoints[i].contains(&cell) || corridor_endpoints[j].contains(&cell) {
                    endpoint_conflicts.push(cell);
                } else {
                    interior_overlaps.push(cell);
                }
            }

            if !endpoint_conflicts.is_empty() {
                issues.push(ValidationIssue {
                    severity: Severity::Error,
                    message: format!(
                        "Links {} and {} share endpoint cell(s) {:?} — door overwrite risk",
                        i, j, endpoint_conflicts,
                    ),
                });
            }

            if !interior_overlaps.is_empty() {
                issues.push(ValidationIssue {
                    severity: Severity::Warning,
                    message: format!(
                        "Links {} and {} share {} corridor cell(s) (visual merging)",
                        i,
                        j,
                        interior_overlaps.len(),
                    ),
                });
            }
        }
    }
}

/// Collect all cells that a corridor polyline passes through.
fn collect_corridor_cells(link: &PlacedLink) -> HashSet<(i32, i32)> {
    let mut cells = HashSet::new();
    for window in link.points.windows(2) {
        let a = &window[0];
        let b = &window[1];
        if a.x == b.x {
            let (from, to) = if a.y <= b.y { (a.y, b.y) } else { (b.y, a.y) };
            for y in from..=to {
                cells.insert((a.x, y));
            }
        } else if a.y == b.y {
            let (from, to) = if a.x <= b.x { (a.x, b.x) } else { (b.x, a.x) };
            for x in from..=to {
                cells.insert((x, a.y));
            }
        }
    }
    cells
}

pub fn check_corridor_room_intersection(plan: &GeometryPlan, issues: &mut Vec<ValidationIssue>) {
    for (idx, link) in plan.links.iter().enumerate() {
        if link.points.len() < 2 {
            continue;
        }

        // Determine which spaces are "endpoint spaces" for this link.
        let start = &link.points[0];
        let end = &link.points[link.points.len() - 1];
        let endpoint_spaces: HashSet<_> = plan
            .spaces
            .iter()
            .filter(|s| {
                let r = s.footprint.bounding_rect();
                r.point_on_boundary(start) || r.point_on_boundary(end)
            })
            .map(|s| s.space_id)
            .collect();

        // Check each segment of the corridor polyline.
        for seg_idx in 0..link.points.len() - 1 {
            let p0 = &link.points[seg_idx];
            let p1 = &link.points[seg_idx + 1];

            for space in &plan.spaces {
                if endpoint_spaces.contains(&space.space_id) {
                    continue;
                }

                let r = space.footprint.bounding_rect();
                if r.segment_crosses_interior(p0, p1) {
                    issues.push(ValidationIssue {
                        severity: Severity::Warning,
                        message: format!(
                            "Link {} segment ({},{})→({},{}) passes through interior of space {:?}",
                            idx, p0.x, p0.y, p1.x, p1.y, space.space_id,
                        ),
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::geom::{Footprint, LinkKind, PlacedLink, Point, Rect};
    use crate::spatial::plan::{RealizationStyle, SpaceId};

    /// Helper to create a simple placed space with a rect footprint.
    fn make_space(id: u32, x: i32, y: i32, w: i32, h: i32) -> PlacedSpace {
        let rect = Rect { x, y, w, h };
        PlacedSpace {
            space_id: SpaceId(id),
            rect,
            footprint: Footprint::Rect(rect),
            style: RealizationStyle::RoomLike,
            label: None,
        }
    }

    /// Helper to create a link with given points.
    fn make_link(points: Vec<Point>) -> PlacedLink {
        PlacedLink {
            kind: LinkKind::Normal,
            points,
        }
    }

    // --- Overlap check tests ---

    #[test]
    fn no_overlap_passes() {
        let spaces = vec![make_space(0, 0, 0, 5, 5), make_space(1, 10, 0, 5, 5)];
        let mut issues = Vec::new();
        check_overlaps(&spaces, &mut issues);
        assert!(issues.is_empty());
    }

    #[test]
    fn overlap_detected() {
        let spaces = vec![
            make_space(0, 0, 0, 5, 5),
            make_space(1, 3, 3, 5, 5), // overlaps at (3,3)→(4,4)
        ];
        let mut issues = Vec::new();
        check_overlaps(&spaces, &mut issues);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].severity, Severity::Error);
        assert!(issues[0].message.contains("overlap"));
    }

    #[test]
    fn touching_edges_do_not_overlap() {
        let spaces = vec![
            make_space(0, 0, 0, 5, 5),
            make_space(1, 5, 0, 5, 5), // starts exactly where first ends
        ];
        let mut issues = Vec::new();
        check_overlaps(&spaces, &mut issues);
        assert!(issues.is_empty());
    }

    #[test]
    fn multiple_overlaps_all_reported() {
        let spaces = vec![
            make_space(0, 0, 0, 5, 5),
            make_space(1, 3, 0, 5, 5),
            make_space(2, 1, 1, 5, 5),
        ];
        let mut issues = Vec::new();
        check_overlaps(&spaces, &mut issues);
        assert_eq!(issues.len(), 3);
    }

    // --- Minimum spacing tests ---

    #[test]
    fn sufficient_spacing_passes() {
        let spaces = vec![
            make_space(0, 0, 0, 5, 5),
            make_space(1, 8, 0, 5, 5), // gap of 3 tiles
        ];
        let mut issues = Vec::new();
        check_minimum_spacing(&spaces, 2, &mut issues);
        assert!(issues.is_empty());
    }

    #[test]
    fn insufficient_spacing_warns() {
        let spaces = vec![
            make_space(0, 0, 0, 5, 5),
            make_space(1, 6, 0, 5, 5), // gap of 1 tile
        ];
        let mut issues = Vec::new();
        check_minimum_spacing(&spaces, 2, &mut issues);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].severity, Severity::Warning);
        assert!(issues[0].message.contains("too close"));
    }

    #[test]
    fn zero_spacing_allows_touching() {
        let spaces = vec![
            make_space(0, 0, 0, 5, 5),
            make_space(1, 5, 0, 5, 5), // touching
        ];
        let mut issues = Vec::new();
        check_minimum_spacing(&spaces, 0, &mut issues);
        assert!(issues.is_empty());
    }

    #[test]
    fn spacing_check_with_vertical_gap() {
        let spaces = vec![
            make_space(0, 0, 0, 5, 5),
            make_space(1, 0, 7, 5, 5), // gap of 2 vertically
        ];
        let mut issues = Vec::new();
        check_minimum_spacing(&spaces, 2, &mut issues);
        assert!(issues.is_empty());
    }

    // --- Link connectivity tests ---

    #[test]
    fn valid_links_pass_connectivity() {
        let plan = GeometryPlan {
            spaces: vec![make_space(0, 0, 0, 7, 7), make_space(1, 12, 0, 7, 7)],
            links: vec![make_link(vec![
                Point { x: 6, y: 3 },  // east edge of space 0
                Point { x: 12, y: 3 }, // west edge of space 1
            ])],
        };
        let mut issues = Vec::new();
        check_link_connectivity(&plan, &mut issues);
        assert!(issues.is_empty());
    }

    #[test]
    fn link_with_dangling_start() {
        let plan = GeometryPlan {
            spaces: vec![make_space(0, 0, 0, 7, 7), make_space(1, 12, 0, 7, 7)],
            links: vec![make_link(vec![
                Point { x: 50, y: 50 }, // nowhere near any space
                Point { x: 12, y: 3 },  // west edge of space 1
            ])],
        };
        let mut issues = Vec::new();
        check_link_connectivity(&plan, &mut issues);
        assert!(issues.iter().any(|i| i.severity == Severity::Error));
        assert!(issues.iter().any(|i| i.message.contains("start")));
    }

    #[test]
    fn link_with_too_few_points() {
        let plan = GeometryPlan {
            spaces: vec![make_space(0, 0, 0, 7, 7)],
            links: vec![make_link(vec![Point { x: 0, y: 3 }])],
        };
        let mut issues = Vec::new();
        check_link_connectivity(&plan, &mut issues);
        assert!(issues.iter().any(|i| i.message.contains("fewer than 2")));
    }

    // --- Link endpoint validity tests ---

    #[test]
    fn endpoints_on_boundary_pass() {
        let plan = GeometryPlan {
            spaces: vec![make_space(0, 0, 0, 7, 7), make_space(1, 12, 0, 7, 7)],
            links: vec![make_link(vec![
                Point { x: 6, y: 3 },  // east edge of space 0
                Point { x: 12, y: 3 }, // west edge of space 1
            ])],
        };
        let mut issues = Vec::new();
        check_link_endpoint_validity(&plan, &mut issues);
        assert!(issues.is_empty());
    }

    #[test]
    fn endpoint_in_interior_fails() {
        let plan = GeometryPlan {
            spaces: vec![make_space(0, 0, 0, 7, 7), make_space(1, 12, 0, 7, 7)],
            links: vec![make_link(vec![
                Point { x: 3, y: 3 },  // center of space 0, NOT boundary
                Point { x: 12, y: 3 }, // west edge of space 1
            ])],
        };
        let mut issues = Vec::new();
        check_link_endpoint_validity(&plan, &mut issues);
        assert!(issues.iter().any(|i| i.severity == Severity::Error));
    }

    // --- Corridor–room intersection tests ---

    #[test]
    fn corridor_avoids_rooms_passes() {
        let plan = GeometryPlan {
            spaces: vec![make_space(0, 0, 0, 7, 7), make_space(1, 20, 0, 7, 7)],
            links: vec![make_link(vec![
                Point { x: 6, y: 3 },  // east edge of space 0
                Point { x: 20, y: 3 }, // west edge of space 1
            ])],
        };
        let mut issues = Vec::new();
        check_corridor_room_intersection(&plan, &mut issues);
        assert!(issues.is_empty());
    }

    #[test]
    fn corridor_through_unrelated_room_warns() {
        let plan = GeometryPlan {
            spaces: vec![
                make_space(0, 0, 0, 7, 7),
                make_space(1, 20, 0, 7, 7),
                make_space(2, 10, 0, 7, 7), // blocking room
            ],
            links: vec![make_link(vec![
                Point { x: 6, y: 3 },  // east edge of space 0
                Point { x: 20, y: 3 }, // west edge of space 1
            ])],
        };
        let mut issues = Vec::new();
        check_corridor_room_intersection(&plan, &mut issues);
        assert!(!issues.is_empty());
        assert!(issues[0].message.contains("passes through interior"));
    }

    #[test]
    fn corridor_on_room_boundary_is_ok() {
        let plan = GeometryPlan {
            spaces: vec![
                make_space(0, 0, 0, 7, 7),
                make_space(1, 14, 0, 7, 7),
                make_space(2, 8, 0, 3, 7), // narrow room (interior is x=9 only)
            ],
            links: vec![make_link(vec![
                Point { x: 6, y: 3 },  // east edge of space 0
                Point { x: 14, y: 3 }, // west edge of space 1
            ])],
        };
        let mut issues = Vec::new();
        check_corridor_room_intersection(&plan, &mut issues);
        // Detects intersection because x=9, y=3 is interior of space 2.
        assert!(!issues.is_empty());
    }

    #[test]
    fn z_shaped_corridor_through_room_detected() {
        let plan = GeometryPlan {
            spaces: vec![
                make_space(0, 0, 0, 7, 7),
                make_space(1, 0, 20, 7, 7),
                make_space(2, 0, 10, 7, 5), // blocking room in the path
            ],
            links: vec![make_link(vec![
                Point { x: 3, y: 6 },  // south edge of space 0
                Point { x: 3, y: 13 }, // intermediate (through space 2!)
                Point { x: 3, y: 20 }, // north edge of space 1
            ])],
        };
        let mut issues = Vec::new();
        check_corridor_room_intersection(&plan, &mut issues);
        assert!(!issues.is_empty());
        assert!(issues[0].message.contains("passes through interior"));
    }

    // --- Corridor–corridor overlap tests ---

    #[test]
    fn disjoint_corridors_no_overlap() {
        let plan = GeometryPlan {
            spaces: vec![
                make_space(0, 0, 0, 7, 7),
                make_space(1, 20, 0, 7, 7),
                make_space(2, 0, 20, 7, 7),
            ],
            links: vec![
                make_link(vec![Point { x: 6, y: 3 }, Point { x: 20, y: 3 }]),
                make_link(vec![Point { x: 3, y: 6 }, Point { x: 3, y: 20 }]),
            ],
        };
        let mut issues = Vec::new();
        check_corridor_corridor_overlap(&plan, &mut issues);
        assert!(issues.is_empty());
    }

    #[test]
    fn shared_interior_cells_warn() {
        // Two corridors share a vertical segment (no endpoint overlap).
        let plan = GeometryPlan {
            spaces: vec![
                make_space(0, 0, 0, 7, 7),
                make_space(1, 0, 20, 7, 7),
                make_space(2, 0, 30, 7, 7),
            ],
            links: vec![
                // Link 0: goes south from space 0, turns east to space 1
                make_link(vec![
                    Point { x: 3, y: 6 },
                    Point { x: 3, y: 15 },
                    Point { x: 3, y: 20 },
                ]),
                // Link 1: also goes south from space 0 through same column
                make_link(vec![
                    Point { x: 3, y: 6 },
                    Point { x: 3, y: 25 },
                    Point { x: 3, y: 30 },
                ]),
            ],
        };
        let mut issues = Vec::new();
        check_corridor_corridor_overlap(&plan, &mut issues);
        // Should detect shared cells (at minimum the overlapping segment y=6..20)
        assert!(
            !issues.is_empty(),
            "Expected corridor overlap warnings/errors"
        );
    }

    #[test]
    fn shared_endpoint_is_error() {
        // Link 1's segment passes through Link 0's endpoint — door overwrite risk.
        let plan = GeometryPlan {
            spaces: vec![
                make_space(0, 0, 0, 7, 7),
                make_space(1, 0, 15, 7, 7),
                make_space(2, 0, 30, 7, 7),
            ],
            links: vec![
                // Link 0: ends at (3, 15) on space 1's boundary
                make_link(vec![Point { x: 3, y: 6 }, Point { x: 3, y: 15 }]),
                // Link 1: passes straight through (3, 15) en route to space 2
                make_link(vec![Point { x: 3, y: 10 }, Point { x: 3, y: 30 }]),
            ],
        };
        let mut issues = Vec::new();
        check_corridor_corridor_overlap(&plan, &mut issues);
        let errors: Vec<_> = issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .collect();
        assert!(
            !errors.is_empty(),
            "Expected error for endpoint cell overlap, got: {:?}",
            issues
        );
    }

    #[test]
    fn single_link_no_overlap_check() {
        let plan = GeometryPlan {
            spaces: vec![make_space(0, 0, 0, 7, 7), make_space(1, 20, 0, 7, 7)],
            links: vec![make_link(vec![Point { x: 6, y: 3 }, Point { x: 20, y: 3 }])],
        };
        let mut issues = Vec::new();
        check_corridor_corridor_overlap(&plan, &mut issues);
        assert!(issues.is_empty());
    }

    // --- Full validator integration test ---

    #[test]
    fn valid_geometry_plan_passes_all_checks() {
        let plan = GeometryPlan {
            spaces: vec![
                make_space(0, 0, 0, 7, 7),
                make_space(1, 11, 0, 7, 7), // gap of 4
            ],
            links: vec![make_link(vec![
                Point { x: 6, y: 3 },  // east edge of space 0
                Point { x: 11, y: 3 }, // west edge of space 1
            ])],
        };

        let validator = GeometryValidator::default();
        let result = validator.validate(&plan);
        assert!(
            result.is_ok(),
            "Expected no errors, got: {:?}",
            result.issues
        );
    }

    #[test]
    fn overlapping_plan_fails_validation() {
        let plan = GeometryPlan {
            spaces: vec![
                make_space(0, 0, 0, 7, 7),
                make_space(1, 5, 0, 7, 7), // overlaps!
            ],
            links: vec![],
        };

        let validator = GeometryValidator::default();
        let result = validator.validate(&plan);
        assert!(!result.is_ok());
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use crate::geometry::geom::{Footprint, LinkKind, PlacedLink, Point, Rect};
    use crate::spatial::plan::{RealizationStyle, SpaceId};
    use proptest::prelude::*;

    /// Strategy to generate a valid non-overlapping grid of spaces.
    /// Places rooms on a grid with guaranteed spacing.
    fn non_overlapping_spaces_strategy(count: usize) -> impl Strategy<Value = Vec<PlacedSpace>> {
        let grid_cols = (count as f64).sqrt().ceil() as usize;
        // Each cell is 20x20 with rooms of size 3..=9
        proptest::collection::vec((3i32..=9, 3i32..=9), count).prop_map(move |sizes| {
            sizes
                .iter()
                .enumerate()
                .map(|(i, &(w, h))| {
                    let col = i % grid_cols;
                    let row = i / grid_cols;
                    let x = (col as i32) * 20;
                    let y = (row as i32) * 20;
                    let rect = Rect { x, y, w, h };
                    PlacedSpace {
                        space_id: SpaceId(i as u32),
                        rect,
                        footprint: Footprint::Rect(rect),
                        style: RealizationStyle::RoomLike,
                        label: None,
                    }
                })
                .collect()
        })
    }

    proptest! {
        /// Non-overlapping grid placement should never report overlap errors.
        #[test]
        fn grid_placed_spaces_have_no_overlaps(spaces in non_overlapping_spaces_strategy(6)) {
            let mut issues = Vec::new();
            check_overlaps(&spaces, &mut issues);
            prop_assert!(issues.is_empty(), "Unexpected overlaps: {:?}", issues);
        }

        /// Non-overlapping grid with large spacing should pass spacing checks.
        #[test]
        fn grid_placed_spaces_have_sufficient_spacing(spaces in non_overlapping_spaces_strategy(6)) {
            let mut issues = Vec::new();
            // Grid spacing is 20, max room size is 9, so gap >= 11. min_spacing=2 should pass.
            check_minimum_spacing(&spaces, 2, &mut issues);
            prop_assert!(issues.is_empty(), "Unexpected spacing issues: {:?}", issues);
        }

        /// A link with endpoints on room boundaries should pass endpoint validity.
        #[test]
        fn valid_link_endpoints_always_pass(
            w0 in 5i32..=11,
            h0 in 5i32..=11,
            w1 in 5i32..=11,
            h1 in 5i32..=11,
        ) {
            let r0 = Rect { x: 0, y: 0, w: w0, h: h0 };
            let r1 = Rect { x: w0 + 5, y: 0, w: w1, h: h1 };
            let spaces = vec![
                PlacedSpace {
                    space_id: SpaceId(0),
                    rect: r0,
                    footprint: Footprint::Rect(r0),
                    style: RealizationStyle::RoomLike,
                    label: None,
                },
                PlacedSpace {
                    space_id: SpaceId(1),
                    rect: r1,
                    footprint: Footprint::Rect(r1),
                    style: RealizationStyle::RoomLike,
                    label: None,
                },
            ];
            // Link from east edge of r0 to west edge of r1.
            let link_y = (h0.min(h1) - 1).max(1);
            let link = PlacedLink {
                kind: LinkKind::Normal,
                points: vec![
                    Point { x: w0 - 1, y: link_y },
                    Point { x: w0 + 5, y: link_y },
                ],
            };
            let plan = GeometryPlan {
                spaces,
                links: vec![link],
            };
            let mut issues = Vec::new();
            check_link_endpoint_validity(&plan, &mut issues);
            prop_assert!(issues.is_empty(), "Endpoint issues: {:?}", issues);
        }

        /// Two rects that don't overlap should never trigger the overlap check.
        #[test]
        fn non_overlapping_rects_never_error(
            x0 in 0i32..50,
            y0 in 0i32..50,
            w0 in 3i32..=10,
            h0 in 3i32..=10,
            gap in 1i32..=20,
        ) {
            let r0 = Rect { x: x0, y: y0, w: w0, h: h0 };
            let r1 = Rect { x: x0 + w0 + gap, y: y0, w: 5, h: 5 };
            prop_assert!(!r0.overlaps(&r1));

            let spaces = vec![
                PlacedSpace {
                    space_id: SpaceId(0),
                    rect: r0,
                    footprint: Footprint::Rect(r0),
                    style: RealizationStyle::RoomLike,
                    label: None,
                },
                PlacedSpace {
                    space_id: SpaceId(1),
                    rect: r1,
                    footprint: Footprint::Rect(r1),
                    style: RealizationStyle::RoomLike,
                    label: None,
                },
            ];
            let mut issues = Vec::new();
            check_overlaps(&spaces, &mut issues);
            prop_assert!(issues.is_empty());
        }

        /// A corridor that stays strictly between two rooms should produce
        /// no intersection warnings.
        #[test]
        fn straight_corridor_between_two_rooms_no_intersection(
            h in 5i32..=11,
        ) {
            let r0 = Rect { x: 0, y: 0, w: 7, h };
            let r1 = Rect { x: 15, y: 0, w: 7, h };
            let spaces = vec![
                PlacedSpace {
                    space_id: SpaceId(0),
                    rect: r0,
                    footprint: Footprint::Rect(r0),
                    style: RealizationStyle::RoomLike,
                    label: None,
                },
                PlacedSpace {
                    space_id: SpaceId(1),
                    rect: r1,
                    footprint: Footprint::Rect(r1),
                    style: RealizationStyle::RoomLike,
                    label: None,
                },
            ];
            let mid_y = h / 2;
            let link = PlacedLink {
                kind: LinkKind::Normal,
                points: vec![
                    Point { x: 6, y: mid_y },
                    Point { x: 15, y: mid_y },
                ],
            };
            let plan = GeometryPlan {
                spaces,
                links: vec![link],
            };
            let mut issues = Vec::new();
            check_corridor_room_intersection(&plan, &mut issues);
            prop_assert!(issues.is_empty(), "Unexpected intersection: {:?}", issues);
        }
    }
}
