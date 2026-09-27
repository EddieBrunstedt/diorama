//! The flow fields: for every place on the map, how far the horde has to go to reach the Command
//! Center and which way to head from there. One pass over the grid serves every enemy of a class,
//! so a thousand of them cost about the same as one. They're recomputed whenever the layout
//! changes, and every so often while walls wear down.
//!
//! Each enemy [`Class`] has its own field. Swarmers fit in a cell, so their field has a node per
//! cell. Brutes need a 2×2 square, so their field has a node per 2×2 block of cells (a brute's
//! middle goes where the block's middle is). A 1-cell gap has no block that fits through it
//! without a wall or rock in it, so brutes can't squeeze through: they break a wall beside it.
//!
//! Walls are passable at a cost based on the HP they have left: when a maze makes the detour
//! longer than chewing through, the field leads through the wall.
//!
//! Distances come from Dijkstra's algorithm from the targets outward, but each node's distance is
//! solved from its neighbors as a wavefront (the fast marching method) rather than summed along
//! grid steps. Straight-line distances come out nearly right in every direction, so the horde
//! heads straight for where it's going instead of along the 8 grid directions.
//!
//! Plain data: [`FlowField`] only needs a [`Grid`], and [`FlowPlugin`] keeps [`FlowFields`] up to
//! date.

use crate::build::{BuildSystems, StructureChanged};
use crate::grid::{Cell, Grid, Structure};
use crate::map::Landmarks;
use crate::tuning::flow::{BRUTE_COST_PER_HP, SWARMER_COST_PER_HP, WEAR_REBUILD_INTERVAL};
use crate::tuning::horde::BRUTE_RADIUS;
use crate::walls::WallDamaged;
use bevy::prelude::*;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

pub struct FlowPlugin;

impl Plugin for FlowPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, build_fields).add_systems(
            Update,
            update_fields.in_set(FlowSystems).after(BuildSystems),
        );
    }
}

/// Keeps the [`FlowFields`] up to date. Whatever follows the fields runs after it.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FlowSystems;

fn build_fields(mut commands: Commands, grid: Res<Grid>, landmarks: Res<Landmarks>) {
    commands.insert_resource(FlowFields::new(&grid, &landmarks));
}

/// When the fields were last rebuilt (game seconds), and whether walls have worn down since.
#[derive(Default)]
struct Rebuilds {
    last: f32,
    worn: bool,
}

fn update_fields(
    time: Res<Time>,
    mut changes: MessageReader<StructureChanged>,
    mut damaged: MessageReader<WallDamaged>,
    grid: Res<Grid>,
    landmarks: Res<Landmarks>,
    mut fields: ResMut<FlowFields>,
    mut rebuilds: Local<Rebuilds>,
) {
    let changed = changes.read().count() > 0;
    rebuilds.worn |= damaged.read().count() > 0;
    let now = time.elapsed_secs();
    if changed || (rebuilds.worn && now - rebuilds.last >= WEAR_REBUILD_INTERVAL) {
        *fields = FlowFields::new(&grid, &landmarks);
        rebuilds.last = now;
        rebuilds.worn = false;
    }
}

/// How an enemy moves through the maze. Each class has its own flow field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Class {
    /// Small, fits through any gap.
    Swarmer,
    /// Too big for a 1-cell gap, and breaks walls fast.
    Brute,
}

impl Class {
    /// Cells along each side of the square the class needs to get through.
    pub fn span(self) -> i32 {
        match self {
            Class::Swarmer => 1,
            Class::Brute => (BRUTE_RADIUS * 2.0).ceil() as i32,
        }
    }

    /// What a wall with `hp` left costs to go through, in cells of open ground.
    pub fn wall_cost(self, hp: u32) -> f32 {
        let per_hp = match self {
            Class::Swarmer => SWARMER_COST_PER_HP,
            Class::Brute => BRUTE_COST_PER_HP,
        };
        1.0 + hp as f32 * per_hp
    }
}

/// What it costs `class` to walk through a cell, in cells of open ground. Infinite if it can't be
/// walked through at all.
pub fn cell_cost(cell: &Cell, class: Class) -> f32 {
    if !cell.terrain.walkable() {
        return f32::INFINITY;
    }
    match cell.structure {
        Some(Structure::Wall { hp }) => class.wall_cost(hp),
        Some(Structure::CommandCenter) | None => 1.0,
    }
}

