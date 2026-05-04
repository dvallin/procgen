use crate::spatial::plan::{RealizationStyle, SpaceId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    /// The four cardinal neighbors (N, S, W, E).
    pub fn cardinals(self) -> [Point; 4] {
        [
            Point {
                x: self.x,
                y: self.y - 1,
            },
            Point {
                x: self.x,
                y: self.y + 1,
            },
            Point {
                x: self.x - 1,
                y: self.y,
            },
            Point {
                x: self.x + 1,
                y: self.y,
            },
        ]
    }

    /// All eight neighbors (cardinal + diagonal).
    pub fn neighbors(self) -> [Point; 8] {
        [
            Point {
                x: self.x - 1,
                y: self.y - 1,
            },
            Point {
                x: self.x,
                y: self.y - 1,
            },
            Point {
                x: self.x + 1,
                y: self.y - 1,
            },
            Point {
                x: self.x - 1,
                y: self.y,
            },
            Point {
                x: self.x + 1,
                y: self.y,
            },
            Point {
                x: self.x - 1,
                y: self.y + 1,
            },
            Point {
                x: self.x,
                y: self.y + 1,
            },
            Point {
                x: self.x + 1,
                y: self.y + 1,
            },
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn center(&self) -> Point {
        Point {
            x: self.x + self.w / 2,
            y: self.y + self.h / 2,
        }
    }

    pub fn north_door(&self) -> Point {
        Point {
            x: self.x + (self.w - 1) / 2,
            y: self.y,
        }
    }

    pub fn south_door(&self) -> Point {
        Point {
            x: self.x + (self.w - 1) / 2,
            y: self.y + self.h - 1,
        }
    }

    pub fn west_door(&self) -> Point {
        Point {
            x: self.x,
            y: self.y + (self.h - 1) / 2,
        }
    }

    pub fn east_door(&self) -> Point {
        Point {
            x: self.x + self.w - 1,
            y: self.y + (self.h - 1) / 2,
        }
    }

    /// Do two rects overlap? (Exclusive — touching edges don't count.)
    pub fn overlaps(&self, other: &Rect) -> bool {
        self.x < other.x + other.w
            && other.x < self.x + self.w
            && self.y < other.y + other.h
            && other.y < self.y + self.h
    }

    /// Finds a shared x for a vertical corridor between two rects.
    /// Midpoint of the overlapping interior x-range (excluding corners).
    pub fn shared_x(&self, other: &Rect) -> Option<i32> {
        let lo = (self.x + 1).max(other.x + 1);
        let hi = (self.x + self.w - 2).min(other.x + other.w - 2);
        if lo <= hi { Some((lo + hi) / 2) } else { None }
    }

    /// Finds a shared y for a horizontal corridor between two rects.
    /// Midpoint of the overlapping interior y-range (excluding corners).
    pub fn shared_y(&self, other: &Rect) -> Option<i32> {
        let lo = (self.y + 1).max(other.y + 1);
        let hi = (self.y + self.h - 2).min(other.y + other.h - 2);
        if lo <= hi { Some((lo + hi) / 2) } else { None }
    }

    /// Is a point on the boundary (edge) of this rect?
    /// The point must be within the rect's extent and on one of its four edges.
    pub fn point_on_boundary(&self, p: &Point) -> bool {
        if p.x < self.x || p.x >= self.x + self.w || p.y < self.y || p.y >= self.y + self.h {
            return false;
        }
        p.x == self.x || p.x == self.x + self.w - 1 || p.y == self.y || p.y == self.y + self.h - 1
    }

    /// Returns a new rect inflated by `amount` on all sides.
    pub fn inflate(&self, amount: i32) -> Rect {
        Rect {
            x: self.x - amount,
            y: self.y - amount,
            w: self.w + 2 * amount,
            h: self.h + 2 * amount,
        }
    }

    /// Compute the minimum axis-aligned gap between two non-overlapping rects.
    /// Returns 0 if they overlap or touch.
    pub fn gap_to(&self, other: &Rect) -> i32 {
        let gap_x = if self.x + self.w <= other.x {
            other.x - (self.x + self.w)
        } else if other.x + other.w <= self.x {
            self.x - (other.x + other.w)
        } else {
            0
        };

        let gap_y = if self.y + self.h <= other.y {
            other.y - (self.y + self.h)
        } else if other.y + other.h <= self.y {
            self.y - (other.y + other.h)
        } else {
            0
        };

        if gap_x > 0 && gap_y > 0 {
            gap_x.min(gap_y)
        } else {
            gap_x.max(gap_y)
        }
    }

    /// Compute the x-axis gap between two rects.
    /// Returns 0 if they overlap or touch along the x axis.
    pub fn gap_x(&self, other: &Rect) -> i32 {
        if self.x + self.w <= other.x {
            other.x - (self.x + self.w)
        } else if other.x + other.w <= self.x {
            self.x - (other.x + other.w)
        } else {
            0
        }
    }

    /// Compute the y-axis gap between two rects.
    /// Returns 0 if they overlap or touch along the y axis.
    pub fn gap_y(&self, other: &Rect) -> i32 {
        if self.y + self.h <= other.y {
            other.y - (self.y + self.h)
        } else if other.y + other.h <= self.y {
            self.y - (other.y + other.h)
        } else {
            0
        }
    }

    /// Returns an iterator over all `Point`s in this rect, row by row (top to bottom, left to right).
    pub fn iter_points(&self) -> RectPointIter {
        RectPointIter {
            x: self.x,
            y: self.y,
            start_x: self.x,
            end_x: self.x + self.w,
            end_y: self.y + self.h,
        }
    }

    /// Does an axis-aligned segment pass through the strict interior of this rect?
    /// "Interior" excludes the boundary cells. Returns false for rects thinner
    /// than 3 in either dimension (no interior exists).
    pub fn segment_crosses_interior(&self, p0: &Point, p1: &Point) -> bool {
        if self.w <= 2 || self.h <= 2 {
            return false;
        }
        let interior = Rect {
            x: self.x + 1,
            y: self.y + 1,
            w: self.w - 2,
            h: self.h - 2,
        };

        if p0.x == p1.x {
            // Vertical segment.
            let x = p0.x;
            let y_min = p0.y.min(p1.y);
            let y_max = p0.y.max(p1.y);
            x >= interior.x
                && x < interior.x + interior.w
                && y_max >= interior.y
                && y_min < interior.y + interior.h
        } else if p0.y == p1.y {
            // Horizontal segment.
            let y = p0.y;
            let x_min = p0.x.min(p1.x);
            let x_max = p0.x.max(p1.x);
            y >= interior.y
                && y < interior.y + interior.h
                && x_max >= interior.x
                && x_min < interior.x + interior.w
        } else {
            // Diagonal — not expected in our grid-based system.
            false
        }
    }
}

/// Iterator over all [`Point`]s inside a [`Rect`], row by row.
pub struct RectPointIter {
    x: i32,
    y: i32,
    start_x: i32,
    end_x: i32,
    end_y: i32,
}

impl Iterator for RectPointIter {
    type Item = Point;

    fn next(&mut self) -> Option<Self::Item> {
        if self.y >= self.end_y {
            return None;
        }
        let p = Point {
            x: self.x,
            y: self.y,
        };
        self.x += 1;
        if self.x >= self.end_x {
            self.x = self.start_x;
            self.y += 1;
        }
        Some(p)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.y >= self.end_y {
            return (0, Some(0));
        }
        let remaining_in_row = (self.end_x - self.x) as usize;
        let remaining_full_rows = (self.end_y - self.y - 1) as usize;
        let row_width = (self.end_x - self.start_x) as usize;
        let total = remaining_in_row + remaining_full_rows * row_width;
        (total, Some(total))
    }
}

impl ExactSizeIterator for RectPointIter {}

/// The shape of a placed space.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Footprint {
    /// A simple axis-aligned rectangle.
    Rect(Rect),
    /// An arbitrary set of cells (for irregular shapes like caves).
    Cells(Vec<Point>),
    /// A composite of multiple sub-footprints.
    Composite(Vec<Footprint>),
}

impl Footprint {
    /// Returns the axis-aligned bounding rectangle of this footprint.
    pub fn bounding_rect(&self) -> Rect {
        match self {
            Footprint::Rect(r) => *r,
            Footprint::Cells(cells) => {
                let min_x = cells.iter().map(|p| p.x).min().unwrap_or(0);
                let max_x = cells.iter().map(|p| p.x).max().unwrap_or(0);
                let min_y = cells.iter().map(|p| p.y).min().unwrap_or(0);
                let max_y = cells.iter().map(|p| p.y).max().unwrap_or(0);
                Rect {
                    x: min_x,
                    y: min_y,
                    w: max_x - min_x + 1,
                    h: max_y - min_y + 1,
                }
            }
            Footprint::Composite(parts) => {
                if parts.is_empty() {
                    return Rect {
                        x: 0,
                        y: 0,
                        w: 0,
                        h: 0,
                    };
                }
                let first = parts[0].bounding_rect();
                let mut min_x = first.x;
                let mut min_y = first.y;
                let mut max_x = first.x + first.w - 1;
                let mut max_y = first.y + first.h - 1;
                for part in &parts[1..] {
                    let r = part.bounding_rect();
                    min_x = min_x.min(r.x);
                    min_y = min_y.min(r.y);
                    max_x = max_x.max(r.x + r.w - 1);
                    max_y = max_y.max(r.y + r.h - 1);
                }
                Rect {
                    x: min_x,
                    y: min_y,
                    w: max_x - min_x + 1,
                    h: max_y - min_y + 1,
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlacedSpace {
    pub space_id: SpaceId,
    pub rect: Rect,
    pub footprint: Footprint,
    pub style: RealizationStyle,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    Normal,
    Restricted,
    Optional,
}

#[derive(Debug, Clone)]
pub struct PlacedLink {
    pub kind: LinkKind,
    pub points: Vec<Point>,
}

#[derive(Debug, Clone)]
pub struct GeometryPlan {
    pub spaces: Vec<PlacedSpace>,
    pub links: Vec<PlacedLink>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- point_on_boundary ---

    #[test]
    fn point_on_boundary_corners() {
        let r = Rect {
            x: 5,
            y: 5,
            w: 10,
            h: 8,
        };
        assert!(r.point_on_boundary(&Point { x: 5, y: 5 }));
        assert!(r.point_on_boundary(&Point { x: 14, y: 5 }));
        assert!(r.point_on_boundary(&Point { x: 5, y: 12 }));
        assert!(r.point_on_boundary(&Point { x: 14, y: 12 }));
    }

    #[test]
    fn point_on_boundary_edges() {
        let r = Rect {
            x: 0,
            y: 0,
            w: 5,
            h: 5,
        };
        assert!(r.point_on_boundary(&Point { x: 2, y: 0 })); // north
        assert!(r.point_on_boundary(&Point { x: 2, y: 4 })); // south
        assert!(r.point_on_boundary(&Point { x: 0, y: 2 })); // west
        assert!(r.point_on_boundary(&Point { x: 4, y: 2 })); // east
    }

    #[test]
    fn point_on_boundary_interior_is_false() {
        let r = Rect {
            x: 5,
            y: 5,
            w: 10,
            h: 8,
        };
        assert!(!r.point_on_boundary(&Point { x: 8, y: 8 }));
    }

    #[test]
    fn point_on_boundary_outside_is_false() {
        let r = Rect {
            x: 5,
            y: 5,
            w: 10,
            h: 8,
        };
        assert!(!r.point_on_boundary(&Point { x: 4, y: 5 }));
        assert!(!r.point_on_boundary(&Point { x: 15, y: 5 }));
    }

    // --- inflate ---

    #[test]
    fn inflate_expands_all_sides() {
        let r = Rect {
            x: 10,
            y: 10,
            w: 5,
            h: 5,
        };
        let inflated = r.inflate(2);
        assert_eq!(
            inflated,
            Rect {
                x: 8,
                y: 8,
                w: 9,
                h: 9
            }
        );
    }

    #[test]
    fn inflate_zero_is_identity() {
        let r = Rect {
            x: 3,
            y: 4,
            w: 7,
            h: 8,
        };
        assert_eq!(r.inflate(0), r);
    }

    // --- gap_to ---

    #[test]
    fn gap_to_separated_horizontally() {
        let a = Rect {
            x: 0,
            y: 0,
            w: 5,
            h: 5,
        };
        let b = Rect {
            x: 8,
            y: 0,
            w: 5,
            h: 5,
        };
        assert_eq!(a.gap_to(&b), 3);
    }

    #[test]
    fn gap_to_separated_vertically() {
        let a = Rect {
            x: 0,
            y: 0,
            w: 5,
            h: 5,
        };
        let b = Rect {
            x: 0,
            y: 10,
            w: 5,
            h: 5,
        };
        assert_eq!(a.gap_to(&b), 5);
    }

    #[test]
    fn gap_to_touching_is_zero() {
        let a = Rect {
            x: 0,
            y: 0,
            w: 5,
            h: 5,
        };
        let b = Rect {
            x: 5,
            y: 0,
            w: 5,
            h: 5,
        };
        assert_eq!(a.gap_to(&b), 0);
    }

    #[test]
    fn gap_to_overlapping_is_zero() {
        let a = Rect {
            x: 0,
            y: 0,
            w: 5,
            h: 5,
        };
        let b = Rect {
            x: 3,
            y: 3,
            w: 5,
            h: 5,
        };
        assert_eq!(a.gap_to(&b), 0);
    }

    // --- segment_crosses_interior ---

    #[test]
    fn segment_crosses_interior_horizontal() {
        let r = Rect {
            x: 5,
            y: 5,
            w: 5,
            h: 5,
        };
        let p0 = Point { x: 0, y: 7 };
        let p1 = Point { x: 15, y: 7 };
        assert!(r.segment_crosses_interior(&p0, &p1));
    }

    #[test]
    fn segment_misses_interior() {
        let r = Rect {
            x: 5,
            y: 5,
            w: 5,
            h: 5,
        };
        let p0 = Point { x: 0, y: 2 };
        let p1 = Point { x: 15, y: 2 };
        assert!(!r.segment_crosses_interior(&p0, &p1));
    }

    #[test]
    fn segment_on_boundary_does_not_cross_interior() {
        let r = Rect {
            x: 5,
            y: 5,
            w: 5,
            h: 5,
        };
        // y=5 is the north boundary; interior starts at y=6.
        let p0 = Point { x: 0, y: 5 };
        let p1 = Point { x: 15, y: 5 };
        assert!(!r.segment_crosses_interior(&p0, &p1));
    }

    #[test]
    fn thin_rect_has_no_interior() {
        let r = Rect {
            x: 0,
            y: 0,
            w: 2,
            h: 5,
        };
        let p0 = Point { x: 1, y: 0 };
        let p1 = Point { x: 1, y: 10 };
        assert!(!r.segment_crosses_interior(&p0, &p1));
    }

    #[test]
    fn vertical_segment_crosses_interior() {
        let r = Rect {
            x: 5,
            y: 5,
            w: 5,
            h: 5,
        };
        let p0 = Point { x: 7, y: 0 };
        let p1 = Point { x: 7, y: 15 };
        assert!(r.segment_crosses_interior(&p0, &p1));
    }
}
