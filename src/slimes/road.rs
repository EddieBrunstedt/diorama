//! How slimes get to the hearth, as plain data: the road as a line in world x/z, and the hop that
//! carries them along it.

use crate::island::{Island, cell_to_world};
use crate::voxel::TILE_SIZE;
use bevy::prelude::*;

/// How far from the hearth's middle the road ends: slimes stop at the stone ring and leap in.
const HEARTH_STOP: f32 = 0.45;

/// Seconds per hop, from one landing to the next.
pub const HOP_SECONDS: f32 = 0.6;
/// The part of each hop spent on the ground, squashed, before taking off.
const HOP_GROUND: f32 = 0.3;
/// How high a hop goes, in world units.
const HOP_HEIGHT: f32 = 0.16;

/// The road from the island's edge to the hearth, as straight legs between points.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RoadPath {
    points: Vec<Vec2>,
}

impl RoadPath {
    pub fn new(points: Vec<Vec2>) -> Self {
        Self { points }
    }

    /// The island's road: from the edge at its open end, through the middle of each road tile,
    /// to the stone ring around the hearth. `None` if there's no road to the hearth.
    pub fn from_island(island: &Island) -> Option<Self> {
        let cells = island.road_path();
        let (open_end, side) = island.road_entry()?;
        let (&hearth, road) = cells.split_last()?;
        let center = |cell: IVec2| cell_to_world(cell).xz();
        let mut points = vec![center(open_end) + side.offset().as_vec2() * TILE_SIZE / 2.0];
        points.extend(road.iter().map(|&cell| center(cell)));
        let last = *points.last()?;
        let toward_road = (last - center(hearth)).normalize_or(Vec2::X);
        points.push(center(hearth) + toward_road * HEARTH_STOP);
        Some(Self::new(points))
    }

    pub fn length(&self) -> f32 {
        self.points
            .windows(2)
            .map(|pair| pair[0].distance(pair[1]))
            .sum()
    }

    /// Where the hearth end of the road is.
    pub fn end(&self) -> Vec2 {
        self.points.last().copied().unwrap_or_default()
    }

    /// The point `distance` along the road (clamped to its ends) and the direction it runs there.
    pub fn sample(&self, distance: f32) -> (Vec2, Vec2) {
        let mut left = distance.max(0.0);
        let mut last = (self.end(), Vec2::X);
        for pair in self.points.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let length = a.distance(b);
            let direction = (b - a).normalize_or(Vec2::X);
            if left <= length {
                return (a + direction * left, direction);
            }
            left -= length;
            last = (b, direction);
        }
        last
    }
}

/// Where a slime is in its hop.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hop {
    /// Height above the ground.
    pub height: f32,
    /// In the air (moving along), as opposed to squashed on the ground gathering itself.
    pub airborne: bool,
}

/// The hop at `phase` (hops so far; the fraction is where in the current one).
pub fn hop(phase: f32) -> Hop {
    let t = phase.rem_euclid(1.0);
    if t < HOP_GROUND {
        return Hop {
            height: 0.0,
            airborne: false,
        };
    }
    let air = (t - HOP_GROUND) / (1.0 - HOP_GROUND);
    Hop {
        height: HOP_HEIGHT * 4.0 * air * (1.0 - air),
        airborne: true,
    }
}

/// Speed along the road while airborne, so that the average over whole hops is `speed`.
pub fn air_speed(speed: f32) -> f32 {
    speed / (1.0 - HOP_GROUND)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn road_runs_from_the_edge_to_the_hearth_ring() {
        let road = RoadPath::from_island(&Island::starting()).unwrap();
        // North of (1, -2), down to (1, 0), then west toward the hearth.
        assert_eq!(road.sample(0.0).0, Vec2::new(1.0, -2.5));
        assert_eq!(road.sample(0.0).1, Vec2::Y);
        assert!((road.end() - Vec2::new(HEARTH_STOP, 0.0)).length() < 1e-5);
        assert!((road.length() - (2.5 + 1.0 - HEARTH_STOP)).abs() < 1e-5);
    }

    #[test]
    fn samples_follow_the_bends_and_clamp() {
        let road = RoadPath::new(vec![Vec2::ZERO, Vec2::new(0.0, 1.0), Vec2::new(-1.0, 1.0)]);
        assert_eq!(road.sample(0.5), (Vec2::new(0.0, 0.5), Vec2::Y));
        assert_eq!(road.sample(1.5), (Vec2::new(-0.5, 1.0), Vec2::NEG_X));
        assert_eq!(road.sample(-1.0).0, Vec2::ZERO);
        assert_eq!(road.sample(9.0), (Vec2::new(-1.0, 1.0), Vec2::NEG_X));
    }

    #[test]
    fn no_road_no_path() {
        assert_eq!(RoadPath::from_island(&Island::from_map(&[".H."])), None);
    }

    #[test]
    fn hops_rest_then_arc() {
        assert_eq!(hop(0.1).height, 0.0);
        assert!(!hop(0.1).airborne);
        assert!(hop(0.5).airborne);
        let top = hop(HOP_GROUND + (1.0 - HOP_GROUND) / 2.0).height;
        assert!((top - HOP_HEIGHT).abs() < 1e-5);
        assert!(hop(3.999).height < 0.01, "lands at the end of each hop");
    }

    #[test]
    fn average_speed_over_whole_hops() {
        // Step through two hops, moving only while airborne.
        let (speed, dt) = (0.5, 0.001);
        let steps = (2.0 * HOP_SECONDS / dt) as usize;
        let moved: f32 = (0..steps)
            .map(|k| k as f32 * dt / HOP_SECONDS)
            .filter(|&phase| hop(phase).airborne)
            .map(|_| air_speed(speed) * dt)
            .sum();
        assert!((moved - speed * 2.0 * HOP_SECONDS).abs() < 0.01);
    }
}
