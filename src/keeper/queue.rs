//! The little guy's order queue and backpack, as plain data.
//!
//! The first order may be *started*: the keeper is carrying it out, so it stays at the front until
//! it's done or removed. Everything behind it can be reordered freely.
//!
//! [`plan_wood`] walks through the queue the way the keeper will, tracking the wood in the backpack
//! and flagging orders that won't be possible when their turn comes. The keeper skips flagged orders
//! when he gets to them, so the plan and what actually happens always agree.
//!
//! Some orders belong to one phase and wait out the other (see [`OrderKind::hold`]): builds can't
//! be started during a wave, and the catapult is only manned during one. A waiting order stays
//! queued and the keeper goes on with the orders behind it, so it's first in line when its phase
//! comes.

use crate::island::TreeId;
use crate::tuning::{
    BACKPACK_CAPACITY, BUILD_SECONDS, CATAPULT_BUILD_SECONDS, CATAPULT_WOOD_COST, CHOP_SECONDS,
    MAX_ORDERS, REPAIR_SECONDS, STARTING_WOOD, TOWER_WOOD_COST, WOOD_PER_TREE,
};
use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OrderId(u32);

/// What the keeper can build on a free meadow.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Building {
    /// Shoots slimes by itself.
    #[default]
    Tower,
    /// Throws boulders, but only while the keeper mans it. One per island.
    Catapult,
}

impl Building {
    pub fn wood_cost(self) -> u32 {
        match self {
            Building::Tower => TOWER_WOOD_COST,
            Building::Catapult => CATAPULT_WOOD_COST,
        }
    }

    pub fn build_seconds(self) -> f32 {
        match self {
            Building::Tower => BUILD_SECONDS,
            Building::Catapult => CATAPULT_BUILD_SECONDS,
        }
    }
}

/// Which part of the game the orders are planned for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Phase {
    /// Getting ready for the next wave: builds can start, the catapult waits.
    Preparing,
    /// Slimes are coming: builds wait, the catapult can be manned.
    Wave,
    /// The wave's last slime is gone, but the next phase hasn't come yet: builds still wait, and
    /// there's no getting into the catapult any more.
    WaveOver,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OrderKind {
    Chop(TreeId),
    /// Build on this (meadow) cell.
    Build(IVec2, Building),
    /// Repair the tower on this cell.
    Repair(IVec2),
    /// Get into the catapult on this cell and work it until called out.
    Man(IVec2),
    /// Walk to this point on the island (world x/z).
    Walk(Vec2),
}

impl OrderKind {
    /// A tower build on `cell`.
    pub fn tower(cell: IVec2) -> Self {
        OrderKind::Build(cell, Building::Tower)
    }

    /// Whether two orders are about the same thing, so the second one would be pointless.
    /// Walking somewhere twice is allowed.
    pub fn same_target(&self, other: &OrderKind) -> bool {
        match (self, other) {
            (OrderKind::Chop(a), OrderKind::Chop(b)) => a == b,
            (OrderKind::Build(a, _), OrderKind::Build(b, _)) => a == b,
            (OrderKind::Repair(a), OrderKind::Repair(b)) => a == b,
            (OrderKind::Man(a), OrderKind::Man(b)) => a == b,
            _ => false,
        }
    }

    /// Change in backpack wood when this order is carried out.
    pub fn wood_change(&self) -> i32 {
        match self {
            OrderKind::Chop(_) => WOOD_PER_TREE as i32,
            OrderKind::Build(_, building) => -(building.wood_cost() as i32),
            OrderKind::Repair(_) | OrderKind::Man(_) | OrderKind::Walk(_) => 0,
        }
    }

    /// Seconds of work once the keeper has walked there. Manning the catapult lasts until he's
    /// called out (see [`Self::open_ended`]), so it has no set time.
    pub fn work_seconds(&self) -> f32 {
        match self {
            OrderKind::Chop(_) => CHOP_SECONDS,
            OrderKind::Build(_, building) => building.build_seconds(),
            OrderKind::Repair(_) => REPAIR_SECONDS,
            OrderKind::Man(_) | OrderKind::Walk(_) => 0.0,
        }
    }

    /// Lasts until the keeper is called out: the orders behind it wait until then.
    pub fn open_ended(&self) -> bool {
        matches!(self, OrderKind::Man(_))
    }

