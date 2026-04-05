//! Fixed-point spatial types, angle helpers, and distance functions.
//!
//! All positions use micro-AU (`µAU`) for radii and milli-degrees (m°) for angles.
//! Integer math throughout for determinism.

use std::collections::HashMap;

use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::{AHashMap, AsteroidTemplateDef, BodyId, OrbitalBodyDef};

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

/// Radius in micro-AU. 1 AU = 1,000,000 units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RadiusAuMicro(pub u64);

/// Angle in milli-degrees. 360° = 360,000 units.
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

/// Sun-centered absolute cartesian position in micro-AU.
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
// AngleMilliDeg — ops trait + methods
// ---------------------------------------------------------------------------

impl std::ops::Add for AngleMilliDeg {
    type Output = Self;

    /// Wrapping addition mod 360,000.
    fn add(self, other: Self) -> Self {
        Self((self.0 + other.0) % FULL_CIRCLE)
    }
}

impl AngleMilliDeg {
    /// Smallest signed difference in [-180,000, +180,000].
    /// Positive means `to` is clockwise from `self`.
    #[allow(clippy::cast_possible_truncation)] // result is in [-180_000, 180_000]
    pub fn signed_delta(self, to: Self) -> i32 {
        let raw = i64::from(to.0) - i64::from(self.0);
        let full = i64::from(FULL_CIRCLE);
        let half = full / 2;
        if raw > half {
            (raw - full) as i32
        } else if raw < -half {
            (raw + full) as i32
        } else {
            raw as i32
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
    /// Squared distance between two absolute positions. Returns `u128` to avoid overflow.
    #[allow(clippy::cast_sign_loss)] // sum of squares is always non-negative
    pub fn distance_squared(self, other: Self) -> u128 {
        let dx = i128::from(self.x_au_um) - i128::from(other.x_au_um);
        let dy = i128::from(self.y_au_um) - i128::from(other.y_au_um);
        (dx * dx + dy * dy) as u128
    }

    /// Distance in micro-AU between two absolute positions.
    pub fn distance(self, other: Self) -> u64 {
        integer_sqrt(self.distance_squared(other))
    }
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

/// Convert polar coordinates (radius micro-AU, angle milli-degrees) to cartesian offset in
/// micro-AU.
#[allow(clippy::cast_possible_truncation)] // values are within i64 range after rounding
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
#[allow(clippy::cast_possible_truncation)] // sqrt of u128 always fits in u64
pub fn integer_sqrt(n: u128) -> u64 {
    if n <= 1 {
        return n as u64;
    }
    let shift = (128 - n.leading_zeros()).div_ceil(2);
    let mut x: u128 = 1u128 << shift;
    loop {
        let x1 = u128::midpoint(x, n / x);
        if x1 >= x {
            break;
        }
        x = x1;
    }
    x as u64
}

// ---------------------------------------------------------------------------
// Travel and co-location helpers
// ---------------------------------------------------------------------------

/// Compute travel time in ticks between two absolute positions.
pub fn travel_ticks(
    from: AbsolutePos,
    to: AbsolutePos,
    ticks_per_au: u64,
    min_transit: u64,
) -> u64 {
    let dist = from.distance(to);
    let ticks = dist * ticks_per_au / 1_000_000;
    ticks.max(min_transit)
}

/// Check if two entities are co-located (same parent body and within docking range).
pub fn is_co_located<S: std::hash::BuildHasher>(
    a: &Position,
    b: &Position,
    body_cache: &HashMap<BodyId, BodyCache, S>,
    docking_range: u64,
) -> bool {
    // Fast path: same parent body
    if a.parent_body != b.parent_body {
        // Different parents — compute absolute distance
        let abs_a = compute_entity_absolute(a, body_cache);
        let abs_b = compute_entity_absolute(b, body_cache);
        return abs_a.distance(abs_b) <= docking_range;
    }
    // Same parent — compute local distance directly
    let (ax, ay) = polar_to_cart(a.radius_au_um, a.angle_mdeg);
    let (bx, by) = polar_to_cart(b.radius_au_um, b.angle_mdeg);
    let dx = u128::from((ax - bx).unsigned_abs());
    let dy = u128::from((ay - by).unsigned_abs());
    let dist_sq = dx * dx + dy * dy;
    dist_sq <= u128::from(docking_range) * u128::from(docking_range)
}

/// Compute an entity's absolute position from its Position and the body cache.
pub fn compute_entity_absolute<S: std::hash::BuildHasher>(
    pos: &Position,
    body_cache: &HashMap<BodyId, BodyCache, S>,
) -> AbsolutePos {
    let parent = &body_cache[&pos.parent_body];
    let (dx, dy) = polar_to_cart(pos.radius_au_um, pos.angle_mdeg);
    AbsolutePos {
        x_au_um: parent.absolute.x_au_um + dx,
        y_au_um: parent.absolute.y_au_um + dy,
    }
}

// ---------------------------------------------------------------------------
// Body position cache
// ---------------------------------------------------------------------------

/// Cached absolute position for an orbital body, with epoch for invalidation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BodyCache {
    pub absolute: AbsolutePos,
    pub epoch: u32,
}

/// Cached absolute position for an entity, with the parent body's epoch at compute time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EntityCache {
    pub absolute: AbsolutePos,
    pub cached_parent_epoch: u32,
}

impl EntityCache {
    /// Recompute this entity's absolute position if the parent body's epoch has changed,
    /// or if the cache is empty (epoch 0).
    pub fn get_or_recompute<S: std::hash::BuildHasher>(
        &mut self,
        position: &Position,
        body_cache: &HashMap<BodyId, BodyCache, S>,
    ) -> AbsolutePos {
        let parent = &body_cache[&position.parent_body];
        if self.cached_parent_epoch != parent.epoch {
            let (dx, dy) = polar_to_cart(position.radius_au_um, position.angle_mdeg);
            self.absolute = AbsolutePos {
                x_au_um: parent.absolute.x_au_um + dx,
                y_au_um: parent.absolute.y_au_um + dy,
            };
            self.cached_parent_epoch = parent.epoch;
        }
        self.absolute
    }
}

/// Build the body cache by walking the body tree root→leaves.
///
/// Bodies with no parent (roots) get their position from `polar_to_cart(radius, angle)`.
/// Children accumulate: `child_abs = parent_abs + polar_to_cart(child.radius, child.angle)`.
/// All bodies start at epoch 1.
pub fn build_body_cache(bodies: &[OrbitalBodyDef]) -> AHashMap<BodyId, BodyCache> {
    let mut cache: AHashMap<BodyId, BodyCache> =
        HashMap::with_capacity_and_hasher(bodies.len(), ahash::RandomState::default());

    // Index bodies by id for parent lookup.
    let by_id: HashMap<&BodyId, &OrbitalBodyDef> = bodies.iter().map(|b| (&b.id, b)).collect();

    // Process roots first, then children. Repeat until all are placed.
    // This handles arbitrary tree depth without requiring sorted input.
    let mut remaining: Vec<&OrbitalBodyDef> = bodies.iter().collect();
    while !remaining.is_empty() {
        let before = remaining.len();
        remaining.retain(|body| {
            let parent_abs = match &body.parent {
                None => AbsolutePos::default(), // Root body (e.g., Sun at origin)
                Some(pid) => {
                    if let Some(pc) = cache.get(pid) {
                        pc.absolute
                    } else {
                        return true; // Parent not yet computed, retry next pass
                    }
                }
            };
            let (dx, dy) = polar_to_cart(
                RadiusAuMicro(body.radius_au_um),
                AngleMilliDeg(body.angle_mdeg),
            );
            cache.insert(
                body.id.clone(),
                BodyCache {
                    absolute: AbsolutePos {
                        x_au_um: parent_abs.x_au_um + dx,
                        y_au_um: parent_abs.y_au_um + dy,
                    },
                    epoch: 1,
                },
            );
            false // Placed successfully, remove from remaining
        });
        assert!(
            remaining.len() < before,
            "body tree has unreachable bodies (cycle or missing parent)"
        );
    }

    // Verify all bodies were placed.
    assert_eq!(
        cache.len(),
        by_id.len(),
        "not all bodies were placed in cache"
    );

    cache
}

// ---------------------------------------------------------------------------
// World-gen sampling helpers (integer math, no floats in RNG path)
// ---------------------------------------------------------------------------

/// Area-weighted radius sampling within an annular band.
/// `r = integer_sqrt(uniform(r_min², r_max²))` prevents inner-edge clustering.
pub fn random_radius_in_band(r_min: u64, r_max: u64, rng: &mut impl Rng) -> RadiusAuMicro {
    if r_min == r_max {
        return RadiusAuMicro(r_min);
    }
    let r_min_sq = u128::from(r_min) * u128::from(r_min);
    let r_max_sq = u128::from(r_max) * u128::from(r_max);
    let uniform = rng.gen_range(r_min_sq..=r_max_sq);
    RadiusAuMicro(integer_sqrt(uniform))
}

/// Wrap-safe angle generation within a zone span.
/// `(start + uniform(0..span)) % 360_000` handles zones that wrap past 360°.
pub fn random_angle_in_span(start: u32, span: u32, rng: &mut impl Rng) -> AngleMilliDeg {
    AngleMilliDeg((start + rng.gen_range(0..span)) % FULL_CIRCLE)
}

/// Generate a random Position within a zone body's defined zone.
///
/// Panics if the body has no zone.
pub fn random_position_in_zone(body: &OrbitalBodyDef, rng: &mut impl Rng) -> Position {
    let zone = body
        .zone
        .as_ref()
        .expect("random_position_in_zone requires a body with a zone");
    Position {
        parent_body: body.id.clone(),
        radius_au_um: random_radius_in_band(zone.radius_min_au_um, zone.radius_max_au_um, rng),
        angle_mdeg: random_angle_in_span(zone.angle_start_mdeg, zone.angle_span_mdeg, rng),
    }
}

/// Weighted random selection from zone bodies using `scan_site_weight`.
/// Integer accumulation — no floats in the RNG path.
///
/// Panics if `zone_bodies` is empty.
pub fn pick_zone_weighted<'a>(
    zone_bodies: &[&'a OrbitalBodyDef],
    rng: &mut impl Rng,
) -> &'a OrbitalBodyDef {
    let total_weight: u32 = zone_bodies
        .iter()
        .map(|b| {
            b.zone
                .as_ref()
                .expect("zone body must have zone")
                .scan_site_weight
        })
        .sum();
    let roll = rng.gen_range(0..total_weight);
    let mut acc = 0u32;
    for body in zone_bodies {
        acc += body
            .zone
            .as_ref()
            .expect("zone body must have zone")
            .scan_site_weight;
        if roll < acc {
            return body;
        }
    }
    zone_bodies.last().expect("zone_bodies must not be empty")
}