/// Whether nothing stands in the way on a cell: an enemy can walk across it (the Command Center
/// counts, since reaching it is the point).
pub fn is_clear(cell: &Cell) -> bool {
    cell.terrain.walkable() && !matches!(cell.structure, Some(Structure::Wall { .. }))
}

/// A flow field for each enemy class.
#[derive(Resource, Clone, Debug)]
pub struct FlowFields {
    pub swarmer: FlowField,
    pub brute: FlowField,
}

impl FlowFields {
    /// The fields toward the Command Center.
    pub fn new(grid: &Grid, landmarks: &Landmarks) -> Self {
        let targets = || landmarks.command_center.cells();
        Self {
            swarmer: FlowField::new(grid, Class::Swarmer, targets()),
            brute: FlowField::new(grid, Class::Brute, targets()),
        }
    }

    pub fn get(&self, class: Class) -> &FlowField {
        match class {
            Class::Swarmer => &self.swarmer,
            Class::Brute => &self.brute,
        }
    }

    /// Whether nothing stands in the way on a single cell (false off the map). See [`is_clear`].
    pub fn cell_is_clear(&self, cell: IVec2) -> bool {
        // The swarmers' field has a node per cell.
        self.swarmer.is_clear(cell)
    }

    /// Whether a cell holds something the horde can break (a wall).
    pub fn cell_is_breakable(&self, cell: IVec2) -> bool {
        self.swarmer.is_breakable(cell)
    }
}

/// Distances to the targets and the way to go, for every node of the map. A node is a square of
/// `span`×`span` cells, named by its lowest cell; for swarmers, it's just a cell.
#[derive(Clone, Debug)]
pub struct FlowField {
    /// Cells along each side of a node.
    span: i32,
    /// The lowest node covered.
    origin: IVec2,
    /// Nodes covered along x and y.
    size: IVec2,
    /// Per node, row by row: what it costs to cross (infinite if it can't be).
    cost: Vec<f32>,
    /// Per node: the cost of the cheapest way to a target (infinite if there's none).
    distance: Vec<f32>,
    /// Per node: the way downhill (unit length), or zero on a target or where there's no way.
    direction: Vec<Vec2>,
    /// Per node: whether every cell in it is clear (see [`is_clear`]).
    clear: Vec<bool>,
}

impl FlowField {
    /// The field for `class` toward `targets` (cells) over the whole map. A node is a target if
    /// any of its cells is.
    pub fn new(grid: &Grid, class: Class, targets: impl IntoIterator<Item = IVec2>) -> Self {
        let span = class.span();
        let empty = Self {
            span,
            origin: IVec2::ZERO,
            size: IVec2::ZERO,
            cost: Vec::new(),
            distance: Vec::new(),
            direction: Vec::new(),
            clear: Vec::new(),
        };
        let Some(bounds) = grid.bounds() else {
            return empty;
        };
        // Each cell's cost over open ground (infinite if impassable), and whether it's clear.
        let cells_size = bounds.size() + IVec2::ONE;
        let mut cell_extra = Vec::with_capacity((cells_size.x * cells_size.y) as usize);
        let mut cell_clear = Vec::with_capacity(cell_extra.capacity());
        for y in 0..cells_size.y {
            for x in 0..cells_size.x {
                match grid.cell(bounds.min + IVec2::new(x, y)) {
                    Some(cell) => {
                        cell_extra.push(cell_cost(cell, class) - 1.0);
                        cell_clear.push(is_clear(cell));
                    }
                    None => {
                        cell_extra.push(f32::INFINITY);
                        cell_clear.push(false);
                    }
                }
            }
        }
        // A node costs 1 plus its cells' extra cost, shared out over its span: crossing a wall
        // head-on, a node's worth of cells passes over it once per cell of the span, so each wall
        // in the way is paid for about once.
        let size = (cells_size - IVec2::splat(span - 1)).max(IVec2::ZERO);
        let count = (size.x * size.y) as usize;
        let mut cost = Vec::with_capacity(count);
        let mut clear = Vec::with_capacity(count);
        for y in 0..size.y {
            for x in 0..size.x {
                let (mut extra, mut all_clear) = (0.0, true);
                for dy in 0..span {
                    for dx in 0..span {
                        let i = ((y + dy) * cells_size.x + x + dx) as usize;
                        extra += cell_extra[i];
                        all_clear &= cell_clear[i];
                    }
                }
                cost.push(1.0 + extra / span as f32);
                clear.push(all_clear);
            }
        }
        let mut field = Self {
            span,
            origin: bounds.min,
            size,
            cost,
            distance: vec![f32::INFINITY; count],
            direction: vec![Vec2::ZERO; count],
            clear,
        };
        let mut target_nodes = Vec::new();
        for target in targets {
            for dy in 0..span {
                for dx in 0..span {
                    target_nodes.extend(field.index(target - IVec2::new(dx, dy)));
                }
            }
        }
        field.march(target_nodes);
        field.point_downhill();
        field
    }