    /// Why the order has to wait in `phase`, if it does: no new builds during the wave, and no
    /// manning the catapult outside it.
    pub fn hold(&self, phase: Phase) -> Option<Problem> {
        match (self, phase) {
            (OrderKind::Build(..), Phase::Wave | Phase::WaveOver) => Some(Problem::WaveOn),
            (OrderKind::Man(_), Phase::Preparing | Phase::WaveOver) => Some(Problem::NoWave),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Order {
    pub id: OrderId,
    pub kind: OrderKind,
}

/// Why an order can't be carried out when its turn comes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Problem {
    NotEnoughWood,
    BackpackFull,
    Unreachable,
    /// A build during the wave. Unlike the other problems, the order isn't skipped: it waits in
    /// the queue for the next prepare phase.
    WaveOn,
    /// Manning the catapult outside the wave. It waits in the queue for the wave.
    NoWave,
}

impl Problem {
    pub fn message(self) -> &'static str {
        match self {
            Problem::NotEnoughWood => "Not enough wood",
            Problem::BackpackFull => "Backpack full",
            Problem::Unreachable => "Can't get there",
            Problem::WaveOn => "Waits until the wave is over",
            Problem::NoWave => "Waits for the wave",
        }
    }

    /// Whether the order stays queued for later instead of being skipped.
    pub fn holds(self) -> bool {
        matches!(self, Problem::WaveOn | Problem::NoWave)
    }
}

#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct Backpack {
    pub wood: u32,
    pub capacity: u32,
}

impl Default for Backpack {
    fn default() -> Self {
        Self {
            wood: STARTING_WOOD,
            capacity: BACKPACK_CAPACITY,
        }
    }
}

impl Backpack {
    /// The problem `kind` would run into with this much wood, if any.
    pub fn problem(&self, kind: &OrderKind) -> Option<Problem> {
        let change = kind.wood_change();
        if change < 0 && self.wood < change.unsigned_abs() {
            Some(Problem::NotEnoughWood)
        } else if change > 0 && self.wood >= self.capacity {
            Some(Problem::BackpackFull)
        } else {
            None
        }
    }

    /// Applies an order's wood change. Wood beyond the capacity is lost.
    pub fn apply(&mut self, kind: &OrderKind) {
        let wood = self.wood as i32 + kind.wood_change();
        self.wood = (wood.max(0) as u32).min(self.capacity);
    }
}

#[derive(Resource, Clone, Debug, Default)]
pub struct OrderQueue {
    orders: Vec<Order>,
    /// Whether the first order is being carried out.
    started: bool,
    next_id: u32,
}

impl OrderQueue {
    pub fn orders(&self) -> &[Order] {
        &self.orders
    }

    pub fn is_empty(&self) -> bool {
        self.orders.is_empty()
    }

    pub fn is_full(&self) -> bool {
        self.orders.len() >= MAX_ORDERS
    }

    /// The order being carried out, if any.
    pub fn started(&self) -> Option<&Order> {
        self.orders.first().filter(|_| self.started)
    }

    /// The order the keeper takes up next: the first one that doesn't have to wait in `phase`.
    pub fn next_up(&self, phase: Phase) -> Option<&Order> {
        self.orders
            .iter()
            .find(|order| order.kind.hold(phase).is_none())
    }

    /// The first index an order can be moved or inserted to (behind the started order).
    pub fn first_free_index(&self) -> usize {
        usize::from(self.started)
    }

    pub fn index_of(&self, id: OrderId) -> Option<usize> {
        self.orders.iter().position(|order| order.id == id)
    }

    /// An order already queued for the same target as `kind`.
    pub fn find_same_target(&self, kind: &OrderKind) -> Option<OrderId> {
        self.orders
            .iter()
            .find(|order| order.kind.same_target(kind))
            .map(|order| order.id)
    }

    /// Adds an order at the end. `None` if the queue is full.
    pub fn push(&mut self, kind: OrderKind) -> Option<OrderId> {
        self.insert(self.orders.len(), kind)
    }

    /// Adds an order to be done next, right behind the started order. `None` if the queue is full.
    pub fn push_front(&mut self, kind: OrderKind) -> Option<OrderId> {
        self.insert(self.first_free_index(), kind)
    }

    /// Where a new order would go: next (right behind the started order) or at the end.
    pub fn insert_index(&self, next: bool) -> usize {
        if next {
            self.first_free_index()
        } else {
            self.orders.len()
        }
    }

