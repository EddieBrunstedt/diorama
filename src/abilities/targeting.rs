//! Where abilities hit, as plain data: which enemy the sniper shot snaps to, and where a
//! barrage's shells fall. Positions are on the ground plane (world x and z), in cells.

use crate::rng::Rng;
use bevy::prelude::*;

/// An enemy the sniper could snap to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mark<Id> {
    pub id: Id,
    pub at: Vec2,
    /// How big it is: the cursor snaps to its edge.
    pub radius: f32,
    /// HP left.
    pub health: f32,
}

/// The enemy the sniper snaps to: the toughest one (the most HP left) whose edge is within
/// `snap` of the cursor, so brutes come first. Ties go to the one nearest the cursor.
pub fn sniper_target<Id: Copy>(
    cursor: Vec2,
    snap: f32,
    marks: impl IntoIterator<Item = Mark<Id>>,
) -> Option<Mark<Id>> {
    let gap = |mark: &Mark<Id>| (mark.at.distance(cursor) - mark.radius).max(0.0);
    marks
        .into_iter()
        .filter(|mark| mark.health > 0.0 && gap(mark) <= snap)
        .max_by(|a, b| {
            a.health
                .total_cmp(&b.health)
                .then_with(|| gap(b).total_cmp(&gap(a)))
        })
}

/// Where a barrage's `count` shells land around `center`: spread evenly over a circle of
/// `radius` (a sunflower spiral, turned at random), each nudged by up to `jitter` but never
/// outside the circle, in a random order.
pub fn barrage_pattern(
    center: Vec2,
    radius: f32,
    count: usize,
    jitter: f32,
    rng: &mut Rng,
) -> Vec<Vec2> {
    let golden_angle = std::f32::consts::PI * (3.0 - 5f32.sqrt());
    let turn = rng.range(0.0, std::f32::consts::TAU);
    let mut spots: Vec<Vec2> = (0..count)
        .map(|i| {
            let out = radius * ((i as f32 + 0.5) / count as f32).sqrt();
            let spot = Vec2::from_angle(turn + i as f32 * golden_angle) * out;
            let nudge =
                Vec2::from_angle(rng.range(0.0, std::f32::consts::TAU)) * rng.range(0.0, jitter);
            center + (spot + nudge).clamp_length_max(radius)
        })
        .collect();
    // Shuffle, so the shells don't land in a spiral.
    for i in (1..spots.len()).rev() {
        spots.swap(i, rng.index(i + 1));
    }
    spots
}

/// When each of a barrage's `count` shells lands, in seconds after the click: the first at
/// `first`, the rest evenly over the `spread` after it.
pub fn landing_times(count: usize, first: f32, spread: f32) -> Vec<f32> {
    (0..count)
        .map(|i| first + spread * i as f32 / (count.max(2) - 1) as f32)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mark(id: u32, x: f32, radius: f32, health: f32) -> Mark<u32> {
        Mark {
            id,
            at: Vec2::new(x, 0.0),
            radius,
            health,
        }
    }

    #[test]
    fn the_sniper_snaps_to_the_toughest_near_the_cursor() {
        let marks = [
            mark(0, 0.2, 0.25, 10.0),
            mark(1, 1.0, 0.25, 10.0),
            mark(2, 2.0, 0.75, 300.0),
            mark(3, 9.0, 0.75, 900.0),
        ];
        // The brute's edge is 1.25 away: in reach, and tougher than the swarmers.
        assert_eq!(sniper_target(Vec2::ZERO, 1.5, marks).map(|m| m.id), Some(2));
        // Out of reach of the brute, it takes the nearest of the equally tough swarmers.
        assert_eq!(sniper_target(Vec2::ZERO, 1.0, marks).map(|m| m.id), Some(0));
        assert_eq!(sniper_target(Vec2::new(-5.0, 0.0), 1.5, marks), None);
        // The dead don't count.
        let dead = [mark(0, 0.0, 0.25, 0.0)];
        assert_eq!(sniper_target(Vec2::ZERO, 1.5, dead), None);
    }

    #[test]
    fn barrage_shells_spread_over_the_circle() {
        let mut rng = Rng::new(7);
        let center = Vec2::new(4.0, -3.0);
        let spots = barrage_pattern(center, 3.0, 12, 0.3, &mut rng);
        assert_eq!(spots.len(), 12);
        assert!(spots.iter().all(|spot| spot.distance(center) <= 3.0 + 1e-4));
        // Spread out: no two land on top of each other, and some land far out.
        for (i, a) in spots.iter().enumerate() {
            for b in &spots[i + 1..] {
                assert!(a.distance(*b) > 0.4, "{a} {b}");
            }
        }
        assert!(spots.iter().any(|spot| spot.distance(center) > 2.3));
        assert!(spots.iter().any(|spot| spot.distance(center) < 1.0));
        // Each quarter of the circle gets some.
        for quarter in 0..4 {
            let count = spots
                .iter()
                .filter(|spot| {
                    let angle = (**spot - center)
                        .to_angle()
                        .rem_euclid(std::f32::consts::TAU);
                    (angle / std::f32::consts::FRAC_PI_2) as usize == quarter
                })
                .count();
            assert!(count >= 1, "quarter {quarter}");
        }
    }

    #[test]
    fn barrage_shells_land_one_after_another() {
        let times = landing_times(12, 1.5, 1.0);
        assert_eq!(times.len(), 12);
        assert_eq!(times[0], 1.5);
        assert!((times[11] - 2.5).abs() < 1e-6);
        assert!(times.windows(2).all(|pair| pair[1] > pair[0]));
        assert_eq!(landing_times(1, 1.5, 1.0), [1.5]);
    }
}