    fn index(&self, node: IVec2) -> Option<usize> {
        let local = node - self.origin;
        (local.cmpge(IVec2::ZERO).all() && local.cmplt(self.size).all())
            .then(|| (local.y * self.size.x + local.x) as usize)
    }

    fn node_of(&self, index: usize) -> IVec2 {
        let index = index as i32;
        self.origin + IVec2::new(index % self.size.x, index / self.size.x)
    }

    /// How far a node's middle is from its lowest cell's corner, along each axis.
    fn center_offset(&self) -> Vec2 {
        Vec2::splat(self.span as f32 / 2.0)
    }

    /// The node whose middle is nearest to a point on the ground (world x and z).
    pub fn node_at(&self, point: Vec2) -> IVec2 {
        (point - self.center_offset() + 0.5).floor().as_ivec2()
    }

    /// A node's middle on the ground (world x and z).
    pub fn node_center(&self, node: IVec2) -> Vec2 {
        node.as_vec2() + self.center_offset()
    }

    /// The cells a node covers.
    pub fn cells(&self, node: IVec2) -> impl Iterator<Item = IVec2> + use<> {
        let span = self.span;
        (0..span * span).map(move |i| node + IVec2::new(i % span, i / span))
    }

    /// The area the field covers on the ground (world x and z).
    pub fn area(&self) -> Rect {
        let far = self.origin + self.size + IVec2::splat(self.span - 1);
        Rect::from_corners(self.origin.as_vec2(), far.as_vec2())
    }

    /// The cost of the cheapest way from `node` to a target: infinite if there's none (or it's
    /// off the map).
    pub fn distance(&self, node: IVec2) -> f32 {
        self.index(node).map_or(f32::INFINITY, |i| self.distance[i])
    }

    /// The cheapest way to a target from any node that covers `cell`: how far it is to go
    /// through that cell.
    pub fn distance_through(&self, cell: IVec2) -> f32 {
        let span = self.span;
        (0..span * span)
            .map(|i| self.distance(cell - IVec2::new(i % span, i / span)))
            .fold(f32::INFINITY, f32::min)
    }

    /// The way to head from the middle of `node` (zero on a target or where there's no way).
    pub fn direction(&self, node: IVec2) -> Vec2 {
        self.index(node).map_or(Vec2::ZERO, |i| self.direction[i])
    }

    /// Whether nothing stands in the way anywhere in `node` (false off the map). See
    /// [`is_clear`].
    pub fn is_clear(&self, node: IVec2) -> bool {
        self.index(node).is_some_and(|i| self.clear[i])
    }

    /// Whether something stands in the way in `node`, but it can be broken through (a wall, not
    /// rock).
    pub fn is_breakable(&self, node: IVec2) -> bool {
        self.index(node)
            .is_some_and(|i| !self.clear[i] && self.cost[i].is_finite())
    }

