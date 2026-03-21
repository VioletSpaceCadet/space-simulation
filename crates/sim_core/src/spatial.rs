//! Fixed-point spatial types, angle helpers, and distance functions.
//!
//! All positions use micro-AU (µAU) for radii and milli-degrees (m°) for angles.
//! Integer math throughout for determinism.

use serde::{Deserialize, Serialize};

use crate::BodyId;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Meters in one astronomical unit.
pub const METERS_PER_AU: u64 = 149_597_870_700;

/// Meters in one micro-AU.
pub const METERS_PER_MICRO_AU: f64 = 149_597.870_7;

/// Full circle in milli-degrees (360° = 360,000 m°).
pub const FULL_CIRCLE: u32 = 360_000;

// ---------------------------------------------------------------------------
// Newtypes
// ---------------------------------------------------------------------------

/// Radius in micro-AU (µAU). 1 AU = 1,000,000 µAU.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RadiusAuMicro(pub u64);

/// Angle in milli-degrees. 360° = 360,000 m°.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AngleMilliDeg(pub u32);

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Resource class for zones and asteroid templates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceClass {
    MetalRich,
    Mixed,
    VolatileRich,
}

// ---------------------------------------------------------------------------
// Structs
// ---------------------------------------------------------------------------

/// Sun-centered absolute cartesian position in µAU.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct AbsolutePos {
    pub x_au_um: i64,
    pub y_au_um: i64,
}

/// Hierarchical polar position relative to a parent orbital body.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Position {
    pub parent_body: BodyId,
    pub radius_au_um: RadiusAuMicro,
    pub angle_mdeg: AngleMilliDeg,
}

// ---------------------------------------------------------------------------
// AngleMilliDeg methods
// ---------------------------------------------------------------------------

impl AngleMilliDeg {
    /// Wrapping addition mod 360,000.
    pub fn add(self, other: Self) -> Self {
        Self((self.0 + other.0) % FULL_CIRCLE)
    }

    /// Smallest signed difference in [-180,000, +180,000].
    /// Positive means `to` is clockwise from `self`.
    pub fn signed_delta(self, to: Self) -> i32 {
        let raw = to.0 as i32 - self.0 as i32;
        let half = FULL_CIRCLE as i32 / 2;
        if raw > half {
            raw - FULL_CIRCLE as i32
        } else if raw < -half {
            raw + FULL_CIRCLE as i32
        } else {
            raw
        }
    }

    /// Check if this angle falls within a span starting at `start` with width `span` m°.
    /// Handles wrap-around (e.g., start=350°, span=40° contains 10°).
    pub fn within_span(self, start: Self, span: u32) -> bool {
        let offset = (self.0 + FULL_CIRCLE - start.0) % FULL_CIRCLE;
        offset < span
    }
}

// ---------------------------------------------------------------------------
// AbsolutePos methods
// ---------------------------------------------------------------------------

impl AbsolutePos {
    /// Squared distance between two absolute positions. Returns u128 to avoid overflow.
    pub fn distance_squared(self, other: Self) -> u128 {
        let dx = i128::from(self.x_au_um) - i128::from(other.x_au_um);
        let dy = i128::from(self.y_au_um) - i128::from(other.y_au_um);
        (dx * dx + dy * dy) as u128
    }

