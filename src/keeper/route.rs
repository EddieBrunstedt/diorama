//! Planning the keeper's route through the queue, as plain data: where he walks for each order,
//! which orders he'll have to skip, how much wood he'll carry along the way, and how long each
//! order takes.

use super::nav::{Goal, NavGrid, path_length};
use super::queue::{Backpack, Order, OrderKind, Phase, Problem, plan_wood};
use crate::island::TreeId;
use crate::tuning::KEEPER_SPEED;
use crate::voxel::TILE_SIZE;
use bevy::prelude::*;
use std::collections::HashMap;

/// How close to a tree the keeper stands to chop it.
const CHOP_STAND_OFF: f32 = 0.2;
const CHOP_REACH: f32 = 0.3;
/// How close to a tower's middle the keeper stands to build, repair or man it (just outside its
/// footprint).
const BUILD_STAND_OFF: f32 = 0.47;
const BUILD_REACH: f32 = 0.45;
/// When a walk target isn't walkable (e.g. the hearth), get this close to it instead.
const WALK_NEAR_REACH: f32 = 0.5;

/// Looks up where a standing tree is (world x/z). `None` if it's gone.
pub trait TreeLookup {
    fn tree_position(&self, id: TreeId) -> Option<Vec2>;
}

impl TreeLookup for HashMap<TreeId, Vec2> {
    fn tree_position(&self, id: TreeId) -> Option<Vec2> {
        self.get(&id).copied()
    }
}

/// Where an order happens (world x/z).
pub fn target_point(kind: &OrderKind, trees: &impl TreeLookup) -> Option<Vec2> {
    match *kind {
        OrderKind::Chop(id) => trees.tree_position(id),
        OrderKind::Build(cell, _) | OrderKind::Repair(cell) | OrderKind::Man(cell) => {
            Some(cell.as_vec2() * TILE_SIZE)
        }
        OrderKind::Walk(point) => Some(point),
    }
}

/// The path the keeper walks from `from` to carry out an order, ending where he works.
///
/// `nav` is updated with what changes while he's on his way: a build site blocks its footprint
/// from the moment he heads there.
pub fn path_for(
    kind: &OrderKind,
    from: Vec2,
    nav: &mut NavGrid,
    trees: &impl TreeLookup,
) -> Option<Vec<Vec2>> {
    let point = target_point(kind, trees)?;
    let goal = match *kind {
        OrderKind::Chop(_) => Goal::beside(point, CHOP_REACH, CHOP_STAND_OFF),
        OrderKind::Build(cell, _) => {
            nav.block_tower(cell);
            Goal::beside(point, BUILD_REACH, BUILD_STAND_OFF)
        }
        OrderKind::Repair(_) | OrderKind::Man(_) => {
            Goal::beside(point, BUILD_REACH, BUILD_STAND_OFF)
        }
        OrderKind::Walk(_) if nav.is_walkable(point) => Goal::at(point),
        OrderKind::Walk(_) => Goal::nearby(point, WALK_NEAR_REACH),
    };
    nav.find_path(from, &goal)
}

/// Seconds it takes the keeper to walk a path.
pub fn walk_seconds(path: &[Vec2]) -> f32 {
    path_length(path) / KEEPER_SPEED
}

/// Updates `nav` for an order that has been carried out. A chopped tree no longer blocks the way.
/// (A tower already blocks its footprint from the moment the keeper heads to build it.)
pub fn apply_done(kind: &OrderKind, nav: &mut NavGrid, trees: &impl TreeLookup) {
    if let OrderKind::Chop(id) = *kind
        && let Some(position) = trees.tree_position(id)
    {
        nav.unblock_point(position);
    }
}

/// One order in the plan.
#[derive(Clone, Debug, PartialEq)]
pub struct PlannedOrder {
    pub order: Order,
    /// Why the keeper will skip this order, if he will.
    pub problem: Option<Problem>,
    /// Wood in the backpack once this order is done.
    pub wood_after: u32,
    /// Where the order happens (world x/z). `None` if its target is gone.
    pub target: Option<Vec2>,
    /// The walk from the previous order to this one. Empty for skipped orders and for the started
    /// order (the keeper is already on his way).
    pub path: Vec<Vec2>,
    /// Seconds to walk `path`.
    pub walk_seconds: f32,
    /// Seconds of chopping or building once he's there. Zero for skipped orders and the started
    /// order.
    pub work_seconds: f32,
}