    /// Dijkstra from the targets outward, with each node's distance solved as a wavefront from
    /// its settled neighbors.
    fn march(&mut self, targets: Vec<usize>) {
        // Distances are never negative, so their bits sort the same way the numbers do.
        let mut queue = BinaryHeap::new();
        for i in targets {
            self.distance[i] = 0.0;
            queue.push(Reverse((0u32, i)));
        }
        let mut settled = vec![false; self.distance.len()];
        while let Some(Reverse((bits, i))) = queue.pop() {
            if settled[i] || f32::from_bits(bits) > self.distance[i] {
                continue;
            }
            settled[i] = true;
            let node = self.node_of(i);
            for step in [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y] {
                let Some(n) = self.index(node + step) else {
                    continue;
                };
                if settled[n] || self.cost[n].is_infinite() {
                    continue;
                }
                let settled_distance = |node: IVec2| {
                    self.index(node)
                        .filter(|&i| settled[i])
                        .map_or(f32::INFINITY, |i| self.distance[i])
                };
                let neighbor = node + step;
                let along_x = settled_distance(neighbor - IVec2::X)
                    .min(settled_distance(neighbor + IVec2::X));
                let along_y = settled_distance(neighbor - IVec2::Y)
                    .min(settled_distance(neighbor + IVec2::Y));
                let distance = wavefront(along_x, along_y, self.cost[n]);
                if distance < self.distance[n] {
                    self.distance[n] = distance;
                    queue.push(Reverse((distance.to_bits(), n)));
                }
            }
        }
    }

