//! The chunk offer as plain data: where the map can grow ([`frontier`]), and the chunks offered
//! after a wave ([`Offer::new`]).
//!
//! Each offer holds [`OFFERED`] chunks, each at its own place along the frontier. One always has
//! a nest and one never does, so there's always a choice between more room and a new front, and
//! the rest have a nest at a chance that grows with the waves. A chunk with a nest pays a bounty
//! (see [`crate::economy::nest_bounty`]). A nest is only offered where the horde can get from it
//! to the Command Center.

use super::templates::{Claim, Orientation, Side, TEMPLATES};
use crate::flow::{Class, FlowField};
use crate::grid::Grid;
use crate::map::Landmarks;
use crate::rng::Rng;
use crate::tuning::chunks::{NEST_CHANCE, NEST_CHANCE_WAVES, OFFERED};
use bevy::prelude::*;

/// A place the map can grow: an empty chunk slot next to the map, and the sides where it meets
/// the map.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slot {
    pub at: IVec2,
    pub sides: Vec<Side>,
}

/// Every empty chunk slot that shares an edge with the map, in a fixed order (row by row), so
/// the same seed makes the same offer.
pub fn frontier(grid: &Grid) -> Vec<Slot> {
    let mut slots: Vec<Slot> = Vec::new();
    for (at, _) in grid.chunks() {
        for side in Side::ALL {
            let slot = at + side.step();
            if grid.chunk(slot).is_some() || slots.iter().any(|s| s.at == slot) {
                continue;
            }
            let sides = Side::ALL
                .into_iter()
                .filter(|side| grid.chunk(slot + side.step()).is_some())
                .collect();
            slots.push(Slot { at: slot, sides });
        }
    }
    slots.sort_by_key(|slot| (slot.at.y, slot.at.x));
    slots
}

/// How likely each of the offer's open places is to hold a nest, after wave `cleared`.
pub fn nest_chance(cleared: u32) -> f32 {
    let (first, last) = NEST_CHANCE;
    let t = (cleared.saturating_sub(1) as f32 / (NEST_CHANCE_WAVES - 1) as f32).min(1.0);
    first + (last - first) * t
}

/// The chunks offered after a wave, one of which the player claims.
#[derive(Clone, Debug, PartialEq)]
pub struct Offer {
    pub choices: Vec<Claim>,
    /// What claiming one with a nest pays.
    pub bounty: u32,
}

impl Offer {
    /// Draws an offer for the map as it is, after wave `cleared`.
    pub fn new(
        grid: &Grid,
        landmarks: &Landmarks,
        cleared: u32,
        bounty: u32,
        rng: &mut Rng,
    ) -> Self {
        let mut slots = frontier(grid);
        shuffle(&mut slots, rng);
        // One with a nest, one without, the rest by chance, in a random order.
        let mut nests: Vec<bool> = (0..OFFERED)
            .map(|i| match i {
                0 => true,
                1 => false,
                _ => rng.f32() < nest_chance(cleared),
            })
            .collect();
        shuffle(&mut nests, rng);
        let choices = slots
            .iter()
            .zip(nests)
            .map(|(slot, nest)| {
                // A nest where the horde can't get out of it isn't offered: plain ground instead.
                let nested = nest
                    .then(|| {
                        pick(slot, true, rng, |claim| {
                            nest_reaches(grid, landmarks, claim)
                        })
                    })
                    .flatten();
                nested.unwrap_or_else(|| {
                    pick(slot, false, rng, |_| true).expect("templates without nests")
                })
            })
            .collect();
        Self { choices, bounty }
    }

    /// The choice the player claimed, if any (its chunk is on the map).
    pub fn claimed(&self, grid: &Grid) -> Option<Claim> {
        self.choices
            .iter()
            .copied()
            .find(|choice| grid.chunk(choice.slot).is_some())
    }

    /// Whether a chunk still has to be claimed before the wave can start.
    pub fn pending(&self, grid: &Grid) -> bool {
        self.claimed(grid).is_none()
    }

    /// The choice whose chunk would hold `cell`, if any.
    pub fn choice_at(&self, cell: IVec2) -> Option<Claim> {
        let slot = Grid::chunk_of(cell);
        self.choices
            .iter()
            .copied()
            .find(|choice| choice.slot == slot)
    }

    /// What claiming `choice` pays.
    pub fn bounty_for(&self, choice: Claim) -> u32 {
        if choice.nest().is_some() {
            self.bounty
        } else {
            0
        }
    }
}

/// A random template (with a nest or without) laid down at `slot`, facing the map, that `fits`.
/// Tries every template and way of laying it down before giving up.
fn pick(slot: &Slot, nest: bool, rng: &mut Rng, fits: impl Fn(Claim) -> bool) -> Option<Claim> {
    let mut templates: Vec<usize> = (0..TEMPLATES.len())
        .filter(|&i| TEMPLATES[i].has_nest() == nest)
        .collect();
    shuffle(&mut templates, rng);
    let mut orientations: Vec<Orientation> = slot
        .sides
        .iter()
        .flat_map(|&facing| {
            [false, true]
                .into_iter()
                .map(move |mirror| Orientation { facing, mirror })
        })
        .collect();
    shuffle(&mut orientations, rng);
    templates.into_iter().find_map(|template| {
        orientations.iter().find_map(|&orientation| {
            let claim = Claim {
                slot: slot.at,
                template,
                orientation,
            };
            fits(claim).then_some(claim)
        })
    })
}