impl PlannedOrder {
    /// How long the order takes from the moment the keeper sets off, if he hasn't yet.
    /// The started order's time left is in [`super::timeline`]'s hands, since it changes as he works.
    pub fn seconds(&self) -> f32 {
        self.walk_seconds + self.work_seconds
    }
}

/// Where the keeper is in carrying out the queue's first order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Progress {
    /// Not started: he'll set off from `keeper` to the first order.
    NotStarted { keeper: Vec2 },
    /// The first order is started; he'll be at `end` when it's done.
    Started { end: Vec2 },
}

/// Plans the whole queue for `phase`: problems, wood and paths for every order. Orders that
/// belong to the other phase wait (see [`plan_wood`]).
///
/// After an open-ended order (manning the catapult) the keeper carries on from where he stood to
/// get in, once he's called out.
pub fn plan(
    orders: &[Order],
    progress: Progress,
    backpack: &Backpack,
    phase: Phase,
    nav: &NavGrid,
    trees: &impl TreeLookup,
) -> Vec<PlannedOrder> {
    let mut nav = nav.clone();
    let mut position = match progress {
        Progress::NotStarted { keeper } => keeper,
        Progress::Started { end } => end,
    };
    let started = match progress {
        Progress::Started { .. } => orders.first().map(|order| order.id),
        Progress::NotStarted { .. } => None,
    };
    let mut paths = HashMap::new();
    let steps = plan_wood(orders, backpack, phase, started, |order| {
        if Some(order.id) == started {
            apply_done(&order.kind, &mut nav, trees);
            return true;
        }
        let mut next_nav = nav.clone();
        let Some(path) = path_for(&order.kind, position, &mut next_nav, trees) else {
            return false;
        };
        apply_done(&order.kind, &mut next_nav, trees);
        nav = next_nav;
        position = *path.last().unwrap();
        paths.insert(order.id, path);
        true
    });
    orders
        .iter()
        .zip(steps)
        .map(|(order, step)| {
            let path = paths.remove(&order.id).unwrap_or_default();
            let work_seconds = if step.problem.is_none() && Some(order.id) != started {
                order.kind.work_seconds()
            } else {
                0.0
            };
            PlannedOrder {
                order: *order,
                problem: step.problem,
                wood_after: step.wood_after,
                target: target_point(&order.kind, trees),
                walk_seconds: walk_seconds(&path),
                work_seconds,
                path,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::island::Island;
    use crate::keeper::queue::OrderQueue;
    use crate::tuning::{BUILD_SECONDS, CHOP_SECONDS, REPAIR_SECONDS};

    const TREE: TreeId = TreeId {
        cell: IVec2::new(-2, -2),
        index: 0,
    };
    const TREE_AT: Vec2 = Vec2::new(-2.2, -2.2);
    const START: Vec2 = Vec2::new(-0.6, 0.2);

    fn setup() -> (NavGrid, HashMap<TreeId, Vec2>) {
        let mut nav = NavGrid::new(&Island::starting());
        nav.block_point(TREE_AT);
        (nav, HashMap::from([(TREE, TREE_AT)]))
    }

    fn queue(kinds: &[OrderKind]) -> OrderQueue {
        let mut queue = OrderQueue::default();
        for &kind in kinds {
            queue.push(kind);
        }
        queue
    }

    fn backpack(wood: u32) -> Backpack {
        Backpack { wood, capacity: 5 }
    }

    #[test]
    fn each_leg_starts_where_the_previous_one_ended() {
        let (nav, trees) = setup();
        let queue = queue(&[
            OrderKind::Chop(TREE),
            OrderKind::tower(IVec2::new(-1, 1)),
            OrderKind::Walk(Vec2::new(2.0, 2.0)),
        ]);
        let planned = plan(
            queue.orders(),
            Progress::NotStarted { keeper: START },
            &backpack(2),
            Phase::Preparing,
            &nav,
            &trees,
        );
        assert!(planned.iter().all(|step| step.problem.is_none()));
        assert_eq!(planned[0].path[0], START);
        for pair in planned.windows(2) {
            assert_eq!(pair[0].path.last(), pair[1].path.first());
        }
        let chop_end = *planned[0].path.last().unwrap();
        assert!((chop_end.distance(TREE_AT) - CHOP_STAND_OFF).abs() < 1e-4);
        assert_eq!(planned[1].wood_after, 0);
        assert_eq!(*planned[2].path.last().unwrap(), Vec2::new(2.0, 2.0));
    }

    #[test]
    fn later_legs_walk_around_a_planned_tower() {
        let (nav, trees) = setup();
        let site = IVec2::new(-1, 1);
        let through = [
            OrderKind::Walk(Vec2::new(-1.4, 1.0)),
            OrderKind::Walk(Vec2::new(-0.6, 1.0)),
        ];
        let straight = plan(
            queue(&through).orders(),
            Progress::NotStarted { keeper: START },
            &backpack(3),
            Phase::Preparing,
            &nav,
            &trees,
        );
        let with_tower = plan(
            queue(&[through[0], OrderKind::tower(site), through[1]]).orders(),
            Progress::NotStarted { keeper: START },
            &backpack(3),
            Phase::Preparing,
            &nav,
            &trees,
        );
        let mut blocked = nav.clone();
        blocked.block_tower(site);
        let last = &with_tower[2].path;
        assert!(
            last.windows(2)
                .all(|pair| blocked.line_of_sight(pair[0], pair[1]))
        );
        assert!(path_length(&straight[1].path) < 0.81);
        assert!(
            path_length(last) > 1.0,
            "detours around the tower: {last:?}"
        );
    }

    #[test]
    fn skipped_orders_have_no_path_and_dont_move_the_keeper() {
        let (nav, trees) = setup();
        let queue = queue(&[
            OrderKind::tower(IVec2::new(-1, 1)),
            OrderKind::Walk(Vec2::new(2.0, 2.0)),
        ]);
        let planned = plan(
            queue.orders(),
            Progress::NotStarted { keeper: START },
            &backpack(0),
            Phase::Preparing,
            &nav,
            &trees,
        );
        assert_eq!(planned[0].problem, Some(Problem::NotEnoughWood));
        assert!(planned[0].path.is_empty());
        assert_eq!(planned[1].path[0], START);
    }

    #[test]
    fn unreachable_orders_are_flagged() {
        let (nav, trees) = setup();
        let gone = TreeId {
            cell: IVec2::new(-2, -2),
            index: 1,
        };
        let queue = queue(&[OrderKind::Chop(gone), OrderKind::Chop(TREE)]);
        let planned = plan(
            queue.orders(),
            Progress::NotStarted { keeper: START },
            &backpack(0),
            Phase::Preparing,
            &nav,
            &trees,
        );
        assert_eq!(planned[0].problem, Some(Problem::Unreachable));
        assert_eq!(planned[0].target, None);
        assert_eq!(planned[1].wood_after, 1);
    }

    #[test]
    fn started_order_continues_from_its_end() {
        let (nav, trees) = setup();
        let end = Vec2::new(-2.0, -1.8);
        let queue = queue(&[OrderKind::Chop(TREE), OrderKind::Walk(Vec2::new(0.0, 2.0))]);
        let planned = plan(
            queue.orders(),
            Progress::Started { end },
            &backpack(0),
            Phase::Preparing,
            &nav,
            &trees,
        );
        assert!(planned[0].path.is_empty());
        assert_eq!(planned[0].wood_after, 1);
        assert_eq!(planned[1].path[0], end);
    }

    #[test]
    fn walking_onto_the_hearth_stops_beside_it() {
        let (nav, trees) = setup();
        let queue = queue(&[OrderKind::Walk(Vec2::ZERO)]);
        let planned = plan(
            queue.orders(),
            Progress::NotStarted {
                keeper: Vec2::new(-2.0, 2.0),
            },
            &backpack(0),
            Phase::Preparing,
            &nav,
            &trees,
        );
        let end = *planned[0].path.last().unwrap();
        assert!(nav.is_walkable(end));
        assert!(end.length() <= WALK_NEAR_REACH);
    }

    #[test]
    fn orders_take_walking_plus_working_time() {
        let (nav, trees) = setup();
        let walk_to = Vec2::new(-0.6, 1.4);
        let queue = queue(&[
            OrderKind::Walk(walk_to),
            OrderKind::Chop(TREE),
            OrderKind::tower(IVec2::new(-1, 1)),
        ]);
        let planned = plan(
            queue.orders(),
            Progress::NotStarted { keeper: START },
            &backpack(3),
            Phase::Preparing,
            &nav,
            &trees,
        );
        // Open ground: a straight walk.
        let walk = START.distance(walk_to) / KEEPER_SPEED;
        assert!((planned[0].seconds() - walk).abs() < 1e-4);
        assert_eq!(planned[0].work_seconds, 0.0);
        assert_eq!(planned[1].work_seconds, CHOP_SECONDS);
        assert_eq!(planned[2].work_seconds, BUILD_SECONDS);
        for step in &planned {
            assert!((step.walk_seconds - path_length(&step.path) / KEEPER_SPEED).abs() < 1e-4);
            assert!(step.walk_seconds > 0.0);
        }
    }

    #[test]
    fn during_the_wave_the_route_skips_waiting_builds() {
        let (nav, trees) = setup();
        let walk_to = Vec2::new(2.0, 2.0);
        let queue = queue(&[
            OrderKind::tower(IVec2::new(-1, 1)),
            OrderKind::Walk(walk_to),
        ]);
        let planned = plan(
            queue.orders(),
            Progress::NotStarted { keeper: START },
            &backpack(3),
            Phase::Wave,
            &nav,
            &trees,
        );
        assert_eq!(planned[0].problem, Some(Problem::WaveOn));
        assert_eq!(planned[0].seconds(), 0.0);
        assert_eq!(planned[0].wood_after, 3);
        assert_eq!(planned[1].path.first(), Some(&START));
        assert_eq!(planned[1].path.last(), Some(&walk_to));
    }

    #[test]
    fn repairing_and_manning_stop_beside_the_structure() {
        let (mut nav, trees) = setup();
        let cell = IVec2::new(-1, 1);
        nav.block_tower(cell);
        let queue = queue(&[
            OrderKind::Repair(cell),
            OrderKind::Man(cell),
            OrderKind::Walk(Vec2::new(2.0, 2.0)),
        ]);
        let planned = plan(
            queue.orders(),
            Progress::NotStarted { keeper: START },
            &backpack(0),
            Phase::Wave,
            &nav,
            &trees,
        );
        for step in &planned[..2] {
            assert!(step.problem.is_none());
            let end = *step.path.last().unwrap();
            assert!((end.distance(cell.as_vec2()) - BUILD_STAND_OFF).abs() < 1e-4);
        }
        assert_eq!(planned[0].work_seconds, REPAIR_SECONDS);
        assert_eq!(
            planned[1].work_seconds, 0.0,
            "manning lasts until he's called out"
        );
        assert_eq!(planned[2].path.first(), planned[1].path.last());
    }

    #[test]
    fn skipped_and_started_orders_take_no_planned_time() {
        let (nav, trees) = setup();
        let queue = queue(&[OrderKind::Chop(TREE), OrderKind::tower(IVec2::new(-1, 1))]);
        let planned = plan(
            queue.orders(),
            Progress::Started { end: START },
            &backpack(0),
            Phase::Preparing,
            &nav,
            &trees,
        );
        assert_eq!(
            planned[0].seconds(),
            0.0,
            "started: its time left is tracked live"
        );
        assert_eq!(planned[1].problem, Some(Problem::NotEnoughWood));
        assert_eq!(planned[1].seconds(), 0.0);
    }
}
