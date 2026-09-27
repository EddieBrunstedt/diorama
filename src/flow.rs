//! The flow field: for every cell, how far the horde has to go to reach the Command Center and
//! which way to head from there. One pass over the grid serves every swarmer, so a thousand of
//! them cost about the same as one. It's recomputed whenever the layout changes.
//!
//! Distances come from Dijkstra's algorithm from the targets outward, but each cell's distance is
//! solved from its neighbors as a wavefront (the fast marching method) rather than summed along
//! grid steps. Straight-line distances come out nearly right in every direction, so swarmers head
//! straight for where they're going instead of along the 8 grid directions.
//!
//! Plain data: [`FlowField`] only needs a [`Grid`], and [`FlowPlugin`] keeps one up to date.

use crate::build::{BuildSystems, StructureChanged};
use crate::grid::{Cell, Grid, Structure};
use crate::map::Landmarks;
use crate::tuning::flow::WALL_COST;
use bevy::prelude::*;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

pub struct FlowPlugin;

impl Plugin for FlowPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, build_field)
            .add_systems(Update, update_field.in_set(FlowSystems).after(BuildSystems));
    }
}

/// Keeps the [`FlowField`] up to date. Whatever follows the field runs after it.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FlowSystems;

fn build_field(mut commands: Commands, grid: Res<Grid>, landmarks: Res<Landmarks>) {
    commands.insert_resource(FlowField::new(&grid, landmarks.command_center.cells()));
}

fn update_field(
    mut changes: MessageReader<StructureChanged>,
    grid: Res<Grid>,
    landmarks: Res<Landmarks>,
    mut field: ResMut<FlowField>,
) {
    if changes.read().count() > 0 {
        *field = FlowField::new(&grid, landmarks.command_center.cells());
    }
}

/// What it costs to walk through a cell, in cells of open ground. Infinite if it can't be
/// walked through at all.
pub fn cell_cost(cell: &Cell) -> f32 {
    if !cell.terrain.walkable() {
        return f32::INFINITY;
    }
    match cell.structure {
        Some(Structure::Wall) => WALL_COST,
        Some(Structure::CommandCenter) | None => 1.0,
    }
}

/// Whether nothing stands in the way on a cell: a swarmer can walk across it (the Command
/// Center counts, since reaching it is the point).
pub fn is_clear(cell: &Cell) -> bool {
    cell.terrain.walkable() && cell.structure != Some(Structure::Wall)
}

/// Distances to the targets and the way to go, for every cell of the map.
#[derive(Resource, Clone, Debug)]
pub struct FlowField {
    /// The lowest cell covered.
    origin: IVec2,
    /// Cells covered along x and y.
    size: IVec2,
    /// Per cell, row by row: the cost of the cheapest way to a target (infinite if there's none).
    distance: Vec<f32>,
    /// Per cell: the way downhill (unit length), or zero on a target or where there's no way.
    direction: Vec<Vec2>,
    /// Per cell: see [`is_clear`].
    clear: Vec<bool>,
}

impl FlowField {
    /// The field toward `targets` over the whole map.
    pub fn new(grid: &Grid, targets: impl IntoIterator<Item = IVec2>) -> Self {
        let Some(bounds) = grid.bounds() else {
            return Self {
                origin: IVec2::ZERO,
                size: IVec2::ZERO,
                distance: Vec::new(),
                direction: Vec::new(),
                clear: Vec::new(),
            };
        };
        let origin = bounds.min;
        let size = bounds.size() + IVec2::ONE;
        let count = (size.x * size.y) as usize;
        let mut costs = Vec::with_capacity(count);
        let mut clear = Vec::with_capacity(count);
        for y in 0..size.y {
            for x in 0..size.x {
                match grid.cell(origin + IVec2::new(x, y)) {
                    Some(cell) => {
                        costs.push(cell_cost(cell));
                        clear.push(is_clear(cell));
                    }
                    None => {
                        costs.push(f32::INFINITY);
                        clear.push(false);
                    }
                }
            }
        }
        let mut field = Self {
            origin,
            size,
            distance: vec![f32::INFINITY; count],
            direction: vec![Vec2::ZERO; count],
            clear,
        };
        field.march(&costs, targets);
        field.point_downhill();
        field
    }

    fn index(&self, cell: IVec2) -> Option<usize> {
        let local = cell - self.origin;
        (local.cmpge(IVec2::ZERO).all() && local.cmplt(self.size).all())
            .then(|| (local.y * self.size.x + local.x) as usize)
    }