    /// Distance in µAU between two absolute positions.
    pub fn distance(self, other: Self) -> u64 {
        integer_sqrt(self.distance_squared(other))
    }
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

/// Convert polar coordinates (radius µAU, angle m°) to cartesian offset (x, y) in µAU.
pub fn polar_to_cart(radius: RadiusAuMicro, angle: AngleMilliDeg) -> (i64, i64) {
    let rad = f64::from(angle.0) * std::f64::consts::PI / 180_000.0;
    let x = (radius.0 as f64 * rad.cos()).round() as i64;
    let y = (radius.0 as f64 * rad.sin()).round() as i64;
    (x, y)
}

// ---------------------------------------------------------------------------
// Integer sqrt
// ---------------------------------------------------------------------------

/// Deterministic integer square root via Newton's method. Returns `floor(sqrt(n))`.
pub fn integer_sqrt(n: u128) -> u64 {
    if n <= 1 {
        return n as u64;
    }
    // Initial estimate: 2^(ceil(bits/2))
    let shift = (128 - n.leading_zeros() + 1) / 2;
    let mut x: u128 = 1u128 << shift;
    loop {
        let x1 = (x + n / x) / 2;
        if x1 >= x {
            break;
        }
        x = x1;
    }
    x as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- AngleMilliDeg --

    #[test]
    fn angle_add_no_wrap() {
        let result = AngleMilliDeg(90_000).add(AngleMilliDeg(45_000));
        assert_eq!(result, AngleMilliDeg(135_000));
    }

    #[test]
    fn angle_add_wraps_at_boundary() {
        let result = AngleMilliDeg(350_000).add(AngleMilliDeg(20_000));
        assert_eq!(result, AngleMilliDeg(10_000));
    }

    #[test]
    fn angle_add_exact_full_circle() {
        let result = AngleMilliDeg(180_000).add(AngleMilliDeg(180_000));
        assert_eq!(result, AngleMilliDeg(0));
    }

    #[test]
    fn signed_delta_clockwise() {
        assert_eq!(
            AngleMilliDeg(10_000).signed_delta(AngleMilliDeg(50_000)),
            40_000
        );
    }

    #[test]
    fn signed_delta_counter_clockwise() {
        assert_eq!(
            AngleMilliDeg(50_000).signed_delta(AngleMilliDeg(10_000)),
            -40_000
        );
    }

    #[test]
    fn signed_delta_wraps_short_way_positive() {
        // 350° to 10° should be +20°, not -340°
        assert_eq!(
            AngleMilliDeg(350_000).signed_delta(AngleMilliDeg(10_000)),
            20_000
        );
    }

    #[test]
    fn signed_delta_wraps_short_way_negative() {
        // 10° to 350° should be -20°, not +340°
        assert_eq!(
            AngleMilliDeg(10_000).signed_delta(AngleMilliDeg(350_000)),
            -20_000
        );
    }

    #[test]
    fn signed_delta_same_angle() {
        assert_eq!(AngleMilliDeg(90_000).signed_delta(AngleMilliDeg(90_000)), 0);
    }

    #[test]
    fn signed_delta_opposite() {
        // 0° to 180° — ambiguous, convention: positive
        assert_eq!(
            AngleMilliDeg(0).signed_delta(AngleMilliDeg(180_000)),
            180_000
        );
    }

    #[test]
    fn within_span_simple_inside() {
        assert!(AngleMilliDeg(45_000).within_span(AngleMilliDeg(30_000), 30_000));
    }

    #[test]
    fn within_span_simple_outside() {
        assert!(!AngleMilliDeg(70_000).within_span(AngleMilliDeg(30_000), 30_000));
    }

    #[test]
    fn within_span_wrap_around_inside() {
        // start=350°, span=40° covers 350°–30°; 10° is inside
        assert!(AngleMilliDeg(10_000).within_span(AngleMilliDeg(350_000), 40_000));
    }

    #[test]
    fn within_span_wrap_around_outside() {
        // start=350°, span=40° covers 350°–30°; 40° is outside
        assert!(!AngleMilliDeg(40_000).within_span(AngleMilliDeg(350_000), 40_000));
    }

    #[test]
    fn within_span_at_start_included() {
        assert!(AngleMilliDeg(30_000).within_span(AngleMilliDeg(30_000), 10_000));
    }

    #[test]
    fn within_span_at_end_excluded() {
        // offset = 10_000, not < 10_000
        assert!(!AngleMilliDeg(40_000).within_span(AngleMilliDeg(30_000), 10_000));
    }

    // -- Distance --

    #[test]
    fn distance_squared_same_point() {
        let a = AbsolutePos {
            x_au_um: 100,
            y_au_um: 200,
        };
        assert_eq!(a.distance_squared(a), 0);
    }

    #[test]
    fn distance_squared_3_4_5() {
        let a = AbsolutePos {
            x_au_um: 0,
            y_au_um: 0,
        };
        let b = AbsolutePos {
            x_au_um: 3,
            y_au_um: 4,
        };
        assert_eq!(a.distance_squared(b), 25);
    }

    #[test]
    fn distance_3_4_5_triangle() {
        let a = AbsolutePos {
            x_au_um: 0,
            y_au_um: 0,
        };
        let b = AbsolutePos {
            x_au_um: 3,
            y_au_um: 4,
        };
        assert_eq!(a.distance(b), 5);
    }

    #[test]
    fn distance_1_au_apart() {
        let a = AbsolutePos {
            x_au_um: 0,
            y_au_um: 0,
        };
        let b = AbsolutePos {
            x_au_um: 1_000_000,
            y_au_um: 0,
        };
        assert_eq!(a.distance(b), 1_000_000);
    }

    #[test]
    fn distance_negative_coords() {
        let a = AbsolutePos {
            x_au_um: -3,
            y_au_um: 0,
        };
        let b = AbsolutePos {
            x_au_um: 0,
            y_au_um: 4,
        };
        assert_eq!(a.distance(b), 5);
    }

    #[test]
    fn distance_squared_agrees_with_distance() {
        let a = AbsolutePos {
            x_au_um: 1_000_000,
            y_au_um: 0,
        };
        let b = AbsolutePos {
            x_au_um: 0,
            y_au_um: 1_000_000,
        };
        let dist = a.distance(b);
        let dist_sq = a.distance_squared(b);
        // floor(sqrt(n))² <= n < (floor(sqrt(n))+1)²
        assert!(dist as u128 * dist as u128 <= dist_sq);
        assert!((dist as u128 + 1) * (dist as u128 + 1) > dist_sq);
    }

    // -- integer_sqrt --

    #[test]
    fn sqrt_zero() {
        assert_eq!(integer_sqrt(0), 0);
    }

    #[test]
    fn sqrt_one() {
        assert_eq!(integer_sqrt(1), 1);
    }

    #[test]
    fn sqrt_perfect_squares() {
        assert_eq!(integer_sqrt(4), 2);
        assert_eq!(integer_sqrt(9), 3);
        assert_eq!(integer_sqrt(16), 4);
        assert_eq!(integer_sqrt(25), 5);
        assert_eq!(integer_sqrt(100), 10);
        assert_eq!(integer_sqrt(10_000), 100);
    }

    #[test]
    fn sqrt_non_perfect_floors() {
        assert_eq!(integer_sqrt(2), 1);
        assert_eq!(integer_sqrt(3), 1);
        assert_eq!(integer_sqrt(5), 2);
        assert_eq!(integer_sqrt(8), 2);
        assert_eq!(integer_sqrt(10), 3);
        assert_eq!(integer_sqrt(99), 9);
    }

    #[test]
    fn sqrt_large_value() {
        assert_eq!(integer_sqrt(1_000_000_000_000), 1_000_000);
    }

    #[test]
    fn sqrt_u128_large() {
        assert_eq!(integer_sqrt(1_000_000_000_000_000_000), 1_000_000_000);
    }

    // -- polar_to_cart --

    #[test]
    fn polar_to_cart_zero_degrees() {
        let (x, y) = polar_to_cart(RadiusAuMicro(1_000_000), AngleMilliDeg(0));
        assert_eq!(x, 1_000_000);
        assert_eq!(y, 0);
    }

    #[test]
    fn polar_to_cart_90_degrees() {
        let (x, y) = polar_to_cart(RadiusAuMicro(1_000_000), AngleMilliDeg(90_000));
        assert_eq!(x, 0);
        assert_eq!(y, 1_000_000);
    }

    #[test]
    fn polar_to_cart_180_degrees() {
        let (x, y) = polar_to_cart(RadiusAuMicro(1_000_000), AngleMilliDeg(180_000));
        assert_eq!(x, -1_000_000);
        assert_eq!(y, 0);
    }

    #[test]
    fn polar_to_cart_270_degrees() {
        let (x, y) = polar_to_cart(RadiusAuMicro(1_000_000), AngleMilliDeg(270_000));
        assert_eq!(x, 0);
        assert_eq!(y, -1_000_000);
    }

    #[test]
    fn polar_to_cart_zero_radius() {
        let (x, y) = polar_to_cart(RadiusAuMicro(0), AngleMilliDeg(45_000));
        assert_eq!(x, 0);
        assert_eq!(y, 0);
    }

    // -- Serialization --

    #[test]
    fn position_serialization_roundtrip() {
        let pos = Position {
            parent_body: BodyId("earth".to_string()),
            radius_au_um: RadiusAuMicro(1_000_000),
            angle_mdeg: AngleMilliDeg(90_000),
        };
        let json = serde_json::to_string(&pos).expect("serialize");
        let restored: Position = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(pos, restored);
    }

    #[test]
    fn absolute_pos_serialization_roundtrip() {
        let pos = AbsolutePos {
            x_au_um: -500_000,
            y_au_um: 300_000,
        };
        let json = serde_json::to_string(&pos).expect("serialize");
        let restored: AbsolutePos = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(pos, restored);
    }
}
