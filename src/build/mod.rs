//! Building with the mouse, in the build phase. Every click changes the grid right away, and
//! pays for it.
//!
//! - 1–5 (or the build panel, [`panel`]) pick what a left click builds: a wall, one of the
//!   towers, or the Artillery Command.
//! - Left click builds it. Drag to draw a straight line (it follows the axis you dragged further
//!   along); structures appear and disappear live as the line changes, until the money runs out.
//! - Left click on a damaged wall or tower, or on the Command Center, repairs it, as far as the
//!   money goes (drag to repair along a line).
//! - Right click sells, and right drag sells along a line.
//! - Esc, or the other mouse button, cancels a drag in progress.
//! - Ctrl+Z undoes the last click or drag, Ctrl+Shift+Z (or Ctrl+Y) redoes, money and all.
//! - Claiming a chunk ([`crate::chunks`]) goes into the same history, so it can be undone too.
//! - Starting the wave commits what was built, which now sells at a loss, and clears the undo
//!   history. Nothing can be built, sold or repaired until the wave is over.
//! - A new run clears away everything built.
//!
//! The rules and the history are plain data in [`history`] (and the prices in
//! [`crate::economy`]). Whatever changes the grid sends a [`StructureChanged`] for each cell,
//! which the visuals follow.

mod highlight;
pub mod history;
mod panel;

use crate::command_center::CommandCenterRepaired;
use crate::economy::{CantAfford, Wallet};
use crate::grid::{Change, Grid, Kind, Structure};
use crate::map::ChunkChanged;
use crate::pointer::{Pointer, PointerSystems, cell_top};
use crate::waves::Phase;
use bevy::prelude::*;
use history::{Action, History, Stroke, StrokeKind};
pub use panel::UiHovered;

pub struct BuildPlugin;

impl Plugin for BuildPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<History>()
            .init_resource::<ActiveStroke>()
            .init_resource::<Tool>()
            .add_message::<StructureChanged>()
            .add_message::<BuildDenied>()
            .add_plugins((highlight::HighlightPlugin, panel::PanelPlugin))
            .add_systems(OnEnter(Phase::Wave), commit)
            .add_systems(OnExit(Phase::GameOver), clear_the_map)
            .add_systems(
                Update,
                (pick_tool, strokes, undo_redo)
                    .chain()
                    .run_if(in_state(Phase::Build))
                    .in_set(BuildSystems)
                    .after(PointerSystems),
            );
    }
}

/// Changes the grid from player input. Visuals that follow the grid run after it.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BuildSystems;

/// A cell's structure changed (placed, removed, undone or redone).
#[derive(Message, Clone, Copy, Debug)]
pub struct StructureChanged(pub Change);

/// A click that couldn't do anything on this cell (e.g. building on a wall or on rock).
#[derive(Message, Clone, Copy, Debug)]
pub struct BuildDenied {
    pub cell: IVec2,
}

/// The click or drag in progress, and the mouse button doing it.
#[derive(Resource, Default)]
pub struct ActiveStroke(pub Option<(Stroke, MouseButton)>);

/// What a left click builds.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tool(pub Kind);

/// The key that picks each kind, in [`Kind::ALL`] order.
pub const TOOL_KEYS: [(KeyCode, &str); 5] = [
    (KeyCode::Digit1, "1"),
    (KeyCode::Digit2, "2"),
    (KeyCode::Digit3, "3"),
    (KeyCode::Digit4, "4"),
    (KeyCode::Digit5, "5"),
];

fn pick_tool(keys: Res<ButtonInput<KeyCode>>, mut tool: ResMut<Tool>) {
    if keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]) {
        return;
    }
    for ((key, _), kind) in TOOL_KEYS.into_iter().zip(Kind::ALL) {
        if keys.just_pressed(key) {
            tool.set_if_neq(Tool(kind));
        }
    }
}

/// Starting the wave commits the build: it sells at a loss from now on, and can't be undone. A
/// drag still in progress keeps what it did.
fn commit(mut grid: ResMut<Grid>, mut history: ResMut<History>, mut active: ResMut<ActiveStroke>) {
    active.0 = None;
    grid.commit();
    history.clear();
}

/// A new run: everything built is gone (the Command Center stands up again in
/// [`crate::command_center`]).
pub fn clear_the_map(
    mut grid: ResMut<Grid>,
    mut history: ResMut<History>,
    mut active: ResMut<ActiveStroke>,
    mut changed: MessageWriter<StructureChanged>,
) {
    active.0 = None;
    history.clear();
    changed.write_batch(grid.clear_built().into_iter().map(StructureChanged));
}

