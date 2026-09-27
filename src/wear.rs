//! Walls and towers wearing down. The horde's hits land here: each one takes HP off the
//! structure in the grid (see [`Grid::damage`]), and one that runs out breaks and leaves bare
//! ground. The flow fields follow: a break rebuilds them right away, and wear rebuilds them every
//! so often, since damaged structures cost the horde less to go through.

use crate::build::StructureChanged;
use crate::flow::Class;
use crate::grid::{Damage, Grid};
use crate::horde::HordeSystems;
use bevy::prelude::*;

pub struct WearPlugin;

impl Plugin for WearPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<StructureHit>()
            .add_message::<StructureDamaged>()
            .add_message::<StructureBroken>()
            .add_systems(Update, apply_hits.in_set(WearSystems).after(HordeSystems));
    }
}

/// Applies the horde's hits to walls and towers. Whatever shows wear runs after it.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WearSystems;

/// An enemy of class `by`, standing at `from` (world x and z), hit the wall or tower on `cell`.
#[derive(Message, Clone, Copy, Debug)]
pub struct StructureHit {
    pub cell: IVec2,
    pub damage: u32,
    pub by: Class,
    pub from: Vec2,
}

/// A wall or tower took hits this frame and still stands, with `hp` out of `max` left. `by` is the
/// hardest-hitting class that hit it, and `from` where one of them stood.
#[derive(Message, Clone, Copy, Debug)]
pub struct StructureDamaged {
    pub cell: IVec2,
    pub hp: u32,
    pub max: u32,
    pub by: Class,
    pub from: Vec2,
}

/// A wall or tower broke. [`StructureChanged`] says so too; this says it wasn't the player
/// removing it.
#[derive(Message, Clone, Copy, Debug)]
pub struct StructureBroken {
    pub cell: IVec2,
    pub by: Class,
}

fn apply_hits(
    mut hits: MessageReader<StructureHit>,
    mut grid: ResMut<Grid>,
    mut damaged: MessageWriter<StructureDamaged>,
    mut broken: MessageWriter<StructureBroken>,
    mut changed: MessageWriter<StructureChanged>,
) {
    // One message per structure, however many hits it took.
    let mut worn: Vec<StructureDamaged> = Vec::new();
    for hit in hits.read() {
        match grid.damage(hit.cell, hit.damage) {
            Some(Damage::Hurt { hp, max }) => {
                if let Some(wall) = worn.iter_mut().find(|wall| wall.cell == hit.cell) {
                    wall.hp = hp;
                    if hit.by == Class::Brute {
                        (wall.by, wall.from) = (hit.by, hit.from);
                    }
                } else {
                    worn.push(StructureDamaged {
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
                broken.write(StructureBroken {
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
