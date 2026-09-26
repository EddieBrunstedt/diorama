//! Walking around the island, as plain data: a fine grid of walkable spots, A* over it, and
//! smoothing so paths are straight lines that bend around obstacles instead of zigzagging.
//!
//! Each tile is split into [`SUBCELLS_PER_TILE`]² subcells. Subcells on a tile are ground; the
//! hearth's middle, tree trunks and tower footprints block them.

use crate::island::{Island, TileKind};
use crate::voxel::TILE_SIZE;
use bevy::prelude::*;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};

pub const SUBCELLS_PER_TILE: i32 = 5;
pub const SUBCELL: f32 = TILE_SIZE / SUBCELLS_PER_TILE as f32;
/// Half the size of the blocked square in the hearth tile's middle.
const HEARTH_HALF: f32 = 0.3;
/// Half the size of a tower's footprint, centered on its tile.
pub const TOWER_HALF: f32 = 0.3;
/// How far the keeper's middle stays from blocked subcells when cutting corners.
const CLEARANCE: f32 = 0.07;
/// Sampling distance when checking whether a straight line is walkable.
const SIGHT_STEP: f32 = 0.04;
/// Path costs are integers in hundredths of a world unit.
const COST_SCALE: f32 = 100.0;

/// The subcell containing a world x/z point.
pub fn subcell_of(point: Vec2) -> IVec2 {
    ((point + Vec2::splat(TILE_SIZE / 2.0)) / SUBCELL)
        .floor()
        .as_ivec2()
}

/// World x/z center of a subcell.
pub fn subcell_center(subcell: IVec2) -> Vec2 {
    (subcell.as_vec2() + 0.5) * SUBCELL - Vec2::splat(TILE_SIZE / 2.0)
}

/// Total length of a path, in world units.
pub fn path_length(path: &[Vec2]) -> f32 {
    path.windows(2).map(|pair| pair[0].distance(pair[1])).sum()
}

/// Where a path should end.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Goal {
    pub point: Vec2,
    /// Any walkable subcell whose center is this close to `point` is good enough.
    pub reach: f32,
    /// `Some(d)`: stop `d` away from `point`, on the side the keeper arrives from (for working on
    /// a tree or build site). `None`: stop on `point` if it's walkable, else at the reached subcell.
    pub stand_off: Option<f32>,
}

impl Goal {
    /// Stand on `point`.
    pub fn at(point: Vec2) -> Self {
        Self {
            point,
            reach: SUBCELL * 0.75,
            stand_off: None,
        }
    }

    /// Get within `reach` of an unwalkable `point`, as close as possible.
    pub fn nearby(point: Vec2, reach: f32) -> Self {
        Self {
            point,
            reach,
            stand_off: None,
        }
    }

    /// Stand `stand_off` away from `point`, e.g. to work on it.
    pub fn beside(point: Vec2, reach: f32, stand_off: f32) -> Self {
        Self {
            point,
            reach,
            stand_off: Some(stand_off),
        }
    }
}

#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct NavGrid {
    ground: HashSet<IVec2>,
    blocked: HashSet<IVec2>,
}

impl NavGrid {
    pub fn new(island: &Island) -> Self {
        let mut grid = Self::default();
        for (cell, kind) in island.tiles() {
            for i in 0..SUBCELLS_PER_TILE {
                for j in 0..SUBCELLS_PER_TILE {
                    grid.ground
                        .insert(cell * SUBCELLS_PER_TILE + IVec2::new(i, j));
                }
            }
            if kind == TileKind::Hearth {
                grid.block_square(cell.as_vec2() * TILE_SIZE, HEARTH_HALF);
            }
        }
        grid
    }

    /// Blocks every subcell whose center lies in the square of half size `half` around `center`.
    pub fn block_square(&mut self, center: Vec2, half: f32) {
        let (min, max) = (subcell_of(center - half), subcell_of(center + half));
        for x in min.x..=max.x {
            for y in min.y..=max.y {
                let subcell = IVec2::new(x, y);
                let offset = (subcell_center(subcell) - center).abs();
                if offset.max_element() <= half {
                    self.blocked.insert(subcell);
                }
            }
        }
    }

    /// Blocks the subcell containing `point` (e.g. a tree trunk).
    pub fn block_point(&mut self, point: Vec2) {
        self.blocked.insert(subcell_of(point));
    }

    pub fn unblock_point(&mut self, point: Vec2) {
        self.blocked.remove(&subcell_of(point));
    }