    /// Points every node down the steepest slope of the distances, looking along each axis at
    /// whichever neighbor is lower.
    fn point_downhill(&mut self) {
        for i in 0..self.distance.len() {
            let here = self.distance[i];
            if here == 0.0 || here.is_infinite() {
                continue;
            }
            let node = self.node_of(i);
            // How much the distance drops per node, moving along an axis in the better direction.
            let slope = |axis: IVec2| -> f32 {
                let behind = self.distance(node - axis);
                let ahead = self.distance(node + axis);
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
    /// nodes so it turns smoothly. Only clear nodes are blended in, so a wall doesn't bend the
    /// way along it; on a node that isn't clear, that node's own way is used.
    pub fn direction_at(&self, point: Vec2) -> Vec2 {
        let here = self.node_at(point);
        if !self.is_clear(here) {
            return self.direction(here);
        }
        // Blend between the four node middles around the point.
        let p = point - self.center_offset();
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
            let node = base + offset;
            if self.is_clear(node) {
                sum += self.direction(node) * weight;
            }
        }
        sum.normalize_or(self.direction(here))
    }

    /// The nodes from `from` down to a target, taking the steepest step each time (diagonal steps
    /// only where both nodes beside the step are clear). Ends early if it gets stuck, e.g. where
    /// there's no way at all.
    pub fn route(&self, from: IVec2) -> Vec<IVec2> {
        let mut route = vec![from];
        let mut node = from;
        let mut here = self.distance(node);
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
                        && !(self.is_clear(node + IVec2::new(x, 0))
                            && self.is_clear(node + IVec2::new(0, y)))
                    {
                        continue;
                    }
                    let drop = here - self.distance(node + step);
                    let slope = if diagonal {
                        drop / std::f32::consts::SQRT_2
                    } else {
                        drop
                    };
                    if slope > 0.0 && best.is_none_or(|(best, _)| slope > best) {
                        best = Some((slope, node + step));
                    }
                }
            }
            let Some((_, next)) = best else {
                break;
            };
            node = next;
            here = self.distance(node);
            route.push(node);
        }
        route
    }

    /// The walls a route from `from` breaks through, in order, each once.
    pub fn breaches(&self, from: IVec2, grid: &Grid) -> Vec<IVec2> {
        let mut walls = Vec::new();
        for node in self.route(from) {
            if self.is_clear(node) {
                continue;
            }
            for cell in self.cells(node) {
                let wall = matches!(grid.structure(cell), Some(Structure::Wall { .. }));
                if wall && !walls.contains(&cell) {
                    walls.push(cell);
                }
            }
        }
        walls
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
    use crate::tuning::walls::WALL_HP;

    /// One chunk of ground: cells 0..16 on both axes.
    fn open_ground() -> Grid {
        let mut grid = Grid::default();
        grid.insert_chunk(IVec2::ZERO, Chunk::new(Terrain::Ground));
        grid
    }

    fn wall(grid: &mut Grid, cells: impl IntoIterator<Item = (i32, i32)>) {
        for (x, y) in cells {
            grid.set_structure(IVec2::new(x, y), Some(Structure::WALL));
        }
    }

    fn angle_between(a: Vec2, b: Vec2) -> f32 {
        a.angle_to(b).abs().to_degrees()
    }

    #[test]
    fn open_ground_distances_are_nearly_straight_lines() {
        let target = IVec2::new(2, 2);
        let field = FlowField::new(&open_ground(), Class::Swarmer, [target]);
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
        let field = FlowField::new(&grid, Class::Swarmer, [target]);
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
        let field = FlowField::new(&grid, Class::Swarmer, [target]);
        let start = IVec2::new(7, 15);
        assert!(field.distance(start) > Class::Swarmer.wall_cost(WALL_HP));
        assert!(field.distance(start).is_finite());
        let route = field.route(start);
        assert_eq!(route.last(), Some(&target));
        let walls: Vec<IVec2> = route
            .iter()
            .copied()
            .filter(|&cell| grid.structure(cell) == Some(Structure::WALL))
            .collect();
        assert_eq!(walls, vec![IVec2::new(7, 10)], "{route:?}");
        assert_eq!(field.breaches(start, &grid), walls);
    }

    #[test]
    fn walls_cost_less_as_they_wear_down() {
        let fresh = Class::Swarmer.wall_cost(WALL_HP);
        assert_eq!(Class::Swarmer.wall_cost(0), 1.0);
        assert!(Class::Swarmer.wall_cost(WALL_HP / 2) < fresh);
        assert!(
            Class::Brute.wall_cost(WALL_HP) < fresh,
            "brutes break walls fast, so walls cost them less"
        );
        // A ring of walls around the target: the horde goes for the damaged wall, even though
        // it's further round.
        let mut grid = open_ground();
        let ring = (5..=10).flat_map(|i| [(i, 5), (i, 10), (5, i), (10, i)]);
        wall(&mut grid, ring);
        let damaged = IVec2::new(9, 10);
        grid.damage(damaged, WALL_HP * 3 / 4);
        let target = IVec2::new(7, 7);
        let field = FlowField::new(&grid, Class::Swarmer, [target]);
        assert_eq!(field.breaches(IVec2::new(7, 15), &grid), vec![damaged]);
        assert!(field.is_breakable(damaged));
        assert!(!field.is_breakable(IVec2::new(7, 12)), "open ground");
    }

    /// Open ground with a line of `what` across y = 8, open at the given x.
    fn line_with_gap(gap: &[i32], rock: bool) -> Grid {
        let mut grid = open_ground();
        for x in (0..CHUNK_SIZE).filter(|x| !gap.contains(x)) {
            let cell = IVec2::new(x, 8);
            if rock {
                grid.set_terrain(cell, Terrain::Rock);
            } else {
                grid.set_structure(cell, Some(Structure::WALL));
            }
        }
        grid
    }

    #[test]
    fn brutes_dont_fit_through_one_cell_gaps() {
        let target = IVec2::new(3, 2);
        let start = IVec2::new(3, 13);
        // Rock with a 1-cell gap: swarmers get through, brutes can't at all.
        let grid = line_with_gap(&[12], true);
        let swarmers = FlowField::new(&grid, Class::Swarmer, [target]);
        let brutes = FlowField::new(&grid, Class::Brute, [target]);
        assert!(swarmers.distance(start).is_finite());
        assert!(brutes.distance(start).is_infinite());
        // A 2-cell gap lets them through.
        let grid = line_with_gap(&[12, 13], true);
        let brutes = FlowField::new(&grid, Class::Brute, [target]);
        let route = brutes.route(start);
        assert!(brutes.distance(start).is_finite());
        assert!(route.contains(&IVec2::new(12, 7)), "{route:?}");
        assert!(route.iter().all(|&node| brutes.is_clear(node)));
    }

    #[test]
    fn brutes_widen_a_gap_by_breaking_the_wall_beside_it() {
        // Walls with a 1-cell gap at x = 13. Swarmers walk through it; brutes break one wall next
        // to it, which is cheaper than breaking two walls anywhere else.
        let grid = line_with_gap(&[13], false);
        let target = IVec2::new(12, 2);
        let start = IVec2::new(12, 13);
        let swarmers = FlowField::new(&grid, Class::Swarmer, [target]);
        assert!(swarmers.breaches(start, &grid).is_empty());
        let brutes = FlowField::new(&grid, Class::Brute, [target]);
        let breaches = brutes.breaches(brutes.node_at(cell_middle(start)), &grid);
        assert_eq!(breaches.len(), 1, "{breaches:?}");
        assert!([12, 14].contains(&breaches[0].x) && breaches[0].y == 8);
        // Crossing a solid stretch of wall costs about two walls' worth.
        let solid = line_with_gap(&[], false);
        let brutes_solid = FlowField::new(&solid, Class::Brute, [target]);
        let node = brutes.node_at(cell_middle(start));
        let saved = brutes_solid.distance(node) - brutes.distance(node);
        let wall = Class::Brute.wall_cost(WALL_HP) - 1.0;
        assert!(
            (saved - wall).abs() < 0.2 * wall,
            "saved {saved}, a wall is {wall}"
        );
        assert_eq!(brutes_solid.breaches(node, &solid).len(), 2);
    }

    fn cell_middle(cell: IVec2) -> Vec2 {
        cell.as_vec2() + 0.5
    }

    #[test]
    fn brute_nodes_are_2x2_blocks() {
        let field = FlowField::new(&open_ground(), Class::Brute, [IVec2::new(8, 8)]);
        // A node's middle is the corner its four cells share.
        let node = IVec2::new(3, 5);
        assert_eq!(field.node_center(node), Vec2::new(4.0, 6.0));
        assert_eq!(field.node_at(Vec2::new(4.0, 6.0)), node);
        assert_eq!(field.node_at(Vec2::new(4.4, 5.6)), node);
        assert_eq!(field.node_at(Vec2::new(4.6, 6.0)), node + IVec2::X);
        let cells: Vec<IVec2> = field.cells(node).collect();
        assert_eq!(cells, [(3, 5), (4, 5), (3, 6), (4, 6)].map(IVec2::from));
        // Every node with the target cell in it is a target.
        for node in [(7, 7), (8, 7), (7, 8), (8, 8)] {
            assert_eq!(field.distance(IVec2::from(node)), 0.0);
        }
        // Blocks sticking out over the map's edge don't exist.
        assert!(field.distance(IVec2::new(15, 3)).is_infinite());
        assert!(field.distance(IVec2::new(14, 3)).is_finite());
        assert_eq!(field.area(), Rect::new(0.0, 0.0, 16.0, 16.0));
        // Swarmer nodes are cells.
        let swarmers = FlowField::new(&open_ground(), Class::Swarmer, [IVec2::new(8, 8)]);
        assert_eq!(swarmers.node_at(Vec2::new(3.9, 5.1)), IVec2::new(3, 5));
        assert_eq!(swarmers.node_center(IVec2::new(3, 5)), Vec2::new(3.5, 5.5));
        assert_eq!(swarmers.distance_through(IVec2::new(8, 8)), 0.0);
    }

    #[test]
    fn rock_is_impassable() {
        let mut grid = open_ground();
        // Rock around the corner cell (0, 0) cuts it off.
        for cell in [(1, 0), (1, 1), (0, 1)] {
            grid.set_terrain(IVec2::from(cell), Terrain::Rock);
        }
        let field = FlowField::new(&grid, Class::Swarmer, [IVec2::new(8, 8)]);
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
        let field = FlowField::new(&grid, Class::Swarmer, [IVec2::new(12, 12)]);
        assert!(field.distance(IVec2::new(2, 2)).is_infinite());
        assert!(field.distance(IVec2::new(4, 4)).is_finite());
    }

    #[test]
    fn routes_go_downhill_to_the_target() {
        let mut grid = open_ground();
        wall(&mut grid, (2..12).map(|y| (6, y)));
        let target = IVec2::new(10, 5);
        let field = FlowField::new(&grid, Class::Swarmer, [target]);
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
        let field = FlowField::new(&open_ground(), Class::Swarmer, [target]);
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
        assert_eq!(wavefront(2.0, f32::INFINITY, 40.0), 42.0);
    }
}
