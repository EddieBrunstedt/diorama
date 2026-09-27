//! Walls wearing down. The horde's hits land here: each one takes HP off the wall in the grid
//! (see [`Grid::damage`]), and a wall that runs out breaks and leaves bare ground. The flow fields
//! follow: a break rebuilds them right away, and wear rebuilds them every so often, since damaged
//! walls cost the horde less to go through.

use crate::build::StructureChanged;
use crate::flow::Class;
use crate::grid::{Damage, Grid};
use crate::horde::HordeSystems;
use bevy::prelude::*;

pub struct WallsPlugin;

impl Plugin for WallsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<WallHit>()
            .add_message::<WallDamaged>()
            .add_message::<WallBroken>()
            .add_systems(Update, apply_hits.in_set(WallSystems).after(HordeSystems));
    }
}

/// Applies the horde's hits to the walls. Whatever shows wear runs after it.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WallSystems;

/// An enemy of class `by`, standing at `from` (world x and z), hit the wall on `cell`.
#[derive(Message, Clone, Copy, Debug)]
pub struct WallHit {
    pub cell: IVec2,
    pub damage: u32,
    pub by: Class,
    pub from: Vec2,
}

/// A wall took hits this frame and still stands, with `hp` out of `max` left. `by` is the
/// hardest-hitting class that hit it, and `from` where one of them stood.
#[derive(Message, Clone, Copy, Debug)]
pub struct WallDamaged {
    pub cell: IVec2,
    pub hp: u32,
    pub max: u32,
    pub by: Class,
    pub from: Vec2,
}

/// A wall broke. [`StructureChanged`] says so too; this says it wasn't the player removing it.
#[derive(Message, Clone, Copy, Debug)]
pub struct WallBroken {
    pub cell: IVec2,
    pub by: Class,
}

fn apply_hits(
    mut hits: MessageReader<WallHit>,
    mut grid: ResMut<Grid>,
    mut damaged: MessageWriter<WallDamaged>,
    mut broken: MessageWriter<WallBroken>,
    mut changed: MessageWriter<StructureChanged>,
) {
    // One message per wall, however many hits it took.
    let mut worn: Vec<WallDamaged> = Vec::new();
    for hit in hits.read() {
        match grid.damage(hit.cell, hit.damage) {
            Some(Damage::Hurt { hp, max }) => {
                if let Some(wall) = worn.iter_mut().find(|wall| wall.cell == hit.cell) {
                    wall.hp = hp;
                    if hit.by == Class::Brute {
                        (wall.by, wall.from) = (hit.by, hit.from);
                    }
                } else {
                    worn.push(WallDamaged {
                        cell: hit.cell,
                        hp,
                        max,
                        by: hit.by,
                        from: hit.from,
                    });
                }
            }
            Some(Damage::Broke(change)) => {
                worn.retain(|wall| wall.cell != hit.cell);
                broken.write(WallBroken {
                    cell: hit.cell,
                    by: hit.by,
                });
                changed.write(StructureChanged(change));
            }
            // Already broken by an earlier hit.
            None => {}
        }
    }
    damaged.write_batch(worn);
}