/// Biased template selection based on zone resource class.
/// Weight: match=3, none=2, mismatch=1. Integer math throughout.
///
/// Panics if `templates` is empty.
pub fn pick_template_biased<'a>(
    templates: &'a [AsteroidTemplateDef],
    zone_class: ResourceClass,
    rng: &mut impl Rng,
) -> &'a AsteroidTemplateDef {
    let total_weight: u32 = templates
        .iter()
        .map(|t| match &t.preferred_class {
            Some(pc) if *pc == zone_class => 3,
            None => 2,
            _ => 1,
        })
        .sum();
    let roll = rng.gen_range(0..total_weight);
    let mut acc = 0u32;
    for template in templates {
        acc += match &template.preferred_class {
            Some(pc) if *pc == zone_class => 3,
            None => 2,
            _ => 1,
        };
        if roll < acc {
            return template;
        }
    }
    templates.last().expect("templates must not be empty")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    // -- AngleMilliDeg --

    #[test]
    fn angle_add_no_wrap() {
        let result = AngleMilliDeg(90_000) + AngleMilliDeg(45_000);
        assert_eq!(result, AngleMilliDeg(135_000));
    }

    #[test]
    fn angle_add_wraps_at_boundary() {
        let result = AngleMilliDeg(350_000) + AngleMilliDeg(20_000);
        assert_eq!(result, AngleMilliDeg(10_000));
    }

    #[test]
    fn angle_add_exact_full_circle() {
        let result = AngleMilliDeg(180_000) + AngleMilliDeg(180_000);
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

    // -- Body cache --

    fn bid(id: &str) -> BodyId {
        BodyId(id.to_string())
    }

    fn body(id: &str, parent: Option<&str>, radius: u64, angle: u32) -> OrbitalBodyDef {
        OrbitalBodyDef {
            id: BodyId(id.to_string()),
            parent: parent.map(|p| BodyId(p.to_string())),
            name: id.to_string(),
            body_type: crate::BodyType::Planet,
            radius_au_um: radius,
            angle_mdeg: angle,
            solar_intensity: 1.0,
            zone: None,
        }
    }

    #[test]
    fn body_cache_sun_at_origin() {
        let bodies = vec![body("sun", None, 0, 0)];
        let cache = build_body_cache(&bodies);
        assert_eq!(cache[&bid("sun")].absolute, AbsolutePos::default());
        assert_eq!(cache[&bid("sun")].epoch, 1);
    }

    #[test]
    fn body_cache_earth_position() {
        let bodies = vec![
            body("sun", None, 0, 0),
            body("earth", Some("sun"), 1_000_000, 0), // 1 AU at 0°
        ];
        let cache = build_body_cache(&bodies);
        assert_eq!(
            cache[&bid("earth")].absolute,
            AbsolutePos {
                x_au_um: 1_000_000,
                y_au_um: 0
            }
        );
    }

    #[test]
    fn body_cache_multi_level_sun_earth_moon() {
        let bodies = vec![
            body("sun", None, 0, 0),
            body("earth", Some("sun"), 1_000_000, 0),
            body("luna", Some("earth"), 2_570, 90_000), // 90° from earth
        ];
        let cache = build_body_cache(&bodies);
        // Luna should be at Earth's position + polar_to_cart(2570, 90°)
        // polar_to_cart(2570, 90°) = (0, 2570)
        assert_eq!(
            cache[&bid("luna")].absolute,
            AbsolutePos {
                x_au_um: 1_000_000,
                y_au_um: 2_570
            }
        );
    }

    #[test]
    fn body_cache_handles_unsorted_input() {
        // Children before parents — build_body_cache should handle this
        let bodies = vec![
            body("luna", Some("earth"), 2_570, 90_000),
            body("earth", Some("sun"), 1_000_000, 0),
            body("sun", None, 0, 0),
        ];
        let cache = build_body_cache(&bodies);
        assert_eq!(cache.len(), 3);
        assert_eq!(
            cache[&bid("luna")].absolute,
            AbsolutePos {
                x_au_um: 1_000_000,
                y_au_um: 2_570
            }
        );
    }

    #[test]
    fn body_cache_all_bodies_get_epoch_1() {
        let bodies = vec![
            body("sun", None, 0, 0),
            body("earth", Some("sun"), 1_000_000, 0),
            body("mars", Some("sun"), 1_524_000, 135_000),
        ];
        let cache = build_body_cache(&bodies);
        for bc in cache.values() {
            assert_eq!(bc.epoch, 1);
        }
    }

    // -- Entity cache --

    #[test]
    fn entity_cache_computes_on_first_access() {
        let bodies = vec![
            body("sun", None, 0, 0),
            body("earth", Some("sun"), 1_000_000, 0),
        ];
        let body_cache = build_body_cache(&bodies);
        let pos = Position {
            parent_body: BodyId("earth".to_string()),
            radius_au_um: RadiusAuMicro(5_000),
            angle_mdeg: AngleMilliDeg(0),
        };
        let mut ec = EntityCache::default();
        let abs = ec.get_or_recompute(&pos, &body_cache);
        assert_eq!(
            abs,
            AbsolutePos {
                x_au_um: 1_005_000,
                y_au_um: 0
            }
        );
        assert_eq!(ec.cached_parent_epoch, 1);
    }

    #[test]
    fn entity_cache_returns_cached_on_same_epoch() {
        let bodies = vec![
            body("sun", None, 0, 0),
            body("earth", Some("sun"), 1_000_000, 0),
        ];
        let body_cache = build_body_cache(&bodies);
        let pos = Position {
            parent_body: BodyId("earth".to_string()),
            radius_au_um: RadiusAuMicro(5_000),
            angle_mdeg: AngleMilliDeg(0),
        };
        let mut ec = EntityCache::default();
        let abs1 = ec.get_or_recompute(&pos, &body_cache);
        let abs2 = ec.get_or_recompute(&pos, &body_cache);
        assert_eq!(abs1, abs2);
    }

    #[test]
    fn entity_cache_recomputes_on_epoch_change() {
        let bodies = vec![
            body("sun", None, 0, 0),
            body("earth", Some("sun"), 1_000_000, 0),
        ];
        let mut body_cache = build_body_cache(&bodies);
        let pos = Position {
            parent_body: BodyId("earth".to_string()),
            radius_au_um: RadiusAuMicro(5_000),
            angle_mdeg: AngleMilliDeg(0),
        };
        let mut ec = EntityCache::default();
        ec.get_or_recompute(&pos, &body_cache);
        assert_eq!(ec.cached_parent_epoch, 1);

        // Simulate body movement: shift earth and bump epoch
        let earth = body_cache.get_mut(&bid("earth")).expect("earth");
        earth.absolute.x_au_um = 2_000_000;
        earth.epoch = 2;

        let abs = ec.get_or_recompute(&pos, &body_cache);
        assert_eq!(
            abs,
            AbsolutePos {
                x_au_um: 2_005_000,
                y_au_um: 0
            }
        );
        assert_eq!(ec.cached_parent_epoch, 2);
    }

    // -- random_radius_in_band --

    #[test]
    fn radius_band_equal_min_max() {
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
        let r = random_radius_in_band(500, 500, &mut rng);
        assert_eq!(r, RadiusAuMicro(500));
    }

    #[test]
    fn radius_band_stays_within_bounds() {
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
        for _ in 0..1000 {
            let r = random_radius_in_band(1000, 2000, &mut rng);
            assert!(r.0 >= 1000 && r.0 <= 2000, "radius {} out of bounds", r.0);
        }
    }

    #[test]
    fn radius_band_area_weighted_not_inner_clustered() {
        // With area weighting, the median radius should be closer to r_max
        // than a uniform distribution would give. For r_min=0, r_max=1000,
        // area-weighted median is ~707 (sqrt(0.5) * 1000).
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
        let mut samples = Vec::new();
        for _ in 0..10_000 {
            samples.push(random_radius_in_band(0, 1000, &mut rng).0);
        }
        samples.sort_unstable();
        let median = samples[5000];
        // Area-weighted median should be around 707, not 500 (uniform)
        assert!(
            median > 600 && median < 800,
            "median {median} suggests non-area-weighted sampling"
        );
    }

    // -- random_angle_in_span --

    #[test]
    fn angle_in_span_stays_within_bounds() {
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
        for _ in 0..1000 {
            let angle = random_angle_in_span(0, 90_000, &mut rng);
            assert!(angle.0 < 90_000, "angle {} out of span", angle.0);
        }
    }

    #[test]
    fn angle_in_span_wraps_around() {
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
        let mut saw_wrapped = false;
        for _ in 0..1000 {
            let angle = random_angle_in_span(350_000, 40_000, &mut rng);
            // Valid: 350_000..360_000 or 0..30_000 (wrapped)
            assert!(
                angle.0 < FULL_CIRCLE,
                "angle {} exceeds full circle",
                angle.0
            );
            if angle.0 < 30_000 {
                saw_wrapped = true;
            }
        }
        assert!(saw_wrapped, "wrap-around angles should be generated");
    }

    #[test]
    fn angle_in_span_full_circle() {
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
        for _ in 0..100 {
            let angle = random_angle_in_span(0, FULL_CIRCLE, &mut rng);
            assert!(
                angle.0 < FULL_CIRCLE,
                "angle {} exceeds full circle",
                angle.0
            );
        }
    }

    // -- random_position_in_zone --

    #[test]
    fn position_in_zone_uses_body_id() {
        let body = OrbitalBodyDef {
            id: BodyId("test_belt".to_string()),
            name: "Test Belt".to_string(),
            parent: None,
            body_type: crate::BodyType::Belt,
            radius_au_um: 0,
            angle_mdeg: 0,
            solar_intensity: 1.0,
            zone: Some(crate::ZoneDef {
                radius_min_au_um: 1000,
                radius_max_au_um: 2000,
                angle_start_mdeg: 0,
                angle_span_mdeg: FULL_CIRCLE,
                resource_class: ResourceClass::MetalRich,
                scan_site_weight: 1,
                implicit_comm_tier: None,
            }),
        };
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
        let pos = random_position_in_zone(&body, &mut rng);
        assert_eq!(pos.parent_body, BodyId("test_belt".to_string()));
        assert!(pos.radius_au_um.0 >= 1000 && pos.radius_au_um.0 <= 2000);
    }

    // -- pick_zone_weighted --

    #[test]
    fn zone_weighted_respects_weights() {
        let heavy = OrbitalBodyDef {
            id: BodyId("heavy".to_string()),
            name: "Heavy".to_string(),
            parent: None,
            body_type: crate::BodyType::Belt,
            radius_au_um: 0,
            angle_mdeg: 0,
            solar_intensity: 1.0,
            zone: Some(crate::ZoneDef {
                radius_min_au_um: 100,
                radius_max_au_um: 200,
                angle_start_mdeg: 0,
                angle_span_mdeg: FULL_CIRCLE,
                resource_class: ResourceClass::MetalRich,
                scan_site_weight: 9,
                implicit_comm_tier: None,
            }),
        };
        let light = OrbitalBodyDef {
            id: BodyId("light".to_string()),
            name: "Light".to_string(),
            parent: None,
            body_type: crate::BodyType::Zone,
            radius_au_um: 0,
            angle_mdeg: 0,
            solar_intensity: 1.0,
            zone: Some(crate::ZoneDef {
                radius_min_au_um: 100,
                radius_max_au_um: 200,
                angle_start_mdeg: 0,
                angle_span_mdeg: FULL_CIRCLE,
                resource_class: ResourceClass::Mixed,
                scan_site_weight: 1,
                implicit_comm_tier: None,
            }),
        };
        let zone_bodies: Vec<&OrbitalBodyDef> = vec![&heavy, &light];
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
        let mut heavy_count = 0u32;
        for _ in 0..1000 {
            let picked = pick_zone_weighted(&zone_bodies, &mut rng);
            if picked.id == BodyId("heavy".to_string()) {
                heavy_count += 1;
            }
        }
        // Weight 9:1, expect ~900 heavy picks. Allow ±50 for randomness.
        assert!(
            heavy_count > 850 && heavy_count < 950,
            "heavy_count {heavy_count} outside expected range for 9:1 weighting"
        );
    }

    // -- pick_template_biased --

    #[test]
    fn template_bias_favors_matching_class() {
        let metal = AsteroidTemplateDef {
            id: "metal".to_string(),
            anomaly_tags: vec![],
            composition_ranges: HashMap::new(),
            preferred_class: Some(ResourceClass::MetalRich),
        };
        let generic = AsteroidTemplateDef {
            id: "generic".to_string(),
            anomaly_tags: vec![],
            composition_ranges: HashMap::new(),
            preferred_class: None,
        };
        let volatile = AsteroidTemplateDef {
            id: "volatile".to_string(),
            anomaly_tags: vec![],
            composition_ranges: HashMap::new(),
            preferred_class: Some(ResourceClass::VolatileRich),
        };
        let templates = vec![metal, generic, volatile];
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);
        let mut counts = HashMap::new();
        for _ in 0..6000 {
            let t = pick_template_biased(&templates, ResourceClass::MetalRich, &mut rng);
            *counts.entry(t.id.clone()).or_insert(0u32) += 1;
        }
        // Weights: metal=3, generic=2, volatile=1, total=6
        // Expected: metal=3000, generic=2000, volatile=1000
        let metal_count = counts.get("metal").copied().unwrap_or(0);
        let generic_count = counts.get("generic").copied().unwrap_or(0);
        let volatile_count = counts.get("volatile").copied().unwrap_or(0);
        assert!(
            metal_count > generic_count && generic_count > volatile_count,
            "expected metal({metal_count}) > generic({generic_count}) > volatile({volatile_count})"
        );
    }

    // -- Determinism --

    #[test]
    fn sampling_is_deterministic() {
        let body = OrbitalBodyDef {
            id: BodyId("belt".to_string()),
            name: "Belt".to_string(),
            parent: None,
            body_type: crate::BodyType::Belt,
            radius_au_um: 0,
            angle_mdeg: 0,
            solar_intensity: 1.0,
            zone: Some(crate::ZoneDef {
                radius_min_au_um: 1000,
                radius_max_au_um: 5000,
                angle_start_mdeg: 45_000,
                angle_span_mdeg: 90_000,
                resource_class: ResourceClass::MetalRich,
                scan_site_weight: 1,
                implicit_comm_tier: None,
            }),
        };
        let mut rng1 = rand_chacha::ChaCha8Rng::seed_from_u64(99);
        let mut rng2 = rand_chacha::ChaCha8Rng::seed_from_u64(99);
        for _ in 0..100 {
            let p1 = random_position_in_zone(&body, &mut rng1);
            let p2 = random_position_in_zone(&body, &mut rng2);
            assert_eq!(p1, p2);
        }
    }
}