    pub fn block_tower(&mut self, cell: IVec2) {
        self.block_square(cell.as_vec2() * TILE_SIZE, TOWER_HALF);
    }

    fn walkable_subcell(&self, subcell: IVec2) -> bool {
        self.ground.contains(&subcell) && !self.blocked.contains(&subcell)
    }

    pub fn is_walkable(&self, point: Vec2) -> bool {
        self.walkable_subcell(subcell_of(point))
    }

    /// Walkable with some room around it.
    fn is_clear(&self, point: Vec2) -> bool {
        [
            Vec2::ZERO,
            Vec2::X * CLEARANCE,
            Vec2::NEG_X * CLEARANCE,
            Vec2::Y * CLEARANCE,
            Vec2::NEG_Y * CLEARANCE,
        ]
        .into_iter()
        .all(|offset| self.is_walkable(point + offset))
    }

    /// Whether the keeper can walk straight from `a` to `b`.
    pub fn line_of_sight(&self, a: Vec2, b: Vec2) -> bool {
        let steps = (a.distance(b) / SIGHT_STEP).ceil().max(1.0) as usize;
        (0..=steps).all(|k| self.is_clear(a.lerp(b, k as f32 / steps as f32)))
    }

    /// A walking path from `from` to `goal`: `from`, the corners to go around, and the end point.
    /// `None` if the goal can't be reached.
    pub fn find_path(&self, from: Vec2, goal: &Goal) -> Option<Vec<Vec2>> {
        let chain = self.search(subcell_of(from), goal)?;
        let last = *chain.last().unwrap();
        let end = match goal.stand_off {
            Some(distance) => {
                let away = (subcell_center(last) - goal.point).normalize_or(Vec2::Y);
                goal.point + away * distance
            }
            None if self.is_walkable(goal.point) => goal.point,
            None => subcell_center(last),
        };
        // The reached subcell's center is left out: the end point is in or right next to it.
        let mut points = vec![from];
        let middle = chain.len().saturating_sub(2);
        points.extend(
            chain
                .iter()
                .skip(1)
                .take(middle)
                .map(|&subcell| subcell_center(subcell)),
        );
        points.push(end);
        Some(self.smooth(&points))
    }

    /// A* from `start` to the nearest subcell that satisfies `goal`. The start may be blocked: the
    /// keeper can always walk out of wherever he's standing (e.g. a site that appeared under him),
    /// crossing blocked ground until he's out, but never back into it.
    fn search(&self, start: IVec2, goal: &Goal) -> Option<Vec<IVec2>> {
        let is_goal = |subcell: IVec2| {
            self.walkable_subcell(subcell)
                && subcell_center(subcell).distance(goal.point) <= goal.reach
        };
        let estimate = |subcell: IVec2| {
            let distance = subcell_center(subcell).distance(goal.point) - goal.reach;
            (distance.max(0.0) * COST_SCALE) as u32
        };
        let key = |subcell: IVec2| (subcell.x, subcell.y);

        let mut open = BinaryHeap::from([Reverse((estimate(start), 0, key(start)))]);
        let mut cost = HashMap::from([(start, 0)]);
        let mut came_from = HashMap::new();
        while let Some(Reverse((_, spent, (x, y)))) = open.pop() {
            let current = IVec2::new(x, y);
            if spent > cost[&current] {
                continue;
            }
            if is_goal(current) {
                let mut chain = vec![current];
                while let Some(&previous) = came_from.get(chain.last().unwrap()) {
                    chain.push(previous);
                }
                chain.reverse();
                return Some(chain);
            }
            let escaping = !self.walkable_subcell(current);
            let can_enter = |subcell: IVec2| {
                self.walkable_subcell(subcell) || (escaping && self.ground.contains(&subcell))
            };
            for dx in -1..=1 {
                for dy in -1..=1 {
                    let step = IVec2::new(dx, dy);
                    let next = current + step;
                    let diagonal = dx != 0 && dy != 0;
                    // No squeezing diagonally between two blocked subcells.
                    if step == IVec2::ZERO
                        || !can_enter(next)
                        || (diagonal
                            && !(can_enter(current + IVec2::new(dx, 0))
                                && can_enter(current + IVec2::new(0, dy))))
                    {
                        continue;
                    }
                    let step_cost = if diagonal {
                        SUBCELL * std::f32::consts::SQRT_2
                    } else {
                        SUBCELL
                    };
                    let next_cost = spent + (step_cost * COST_SCALE).round() as u32;
                    if cost.get(&next).is_none_or(|&known| next_cost < known) {
                        cost.insert(next, next_cost);
                        came_from.insert(next, current);
                        open.push(Reverse((next_cost + estimate(next), next_cost, key(next))));
                    }
                }
            }
        }
        None
    }