    fn cell_of(&self, index: usize) -> IVec2 {
        let index = index as i32;
        self.origin + IVec2::new(index % self.size.x, index / self.size.x)
    }

    /// The area the field covers on the ground (world x and z).
    pub fn area(&self) -> Rect {
        Rect::from_corners(self.origin.as_vec2(), (self.origin + self.size).as_vec2())
    }

    /// The cost of the cheapest way from `cell` to a target: infinite if there's none (or it's
    /// off the map).
    pub fn distance(&self, cell: IVec2) -> f32 {
        self.index(cell).map_or(f32::INFINITY, |i| self.distance[i])
    }

    /// The way to head from the middle of `cell` (zero on a target or where there's no way).
    pub fn direction(&self, cell: IVec2) -> Vec2 {
        self.index(cell).map_or(Vec2::ZERO, |i| self.direction[i])
    }

    /// Whether nothing stands in the way on `cell` (false off the map). See [`is_clear`].
    pub fn is_clear(&self, cell: IVec2) -> bool {
        self.index(cell).is_some_and(|i| self.clear[i])
    }

    /// Dijkstra from the targets outward, with each cell's distance solved as a wavefront from
    /// its settled neighbors.
    fn march(&mut self, costs: &[f32], targets: impl IntoIterator<Item = IVec2>) {
        // Distances are never negative, so their bits sort the same way the numbers do.
        let mut queue = BinaryHeap::new();
        for target in targets {
            if let Some(i) = self.index(target) {
                self.distance[i] = 0.0;
                queue.push(Reverse((0u32, i)));
            }
        }
        let mut settled = vec![false; self.distance.len()];
        while let Some(Reverse((bits, i))) = queue.pop() {
            if settled[i] || f32::from_bits(bits) > self.distance[i] {
                continue;
            }
            settled[i] = true;
            let cell = self.cell_of(i);
            for step in [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y] {
                let Some(n) = self.index(cell + step) else {
                    continue;
                };
                if settled[n] || costs[n].is_infinite() {
                    continue;
                }
                let settled_distance = |cell: IVec2| {
                    self.index(cell)
                        .filter(|&i| settled[i])
                        .map_or(f32::INFINITY, |i| self.distance[i])
                };
                let neighbor = cell + step;
                let along_x = settled_distance(neighbor - IVec2::X)
                    .min(settled_distance(neighbor + IVec2::X));
                let along_y = settled_distance(neighbor - IVec2::Y)
                    .min(settled_distance(neighbor + IVec2::Y));
                let distance = wavefront(along_x, along_y, costs[n]);
                if distance < self.distance[n] {
                    self.distance[n] = distance;
                    queue.push(Reverse((distance.to_bits(), n)));
                }
            }
        }
    }

    /// Points every cell down the steepest slope of the distances, looking along each axis at
    /// whichever neighbor is lower.
    fn point_downhill(&mut self) {
        for i in 0..self.distance.len() {
            let here = self.distance[i];
            if here == 0.0 || here.is_infinite() {
                continue;
            }
            let cell = self.cell_of(i);
            // How much the distance drops per cell, moving along an axis in the better direction.
            let slope = |axis: IVec2| -> f32 {
                let behind = self.distance(cell - axis);
                let ahead = self.distance(cell + axis);
                if behind.min(ahead) >= here {
                    0.0
                } else if ahead <= behind {
                    here - ahead
                } else {
                    behind - here
                }
            };
            self.direction[i] = Vec2::new(slope(IVec2::X), slope(IVec2::Y)).normalize_or_zero();
        }
    }

    /// The way to head from a point on the ground (world x and z), blended between the nearest
    /// cells so it turns smoothly. Only clear cells are blended in, so a wall doesn't bend the
    /// way along it; on a cell that isn't clear, that cell's own way is used.
    pub fn direction_at(&self, point: Vec2) -> Vec2 {
        let here = point.floor().as_ivec2();
        if !self.is_clear(here) {
            return self.direction(here);
        }
        // Blend between the four cell centers around the point.
        let p = point - Vec2::splat(0.5);
        let base = p.floor();
        let t = p - base;
        let base = base.as_ivec2();
        let mut sum = Vec2::ZERO;
        for (offset, weight) in [
            (IVec2::new(0, 0), (1.0 - t.x) * (1.0 - t.y)),
            (IVec2::new(1, 0), t.x * (1.0 - t.y)),
            (IVec2::new(0, 1), (1.0 - t.x) * t.y),
            (IVec2::new(1, 1), t.x * t.y),
        ] {
            let cell = base + offset;
            if self.is_clear(cell) {
                sum += self.direction(cell) * weight;
            }
        }
        sum.normalize_or(self.direction(here))
    }

