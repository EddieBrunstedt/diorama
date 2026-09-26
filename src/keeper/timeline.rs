//! When each order in the plan will happen, as plain data: the orders laid end to end from now,
//! and which of them won't be done before the wave starts.
//!
//! The plan knows how long every order takes (walking there plus chopping or building). The order
//! in progress is the exception: its time left shrinks as the keeper works, so it's passed in
//! separately and the timeline is rebuilt every frame (it's only a running sum).

use super::route::PlannedOrder;

/// When one order happens, in seconds from now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slot {
    pub start: f32,
    pub end: f32,
    /// Won't be done before the wave starts.
    pub late: bool,
}

impl Slot {
    pub fn seconds(&self) -> f32 {
        self.end - self.start
    }
}

/// Lays the planned orders end to end, starting now.
///
/// `started_left` is the time left on the first order if the keeper is already carrying it out.
/// `deadline` is the time until the wave starts, if the prepare phase is on: orders that end after
/// it are late. Skipped orders take no time and are never late (they're flagged already).
pub fn schedule(
    orders: &[PlannedOrder],
    started_left: Option<f32>,
    deadline: Option<f32>,
) -> Vec<Slot> {
    let mut now = 0.0;
    orders
        .iter()
        .enumerate()
        .map(|(index, planned)| {
            let seconds = match started_left {
                Some(left) if index == 0 => left.max(0.0),
                _ => planned.seconds(),
            };
            let start = now;
            now += seconds;
            Slot {
                start,
                end: now,
                late: planned.problem.is_none() && deadline.is_some_and(|d| is_late(now, d)),
            }
        })
        .collect()
}

/// Whether something that's done `end` seconds from now misses a deadline `deadline` seconds away.
pub fn is_late(end: f32, deadline: f32) -> bool {
    // A few milliseconds of slack, so an order that exactly fills the time isn't flagged by
    // rounding.
    end > deadline + 1e-3
}

/// The first late order. Everything after it that isn't skipped is late too, since the orders
/// are done one after another.
pub fn first_late(slots: &[Slot]) -> Option<usize> {
    slots.iter().position(|slot| slot.late)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keeper::queue::{Order, OrderKind, OrderQueue, Problem};
    use bevy::math::Vec2;

    fn orders(count: usize) -> Vec<Order> {
        let mut queue = OrderQueue::default();
        for _ in 0..count {
            queue.push(OrderKind::Walk(Vec2::ZERO));
        }
        queue.orders().to_vec()
    }

    /// Planned orders taking these many seconds each (`None`: skipped).
    fn planned(seconds: &[Option<f32>]) -> Vec<PlannedOrder> {
        orders(seconds.len())
            .into_iter()
            .zip(seconds)
            .map(|(order, &seconds)| PlannedOrder {
                order,
                problem: seconds.is_none().then_some(Problem::NotEnoughWood),
                wood_after: 0,
                target: None,
                path: Vec::new(),
                walk_seconds: 0.5 * seconds.unwrap_or(0.0),
                work_seconds: 0.5 * seconds.unwrap_or(0.0),
            })
            .collect()
    }

    fn ends(slots: &[Slot]) -> Vec<f32> {
        slots.iter().map(|slot| slot.end).collect()
    }

    #[test]
    fn orders_are_laid_end_to_end() {
        let slots = schedule(&planned(&[Some(2.0), Some(3.0), Some(1.5)]), None, None);
        assert_eq!(ends(&slots), vec![2.0, 5.0, 6.5]);
        assert_eq!(slots[0].start, 0.0);
        assert_eq!(slots[2].start, 5.0);
        assert_eq!(slots[1].seconds(), 3.0);
    }

    #[test]
    fn the_started_order_takes_its_time_left() {
        let slots = schedule(&planned(&[Some(0.0), Some(3.0)]), Some(1.25), None);
        assert_eq!(ends(&slots), vec![1.25, 4.25]);
    }

    #[test]
    fn skipped_orders_take_no_time() {
        let slots = schedule(&planned(&[Some(2.0), None, Some(1.0)]), None, None);
        assert_eq!(ends(&slots), vec![2.0, 2.0, 3.0]);
    }

    #[test]
    fn orders_ending_after_the_deadline_are_late() {
        // Ends at 4, 8 (partly after the deadline) and 12.
        let slots = schedule(
            &planned(&[Some(4.0), Some(4.0), Some(4.0)]),
            None,
            Some(6.0),
        );
        let late: Vec<bool> = slots.iter().map(|slot| slot.late).collect();
        assert_eq!(late, vec![false, true, true]);
        assert_eq!(first_late(&slots), Some(1));
    }

    #[test]
    fn an_order_that_just_fits_is_on_time() {
        let slots = schedule(&planned(&[Some(2.5), Some(3.5)]), None, Some(6.0));
        assert_eq!(first_late(&slots), None);
    }

    #[test]
    fn skipped_orders_are_never_late() {
        let slots = schedule(&planned(&[Some(8.0), None]), None, Some(6.0));
        assert!(slots[0].late);
        assert!(!slots[1].late);
    }

    #[test]
    fn nothing_is_late_without_a_deadline() {
        let slots = schedule(&planned(&[Some(100.0)]), None, None);
        assert_eq!(first_late(&slots), None);
    }
}