/// Whether both swarmers and brutes can get from the claim's nest to the Command Center, once
/// it's on the map.
pub fn nest_reaches(grid: &Grid, landmarks: &Landmarks, claim: Claim) -> bool {
    let Some(nest) = claim.nest() else {
        return true;
    };
    let mut grid = grid.clone();
    claim.add_to(&mut grid);
    let targets = || landmarks.command_center.cells();
    let swarmers = FlowField::new(&grid, Class::Swarmer, targets());
    let brutes = FlowField::new(&grid, Class::Brute, targets());
    nest.cells().all(|cell| swarmers.distance(cell).is_finite())
        && brutes
            .distance(brutes.node_at(nest.center().xz()))
            .is_finite()
}

/// Fisher–Yates.
fn shuffle<T>(items: &mut [T], rng: &mut Rng) {
    for i in (1..items.len()).rev() {
        items.swap(i, rng.index(i + 1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::Terrain;
    use crate::map::starting_map;

    #[test]
    fn the_frontier_rings_the_map() {
        let (grid, _) = starting_map();
        let slots = frontier(&grid);
        // The 2×2 starting map has two free slots along each side.
        assert_eq!(slots.len(), 8);
        for slot in &slots {
            assert!(grid.chunk(slot.at).is_none());
            assert_eq!(slot.sides.len(), 1, "{slot:?}");
            assert!(grid.chunk(slot.at + slot.sides[0].step()).is_some());
        }
        // A chunk added north of the top-left one takes a slot and opens two more around it,
        // and the slot beside it (north of the top-right one) now meets the map on two sides.
        let mut grid = grid;
        let claim = Claim {
            slot: IVec2::new(-1, -2),
            template: 0,
            orientation: Orientation {
                facing: Side::South,
                mirror: false,
            },
        };
        claim.add_to(&mut grid);
        let slots = frontier(&grid);
        assert_eq!(slots.len(), 9);
        let corner = slots.iter().find(|s| s.at == IVec2::new(0, -2)).unwrap();
        assert_eq!(corner.sides.len(), 2);
    }

    #[test]
    fn offers_mix_room_and_fronts() {
        let (grid, landmarks) = starting_map();
        for seed in 0..40 {
            let mut rng = Rng::new(seed);
            let cleared = 1 + (seed as u32 % 25);
            let offer = Offer::new(&grid, &landmarks, cleared, 50, &mut rng);
            assert_eq!(offer.choices.len(), OFFERED);
            let nests = offer.choices.iter().filter(|c| c.nest().is_some()).count();
            assert!((1..OFFERED).contains(&nests), "seed {seed}: {nests} nests");
            let mut slots: Vec<IVec2> = offer.choices.iter().map(|c| c.slot).collect();
            slots.sort_by_key(|slot| (slot.y, slot.x));
            slots.dedup();
            assert_eq!(slots.len(), OFFERED, "each at its own place");
            for choice in &offer.choices {
                assert!(grid.chunk(choice.slot).is_none());
                // Laid down facing the map.
                let facing = choice.orientation.facing;
                assert!(grid.chunk(choice.slot + facing.step()).is_some());
                assert!(nest_reaches(&grid, &landmarks, *choice));
                assert_eq!(
                    offer.bounty_for(*choice),
                    if choice.nest().is_some() { 50 } else { 0 }
                );
            }
        }
    }

    #[test]
    fn the_same_seed_makes_the_same_offer() {
        let (grid, landmarks) = starting_map();
        let offer = |seed| Offer::new(&grid, &landmarks, 3, 50, &mut Rng::new(seed));
        assert_eq!(offer(7), offer(7));
        assert!((0..10).any(|seed| offer(seed) != offer(7)));
    }

    #[test]
    fn nests_come_more_often_as_the_waves_go_by() {
        assert_eq!(nest_chance(1), NEST_CHANCE.0);
        assert!(nest_chance(10) > nest_chance(5));
        assert_eq!(nest_chance(NEST_CHANCE_WAVES), NEST_CHANCE.1);
        assert_eq!(nest_chance(100), NEST_CHANCE.1);
    }

    #[test]
    fn a_nest_the_horde_cannot_leave_is_not_offered() {
        let (mut grid, landmarks) = starting_map();
        // Wall the north edge of the starting map off with rock: nothing claimed up there can
        // reach the Command Center.
        for x in -16..16 {
            grid.set_terrain(IVec2::new(x, -16), Terrain::Rock);
        }
        let north = Slot {
            at: IVec2::new(-1, -2),
            sides: vec![Side::South],
        };
        let mut rng = Rng::new(1);
        let fits = |claim| nest_reaches(&grid, &landmarks, claim);
        assert_eq!(pick(&north, true, &mut rng, fits), None);
        assert!(pick(&north, false, &mut rng, |_| true).is_some());
    }

    #[test]
    fn claiming_one_settles_the_offer() {
        let (mut grid, landmarks) = starting_map();
        let offer = Offer::new(&grid, &landmarks, 2, 45, &mut Rng::new(3));
        assert!(offer.pending(&grid));
        let choice = offer.choices[1];
        let cell = Grid::chunk_origin(choice.slot) + IVec2::new(5, 9);
        assert_eq!(offer.choice_at(cell), Some(choice));
        assert_eq!(offer.choice_at(IVec2::ZERO), None, "on the map");
        choice.add_to(&mut grid);
        assert_eq!(offer.claimed(&grid), Some(choice));
        assert!(!offer.pending(&grid));
    }
}