    /// The cells from `from` down to a target, taking the steepest step each time (diagonal steps
    /// only where both cells beside the step are clear). Ends early if it gets stuck, e.g. where
    /// there's no way at all.
    pub fn route(&self, from: IVec2) -> Vec<IVec2> {
        let mut route = vec![from];
        let mut cell = from;
        let mut here = self.distance(cell);
        while here > 0.0 && here.is_finite() && route.len() <= self.distance.len() {
            let mut best: Option<(f32, IVec2)> = None;
            for y in -1..=1 {
                for x in -1..=1 {
                    let step = IVec2::new(x, y);
                    if step == IVec2::ZERO {
                        continue;
                    }
                    let diagonal = x != 0 && y != 0;
                    if diagonal
                        && !(self.is_clear(cell + IVec2::new(x, 0))
                            && self.is_clear(cell + IVec2::new(0, y)))
                    {
                        continue;
                    }
                    let drop = here - self.distance(cell + step);
                    let slope = if diagonal {
                        drop / std::f32::consts::SQRT_2
                    } else {
                        drop
                    };
                    if slope > 0.0 && best.is_none_or(|(best, _)| slope > best) {
                        best = Some((slope, cell + step));
                    }
                }
            }
            let Some((_, next)) = best else {
                break;
            };
            cell = next;
            here = self.distance(cell);
            route.push(cell);
        }
        route
    }
}

