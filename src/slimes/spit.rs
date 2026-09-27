//! Slimes spitting goo at towers, as plain data: which tower a slime spits at.
//!
//! A slime spits when it lands a hop, if it's had time to gather more goo since its last spit and
//! a tower that still stands is within reach. Towers next to the road get hurt; towers a tile
//! further back are out of reach. So every stretch of road that runs past a tower is more shooting
//! for the tower, but also more goo on it and more repairs for the keeper.

use bevy::prelude::*;

/// A tower a slime could spit at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpitTarget<T> {
    pub id: T,
    /// World x/z of the tower's middle.
    pub position: Vec2,
    pub broken: bool,
}

/// The tower a slime at `from` spits at: the nearest one within `reach` that isn't broken yet.
pub fn pick_spit_target<T: Copy>(
    from: Vec2,
    reach: f32,
    towers: impl IntoIterator<Item = SpitTarget<T>>,
) -> Option<T> {
    towers
        .into_iter()
        .filter(|tower| !tower.broken)
        .map(|tower| (tower.position.distance(from), tower.id))
        .filter(|(distance, _)| *distance <= reach)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, id)| id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tower(id: usize, x: f32, broken: bool) -> SpitTarget<usize> {
        SpitTarget {
            id,
            position: Vec2::new(x, 0.0),
            broken,
        }
    }

    #[test]
    fn spits_at_the_nearest_standing_tower_in_reach() {
        let towers = [
            tower(0, 1.0, false),
            tower(1, 0.6, true),
            tower(2, 0.8, false),
        ];
        assert_eq!(pick_spit_target(Vec2::ZERO, 1.2, towers), Some(2));
    }

    #[test]
    fn leaves_far_and_broken_towers_alone() {
        assert_eq!(
            pick_spit_target(Vec2::ZERO, 1.2, [tower(0, 1.5, false)]),
            None
        );
        assert_eq!(
            pick_spit_target(Vec2::ZERO, 1.2, [tower(0, 0.5, true)]),
            None
        );
        assert_eq!(pick_spit_target::<usize>(Vec2::ZERO, 1.2, []), None);
    }
}