    /// The orders as they'd be with `kind` added (see [`Self::insert_index`]), for previewing an
    /// order before it's given. Returns the orders and the new one's index.
    pub fn with_candidate(&self, kind: OrderKind, next: bool) -> (Vec<Order>, usize) {
        let index = self.insert_index(next);
        let mut orders = self.orders.clone();
        let id = OrderId(self.next_id);
        orders.insert(index, Order { id, kind });
        (orders, index)
    }

    fn insert(&mut self, index: usize, kind: OrderKind) -> Option<OrderId> {
        if self.is_full() {
            return None;
        }
        let id = OrderId(self.next_id);
        self.next_id += 1;
        self.orders.insert(index, Order { id, kind });
        Some(id)
    }

    /// Removes an order. Removing the started order cancels it.
    pub fn remove(&mut self, id: OrderId) -> Option<Order> {
        let index = self.index_of(id)?;
        if index == 0 {
            self.started = false;
        }
        Some(self.orders.remove(index))
    }

    /// Moves an order to `index` (clamped so it stays behind the started order).
    /// The started order itself can't be moved. Returns whether anything changed.
    pub fn move_to(&mut self, id: OrderId, index: usize) -> bool {
        let Some(from) = self.index_of(id) else {
            return false;
        };
        let first = self.first_free_index();
        if from < first {
            return false;
        }
        let to = index.clamp(first, self.orders.len() - 1);
        if from == to {
            return false;
        }
        let order = self.orders.remove(from);
        self.orders.insert(to, order);
        true
    }

    /// Marks an order as being carried out and returns it. It moves to the front, ahead of any
    /// orders waiting for their phase.
    pub fn start(&mut self, id: OrderId) -> Option<Order> {
        let order = self.orders.remove(self.index_of(id)?);
        self.orders.insert(0, order);
        self.started = true;
        Some(order)
    }

    /// Removes every order. Ids stay unique, so nothing mistakes a new order for an old one.
    pub fn clear(&mut self) {
        self.orders.clear();
        self.started = false;
    }

    /// Removes the started order because it's done.
    pub fn finish(&mut self) -> Option<Order> {
        if !self.started {
            return None;
        }
        self.started = false;
        Some(self.orders.remove(0))
    }
}

/// The planned state after one order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WoodStep {
    /// Why the order will be skipped, if it will.
    pub problem: Option<Problem>,
    /// Wood in the backpack after this order.
    pub wood_after: u32,
}