/// Tells whatever follows the map what an edit did.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Report<'w> {
    changed: MessageWriter<'w, StructureChanged>,
    repaired: MessageWriter<'w, CommandCenterRepaired>,
    chunks: MessageWriter<'w, ChunkChanged>,
}

impl Report<'_> {
    pub fn send(&mut self, actions: Vec<Action>) {
        for action in actions {
            match action {
                Action::Cell(change) => {
                    self.changed.write(StructureChanged(change));
                }
                Action::CommandCenter { from, to, .. } => {
                    self.repaired.write(CommandCenterRepaired { from, to });
                }
                Action::Claim { claim, claimed } => {
                    self.chunks.write(ChunkChanged {
                        at: claim.slot,
                        claimed,
                    });
                }
            }
        }
    }
}

/// Clicks and drags on the map. A click off the map is left to whatever is there (a chunk on
/// offer, see `crate::chunks`), which runs after this, so the click never builds on the chunk it
/// just claimed.
pub fn strokes(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    pointer: Res<Pointer>,
    tool: Res<Tool>,
    ui: Res<UiHovered>,
    mut grid: ResMut<Grid>,
    mut wallet: ResMut<Wallet>,
    mut active: ResMut<ActiveStroke>,
    mut history: ResMut<History>,
    mut report: Report,
    mut denied: MessageWriter<BuildDenied>,
    mut broke: MessageWriter<CantAfford>,
) {
    let mut send = |actions: Vec<Action>| report.send(actions);

    if let Some((stroke, button)) = active.0.take() {
        let other = match button {
            MouseButton::Left => MouseButton::Right,
            _ => MouseButton::Left,
        };
        if keys.just_pressed(KeyCode::Escape) || buttons.just_pressed(other) {
            send(stroke.cancel(&mut grid, &mut wallet));
            return;
        }
        // A drag picks against the map as it was when it began: walls it places don't pull its
        // end toward the camera, and walls it removes don't let the end slip past them.
        let mut stroke = stroke;
        let end = pointer.pick(|cell| {
            grid.cell(cell).map_or(0.0, |_| {
                cell_top(&grid, cell, stroke.structure_before(&grid, cell))
            })
        });
        if let Some(cell) = end {
            send(stroke.update(&mut grid, &mut wallet, cell));
            if stroke.take_short() {
                broke.write(CantAfford);
            }
        }
        if buttons.pressed(button) {
            active.0 = Some((stroke, button));
        } else {
            history.record(stroke.finish());
        }
        return;
    }

    // Clicks on the build panel and the buttons are for them.
    if ui.0 {
        return;
    }
    // Off the map there's nothing to build on (the chunks offered there are claimed by
    // `crate::chunks`).
    let Some(cell) = pointer.cell.filter(|&cell| grid.cell(cell).is_some()) else {
        return;
    };
    // A left click on something damaged repairs it; anywhere else it builds.
    let damaged = grid
        .structure(cell)
        .is_some_and(|structure| match structure {
            Structure::CommandCenter => grid.command_center.damaged(),
            structure => structure.damaged(),
        });
    let left = if damaged {
        StrokeKind::Repair
    } else {
        StrokeKind::Place(tool.0)
    };
    for (button, kind) in [
        (MouseButton::Left, left),
        (MouseButton::Right, StrokeKind::Remove),
    ] {
        if buttons.just_pressed(button) {
            let (mut stroke, changes) = Stroke::begin(kind, cell, &mut grid, &mut wallet);
            if stroke.take_short() {
                broke.write(CantAfford);
            }
            if stroke.is_empty() {
                denied.write(BuildDenied { cell });
            }
            send(changes);
            active.0 = Some((stroke, button));
            return;
        }
    }
}

fn undo_redo(
    keys: Res<ButtonInput<KeyCode>>,
    active: Res<ActiveStroke>,
    mut grid: ResMut<Grid>,
    mut wallet: ResMut<Wallet>,
    mut history: ResMut<History>,
    mut report: Report,
    mut broke: MessageWriter<CantAfford>,
) {
    // Finish (or cancel) the drag first, so undo never tangles with a half-drawn line.
    if active.0.is_some() {
        return;
    }
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    if !ctrl {
        return;
    }
    let changes = if keys.just_pressed(KeyCode::KeyZ) && !shift {
        history.undo(&mut grid, &mut wallet)
    } else if (keys.just_pressed(KeyCode::KeyZ) && shift) || keys.just_pressed(KeyCode::KeyY) {
        history.redo(&mut grid, &mut wallet)
    } else {
        return;
    };
    match changes {
        Ok(actions) => report.send(actions),
        Err(_) => {
            broke.write(CantAfford);
        }
    }
}