    /// Drops every point that can be skipped by walking in a straight line.
    fn smooth(&self, points: &[Vec2]) -> Vec<Vec2> {
        let mut smoothed = vec![points[0]];
        let mut i = 0;
        while i + 1 < points.len() {
            // Neighboring points are always accepted, so paths can start or end in blocked spots.
            let mut j = points.len() - 1;
            while j > i + 1 && !self.line_of_sight(points[i], points[j]) {
                j -= 1;
            }
            smoothed.push(points[j]);
            i = j;
        }
        smoothed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_field() -> Island {
        Island::from_map(&[
            "....", //
            "..H.", //
            "....", //
            "....", //
        ])
    }

    #[test]
    fn subcell_coordinates_round_trip() {
        for subcell in [IVec2::ZERO, IVec2::new(-7, 3), IVec2::new(12, -1)] {
            assert_eq!(subcell_of(subcell_center(subcell)), subcell);
        }
        // Tile (0, 0) spans -0.5..0.5 and starts at subcell 0.
        assert_eq!(subcell_of(Vec2::splat(-0.49)), IVec2::ZERO);
        assert_eq!(
            subcell_of(Vec2::splat(0.49)),
            IVec2::splat(SUBCELLS_PER_TILE - 1)
        );
    }

    #[test]
    fn hearth_middle_and_off_island_are_not_walkable() {
        let nav = NavGrid::new(&Island::starting());
        assert!(!nav.is_walkable(Vec2::ZERO));
        assert!(nav.is_walkable(Vec2::new(0.0, 0.42)));
        assert!(nav.is_walkable(Vec2::new(-2.0, 2.0)));
        assert!(!nav.is_walkable(Vec2::new(3.0, 0.0)));
    }

    #[test]
    fn open_ground_is_one_straight_line() {
        let nav = NavGrid::new(&open_field());
        let from = Vec2::new(-1.8, 1.9);
        let to = Vec2::new(0.9, 1.7);
        let path = nav.find_path(from, &Goal::at(to)).unwrap();
        assert_eq!(path, vec![from, to]);
    }

    #[test]
    fn paths_go_around_the_hearth() {
        let nav = NavGrid::new(&open_field());
        let from = Vec2::new(-1.0, 0.0);
        let to = Vec2::new(1.0, 0.0);
        let path = nav.find_path(from, &Goal::at(to)).unwrap();
        assert_eq!(*path.first().unwrap(), from);
        assert_eq!(*path.last().unwrap(), to);
        assert!(path.len() > 2, "has to bend around the hearth");
        for pair in path.windows(2) {
            assert!(nav.line_of_sight(pair[0], pair[1]));
        }
        // Barely longer than going straight through.
        assert!(path_length(&path) < 2.6, "{path:?}");
    }

    #[test]
    fn no_path_off_the_island() {
        let nav = NavGrid::new(&open_field());
        assert_eq!(
            nav.find_path(Vec2::ZERO, &Goal::at(Vec2::new(9.0, 9.0))),
            None
        );
    }

    #[test]
    fn stands_beside_a_blocked_target() {
        let mut nav = NavGrid::new(&open_field());
        let tree = Vec2::new(-1.2, 1.1);
        nav.block_point(tree);
        let path = nav
            .find_path(Vec2::new(1.5, 1.5), &Goal::beside(tree, 0.3, 0.2))
            .unwrap();
        let end = *path.last().unwrap();
        assert!((end.distance(tree) - 0.2).abs() < 1e-4);
    }

    #[test]
    fn a_blocked_start_can_still_walk_out() {
        let mut nav = NavGrid::new(&open_field());
        nav.block_tower(IVec2::new(-1, 1));
        let path = nav
            .find_path(Vec2::new(-1.0, 1.0), &Goal::at(Vec2::new(-1.0, 2.0)))
            .unwrap();
        assert_eq!(*path.last().unwrap(), Vec2::new(-1.0, 2.0));
    }

    #[test]
    fn tower_blocks_its_footprint_only() {
        let mut nav = NavGrid::new(&open_field());
        nav.block_tower(IVec2::new(-1, 1));
        assert!(!nav.is_walkable(Vec2::new(-1.0, 1.0)));
        assert!(!nav.is_walkable(Vec2::new(-0.8, 1.2)));
        assert!(nav.is_walkable(Vec2::new(-0.6, 1.0)));
    }
}