/// Plans the queue's wood: which orders will run short of wood or overfill the backpack, and how
/// much wood there is after each one.
///
/// Orders that can't be started in `phase` wait (see [`OrderKind::hold`]), except the `started`
/// one, which the keeper finishes. Waiting orders don't change the wood.
///
/// `reachable` is asked about each order that has enough wood, in queue order, so route planning can
/// run alongside. An unreachable order is skipped too, so it doesn't change the wood.
pub fn plan_wood(
    orders: &[Order],
    backpack: &Backpack,
    phase: Phase,
    started: Option<OrderId>,
    mut reachable: impl FnMut(&Order) -> bool,
) -> Vec<WoodStep> {
    let mut backpack = backpack.clone();
    orders
        .iter()
        .map(|order| {
            let hold = order.kind.hold(phase).filter(|_| Some(order.id) != started);
            let problem = hold
                .or_else(|| backpack.problem(&order.kind))
                .or_else(|| (!reachable(order)).then_some(Problem::Unreachable));
            if problem.is_none() {
                backpack.apply(&order.kind);
            }
            WoodStep {
                problem,
                wood_after: backpack.wood,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(index: u8) -> OrderKind {
        OrderKind::Chop(TreeId {
            cell: IVec2::new(-2, -2),
            index,
        })
    }

    fn build() -> OrderKind {
        OrderKind::tower(IVec2::new(1, 1))
    }

    fn man() -> OrderKind {
        OrderKind::Man(IVec2::new(-1, 1))
    }

    fn kinds(queue: &OrderQueue) -> Vec<OrderKind> {
        queue.orders().iter().map(|order| order.kind).collect()
    }

    fn backpack(wood: u32) -> Backpack {
        Backpack { wood, capacity: 5 }
    }

    fn problems(steps: &[WoodStep]) -> Vec<Option<Problem>> {
        steps.iter().map(|step| step.problem).collect()
    }

    #[test]
    fn push_appends_and_push_front_inserts_first() {
        let mut queue = OrderQueue::default();
        queue.push(tree(0));
        queue.push(tree(1));
        queue.push_front(build());
        assert_eq!(kinds(&queue), vec![build(), tree(0), tree(1)]);
    }

    #[test]
    fn push_front_goes_behind_the_started_order() {
        let mut queue = OrderQueue::default();
        let a = queue.push(tree(0)).unwrap();
        queue.start(a);
        queue.push_front(build());
        assert_eq!(kinds(&queue), vec![tree(0), build()]);
    }

    #[test]
    fn ids_are_unique() {
        let mut queue = OrderQueue::default();
        let a = queue.push(tree(0)).unwrap();
        queue.remove(a);
        let b = queue.push(tree(0)).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn queue_has_a_limit() {
        let mut queue = OrderQueue::default();
        for _ in 0..MAX_ORDERS {
            assert!(queue.push(OrderKind::Walk(Vec2::ZERO)).is_some());
        }
        assert!(queue.is_full());
        assert_eq!(queue.push(OrderKind::Walk(Vec2::ZERO)), None);
        assert_eq!(queue.push_front(OrderKind::Walk(Vec2::ZERO)), None);
    }

    #[test]
    fn candidates_go_where_a_click_would_put_them() {
        let mut queue = OrderQueue::default();
        let a = queue.push(tree(0)).unwrap();
        queue.push(tree(1));
        queue.start(a);
        let (orders, index) = queue.with_candidate(build(), true);
        assert_eq!(index, 1);
        assert_eq!(orders[1].kind, build());
        let (orders, index) = queue.with_candidate(build(), false);
        assert_eq!((orders.len(), index), (3, 2));
        assert_eq!(queue.orders().len(), 2, "the queue itself is unchanged");
        let candidate = orders[index].id;
        assert!(queue.orders().iter().all(|order| order.id != candidate));
    }

    #[test]
    fn move_to_reorders() {
        let mut queue = OrderQueue::default();
        let a = queue.push(tree(0)).unwrap();
        queue.push(tree(1));
        queue.push(tree(2));
        assert!(queue.move_to(a, 2));
        assert_eq!(kinds(&queue), vec![tree(1), tree(2), tree(0)]);
        assert!(queue.move_to(a, 0));
        assert_eq!(kinds(&queue), vec![tree(0), tree(1), tree(2)]);
        assert!(
            !queue.move_to(a, 0),
            "moving to the same place changes nothing"
        );
        assert!(queue.move_to(a, 99), "indices past the end are clamped");
        assert_eq!(kinds(&queue), vec![tree(1), tree(2), tree(0)]);
    }

    #[test]
    fn started_order_stays_in_front() {
        let mut queue = OrderQueue::default();
        let a = queue.push(tree(0)).unwrap();
        let b = queue.push(tree(1)).unwrap();
        queue.start(a);
        assert!(!queue.move_to(a, 1), "the started order can't move");
        assert!(!queue.move_to(b, 0), "nothing can move in front of it");
        assert_eq!(kinds(&queue), vec![tree(0), tree(1)]);
    }

    #[test]
    fn start_and_finish() {
        let mut queue = OrderQueue::default();
        let a = queue.push(tree(0)).unwrap();
        queue.push(tree(1));
        assert_eq!(queue.started(), None);
        assert_eq!(queue.finish(), None, "only a started order can finish");
        let started = queue.start(a).unwrap();
        assert_eq!(queue.started(), Some(&started));
        assert_eq!(queue.finish(), Some(started));
        assert_eq!(kinds(&queue), vec![tree(1)]);
        assert_eq!(queue.started(), None);
    }

    #[test]
    fn removing_the_started_order_cancels_it() {
        let mut queue = OrderQueue::default();
        let a = queue.push(tree(0)).unwrap();
        queue.push(tree(1));
        queue.start(a);
        queue.remove(a);
        assert_eq!(queue.started(), None);
        assert_eq!(queue.first_free_index(), 0);
    }

    #[test]
    fn finds_orders_for_the_same_target() {
        let mut queue = OrderQueue::default();
        let a = queue.push(tree(0)).unwrap();
        let b = queue.push(build()).unwrap();
        queue.push(OrderKind::Walk(Vec2::ONE));
        assert_eq!(queue.find_same_target(&tree(0)), Some(a));
        assert_eq!(queue.find_same_target(&tree(1)), None);
        assert_eq!(queue.find_same_target(&build()), Some(b));
        assert_eq!(queue.find_same_target(&OrderKind::Walk(Vec2::ONE)), None);
    }

    #[test]
    fn wood_is_tracked_through_the_queue() {
        let orders = [tree(0), tree(1), tree(2), build(), tree(3)];
        let mut queue = OrderQueue::default();
        for kind in orders {
            queue.push(kind);
        }
        let steps = plan_wood(queue.orders(), &backpack(0), Phase::Preparing, None, |_| {
            true
        });
        let wood: Vec<u32> = steps.iter().map(|step| step.wood_after).collect();
        assert_eq!(wood, vec![1, 2, 3, 0, 1]);
        assert!(steps.iter().all(|step| step.problem.is_none()));
    }

    #[test]
    fn build_without_enough_wood_is_flagged_and_skipped() {
        let mut queue = OrderQueue::default();
        queue.push(tree(0));
        queue.push(build());
        queue.push(tree(1));
        queue.push(tree(2));
        queue.push(build());
        let steps = plan_wood(queue.orders(), &backpack(1), Phase::Preparing, None, |_| {
            true
        });
        // 1 + 1 = 2 wood isn't enough for the first build, so it's skipped and keeps the wood,
        // which makes the second build possible.
        assert_eq!(
            problems(&steps),
            vec![None, Some(Problem::NotEnoughWood), None, None, None]
        );
        let wood: Vec<u32> = steps.iter().map(|step| step.wood_after).collect();
        assert_eq!(wood, vec![2, 2, 3, 4, 1]);
    }

    #[test]
    fn reordering_fixes_a_flagged_build() {
        let mut queue = OrderQueue::default();
        let first_build = queue.push(build()).unwrap();
        for index in 0..3 {
            queue.push(tree(index));
        }
        let steps = plan_wood(queue.orders(), &backpack(0), Phase::Preparing, None, |_| {
            true
        });
        assert_eq!(steps[0].problem, Some(Problem::NotEnoughWood));

        queue.move_to(first_build, 3);
        let steps = plan_wood(queue.orders(), &backpack(0), Phase::Preparing, None, |_| {
            true
        });
        assert!(steps.iter().all(|step| step.problem.is_none()));
        assert_eq!(steps[3].wood_after, 0);
    }

    #[test]
    fn chopping_into_a_full_backpack_is_flagged() {
        let mut queue = OrderQueue::default();
        queue.push(tree(0));
        queue.push(tree(1));
        let steps = plan_wood(queue.orders(), &backpack(4), Phase::Preparing, None, |_| {
            true
        });
        assert_eq!(problems(&steps), vec![None, Some(Problem::BackpackFull)]);
        assert_eq!(steps[1].wood_after, 5);
    }

    #[test]
    fn unreachable_orders_are_skipped() {
        let mut queue = OrderQueue::default();
        let blocked = queue.push(tree(0)).unwrap();
        queue.push(tree(1));
        let steps = plan_wood(
            queue.orders(),
            &backpack(0),
            Phase::Preparing,
            None,
            |order| order.id != blocked,
        );
        assert_eq!(problems(&steps), vec![Some(Problem::Unreachable), None]);
        assert_eq!(steps[0].wood_after, 0);
        assert_eq!(steps[1].wood_after, 1);
    }

    #[test]
    fn orders_short_of_wood_are_not_route_planned() {
        let mut queue = OrderQueue::default();
        queue.push(build());
        queue.push(OrderKind::Walk(Vec2::ZERO));
        let mut asked = Vec::new();
        plan_wood(
            queue.orders(),
            &backpack(0),
            Phase::Preparing,
            None,
            |order| {
                asked.push(order.kind);
                true
            },
        );
        assert_eq!(asked, vec![OrderKind::Walk(Vec2::ZERO)]);
    }

    #[test]
    fn builds_wait_during_the_wave_without_using_wood() {
        let mut queue = OrderQueue::default();
        queue.push(build());
        queue.push(tree(0));
        queue.push(tree(1));
        let steps = plan_wood(queue.orders(), &backpack(3), Phase::Wave, None, |_| true);
        assert_eq!(problems(&steps), vec![Some(Problem::WaveOn), None, None]);
        let wood: Vec<u32> = steps.iter().map(|step| step.wood_after).collect();
        assert_eq!(wood, vec![3, 4, 5]);
        assert!(Problem::WaveOn.holds());
        assert!(!Problem::NotEnoughWood.holds());
    }

    #[test]
    fn a_build_started_before_the_wave_is_finished() {
        let mut queue = OrderQueue::default();
        let started = queue.push(build()).unwrap();
        queue.start(started);
        queue.push(build());
        let steps = plan_wood(
            queue.orders(),
            &backpack(6),
            Phase::Wave,
            Some(started),
            |_| true,
        );
        assert_eq!(problems(&steps), vec![None, Some(Problem::WaveOn)]);
    }

    #[test]
    fn during_the_wave_the_keeper_goes_on_past_waiting_builds() {
        let mut queue = OrderQueue::default();
        queue.push(build());
        let chop = queue.push(tree(0)).unwrap();
        assert_eq!(
            queue.next_up(Phase::Preparing).map(|order| order.kind),
            Some(build())
        );
        assert_eq!(queue.next_up(Phase::Wave).map(|order| order.id), Some(chop));

        // Starting it moves it to the front, ahead of the waiting build.
        queue.start(chop);
        assert_eq!(kinds(&queue), vec![tree(0), build()]);
        assert_eq!(queue.started().map(|order| order.id), Some(chop));
        queue.finish();
        assert_eq!(
            queue.next_up(Phase::Wave),
            None,
            "only the waiting build is left"
        );
    }

    #[test]
    fn the_catapult_is_manned_only_during_the_wave() {
        let mut queue = OrderQueue::default();
        queue.push(man());
        let chop = queue.push(tree(0)).unwrap();
        let steps = plan_wood(queue.orders(), &backpack(0), Phase::Preparing, None, |_| {
            true
        });
        assert_eq!(problems(&steps), vec![Some(Problem::NoWave), None]);
        assert!(Problem::NoWave.holds());
        assert_eq!(queue.next_up(Phase::Preparing).map(|o| o.id), Some(chop));
        assert_eq!(queue.next_up(Phase::Wave).map(|o| o.kind), Some(man()));
        let steps = plan_wood(queue.orders(), &backpack(0), Phase::Wave, None, |_| true);
        assert!(steps.iter().all(|step| step.problem.is_none()));
        // Once the last slime is gone, it's too late to get in.
        assert_eq!(queue.next_up(Phase::WaveOver).map(|o| o.id), Some(chop));
        assert_eq!(man().hold(Phase::WaveOver), Some(Problem::NoWave));
        assert_eq!(build().hold(Phase::WaveOver), Some(Problem::WaveOn));
    }

    #[test]
    fn orders_on_the_same_structure_are_the_same() {
        let cell = IVec2::new(2, 0);
        assert!(OrderKind::tower(cell).same_target(&OrderKind::Build(cell, Building::Catapult)));
        assert!(OrderKind::Repair(cell).same_target(&OrderKind::Repair(cell)));
        assert!(!OrderKind::Repair(cell).same_target(&OrderKind::Man(cell)));
        assert!(OrderKind::Man(cell).same_target(&OrderKind::Man(cell)));
    }

    #[test]
    fn what_orders_cost_and_take() {
        let cell = IVec2::ZERO;
        assert_eq!(
            OrderKind::Build(cell, Building::Catapult).wood_change(),
            -(CATAPULT_WOOD_COST as i32)
        );
        assert_eq!(
            OrderKind::Build(cell, Building::Catapult).work_seconds(),
            CATAPULT_BUILD_SECONDS
        );
        assert_eq!(OrderKind::Repair(cell).wood_change(), 0);
        assert_eq!(OrderKind::Repair(cell).work_seconds(), REPAIR_SECONDS);
        assert!(OrderKind::Man(cell).open_ended());
        assert!(!OrderKind::Repair(cell).open_ended());
        assert_eq!(OrderKind::Repair(cell).hold(Phase::Wave), None);
        assert_eq!(OrderKind::Repair(cell).hold(Phase::Preparing), None);
    }

    #[test]
    fn clearing_keeps_ids_unique() {
        let mut queue = OrderQueue::default();
        let a = queue.push(tree(0)).unwrap();
        queue.start(a);
        queue.clear();
        assert!(queue.is_empty());
        assert_eq!(queue.started(), None);
        assert_ne!(queue.push(tree(0)), Some(a));
    }

    #[test]
    fn backpack_apply_clamps() {
        let mut full = backpack(5);
        full.apply(&tree(0));
        assert_eq!(full.wood, 5);
        let mut empty = backpack(1);
        empty.apply(&build());
        assert_eq!(empty.wood, 0);
    }
}