/// A cell's distance from its lower neighbor along x and along y (either may be infinite), when
/// crossing it costs `cost`. Where both neighbors are close enough to matter, the wavefront
/// arrives between them, which is what makes diagonal distances come out right.
fn wavefront(along_x: f32, along_y: f32, cost: f32) -> f32 {
    let (low, high) = if along_x <= along_y {
        (along_x, along_y)
    } else {
        (along_y, along_x)
    };
    if high - low >= cost {
        low + cost
    } else {
        (low + high + (2.0 * cost * cost - (high - low).powi(2)).sqrt()) / 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::{Chunk, Terrain};
    use crate::tuning::grid::CHUNK_SIZE;

    /// One chunk of ground: cells 0..16 on both axes.
    fn open_ground() -> Grid {
        let mut grid = Grid::default();
        grid.insert_chunk(IVec2::ZERO, Chunk::new(Terrain::Ground));
        grid
    }

    fn wall(grid: &mut Grid, cells: impl IntoIterator<Item = (i32, i32)>) {
        for (x, y) in cells {
            grid.set_structure(IVec2::new(x, y), Some(Structure::Wall));
        }
    }

    fn angle_between(a: Vec2, b: Vec2) -> f32 {
        a.angle_to(b).abs().to_degrees()
    }

    #[test]
    fn open_ground_distances_are_nearly_straight_lines() {
        let target = IVec2::new(2, 2);
        let field = FlowField::new(&open_ground(), [target]);
        assert_eq!(field.distance(target), 0.0);
        assert_eq!(field.distance(IVec2::new(12, 2)), 10.0);
        for cell in [IVec2::new(12, 12), IVec2::new(14, 7), IVec2::new(4, 15)] {
            let straight = (cell - target).as_vec2().length();
            let distance = field.distance(cell);
            assert!(
                (distance - straight).abs() / straight < 0.06,
                "{cell}: {distance} vs {straight}"
            );
            let toward = (target - cell).as_vec2();
            let direction = field.direction(cell);
            assert!(
                angle_between(direction, toward) < 8.0,
                "{cell}: heads {direction}, not toward the target"
            );
        }
    }

    #[test]
    fn the_way_goes_around_walls_through_the_gap() {
        let mut grid = open_ground();
        // A wall across the map at y = 8, with a gap at x = 13.
        wall(
            &mut grid,
            (0..CHUNK_SIZE).filter(|&x| x != 13).map(|x| (x, 8)),
        );
        let target = IVec2::new(3, 2);
        let field = FlowField::new(&grid, [target]);
        let start = IVec2::new(3, 14);
        let route = field.route(start);
        assert_eq!(route.last(), Some(&target));
        assert!(route.contains(&IVec2::new(13, 8)), "{route:?}");
        assert!(route.iter().all(|&cell| field.is_clear(cell)));
        // The detour makes it much further than the straight line.
        assert!(field.distance(start) > 20.0);
        // Right below the wall, the way leads along it toward the gap.
        assert!(field.direction(IVec2::new(5, 9)).x > 0.9);
    }

    #[test]
    fn a_sealed_target_is_reached_through_the_nearest_wall() {
        let mut grid = open_ground();
        // A ring of walls around cells (6..=9, 6..=9).
        let ring = (5..=10).flat_map(|i| [(i, 5), (i, 10), (5, i), (10, i)]);
        wall(&mut grid, ring);
        let target = IVec2::new(7, 7);
        let field = FlowField::new(&grid, [target]);
        let start = IVec2::new(7, 15);
        assert!(field.distance(start) > WALL_COST);
        assert!(field.distance(start).is_finite());
        let route = field.route(start);
        assert_eq!(route.last(), Some(&target));
        let walls: Vec<IVec2> = route
            .iter()
            .copied()
            .filter(|&cell| grid.structure(cell) == Some(Structure::Wall))
            .collect();
        assert_eq!(walls, vec![IVec2::new(7, 10)], "{route:?}");
    }

    #[test]
    fn rock_is_impassable() {
        let mut grid = open_ground();
        // Rock around the corner cell (0, 0) cuts it off.
        for cell in [(1, 0), (1, 1), (0, 1)] {
            grid.set_terrain(IVec2::from(cell), Terrain::Rock);
        }
        let field = FlowField::new(&grid, [IVec2::new(8, 8)]);
        assert!(field.distance(IVec2::new(1, 1)).is_infinite());
        assert!(field.distance(IVec2::ZERO).is_infinite());
        assert_eq!(field.direction(IVec2::ZERO), Vec2::ZERO);
        assert_eq!(field.route(IVec2::ZERO), vec![IVec2::ZERO]);
        assert!(!field.is_clear(IVec2::new(1, 1)));
        assert!(!field.is_clear(IVec2::new(-1, 3)), "off the map");
    }

    #[test]
    fn nothing_leaks_between_diagonal_rocks() {
        let mut grid = open_ground();
        // A diagonal line of rock from (0, 6) to (6, 0): the corner below it is sealed off, even
        // though the rocks only touch at their corners.
        for i in 0..=6 {
            grid.set_terrain(IVec2::new(i, 6 - i), Terrain::Rock);
        }
        let field = FlowField::new(&grid, [IVec2::new(12, 12)]);
        assert!(field.distance(IVec2::new(2, 2)).is_infinite());
        assert!(field.distance(IVec2::new(4, 4)).is_finite());
    }

    #[test]
    fn routes_go_downhill_to_the_target() {
        let mut grid = open_ground();
        wall(&mut grid, (2..12).map(|y| (6, y)));
        let target = IVec2::new(10, 5);
        let field = FlowField::new(&grid, [target]);
        let route = field.route(IVec2::new(1, 7));
        assert_eq!(route.last(), Some(&target));
        for pair in route.windows(2) {
            assert!(field.distance(pair[1]) < field.distance(pair[0]));
            let step = pair[1] - pair[0];
            assert!(step.abs().max_element() == 1);
        }
    }

    #[test]
    fn directions_blend_between_cells() {
        let target = IVec2::new(8, 0);
        let field = FlowField::new(&open_ground(), [target]);
        // Exactly at a cell's middle, the blend is that cell's way.
        let cell = IVec2::new(3, 10);
        let middle = cell.as_vec2() + Vec2::splat(0.5);
        assert!(field.direction_at(middle).distance(field.direction(cell)) < 1e-5);
        // Between two cells, it's between their ways.
        let left = field.direction(IVec2::new(3, 10));
        let right = field.direction(IVec2::new(4, 10));
        let between = field.direction_at(Vec2::new(4.0, 10.5));
        assert!(angle_between(left, between) < angle_between(left, right));
        assert!(angle_between(between, right) < angle_between(left, right));
        assert!((between.length() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn wavefront_meets_between_two_neighbors() {
        // One neighbor: a plain step.
        assert_eq!(wavefront(3.0, f32::INFINITY, 1.0), 4.0);
        // Two equal neighbors: the diagonal of a unit square from both sides, 1/sqrt(2) further.
        let both = wavefront(0.0, 0.0, 1.0);
        assert!((both - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
        // A costly cell takes longer to cross.
        assert_eq!(wavefront(2.0, f32::INFINITY, WALL_COST), 2.0 + WALL_COST);
    }
}
