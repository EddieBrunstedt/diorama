//! Building with the mouse. Every click changes the grid right away.
//!
//! - Left click places a wall. Drag to draw a straight line (it follows the axis you dragged
//!   further along); walls appear and disappear live as the line changes.
//! - Right click removes, and right drag removes along a line.
//! - Esc, or the other mouse button, cancels a drag in progress.
//! - Ctrl+Z undoes the last click or drag, Ctrl+Shift+Z (or Ctrl+Y) redoes.
//!
//! The rules and the history are plain data in [`history`]. Whatever changes the grid sends a
//! [`StructureChanged`] for each cell, which the visuals follow.

mod highlight;
pub mod history;

use crate::grid::{Change, Grid, Structure};
use crate::pointer::{Pointer, PointerSystems, cell_top};
use bevy::prelude::*;
use history::{History, Stroke, StrokeKind};

pub struct BuildPlugin;

impl Plugin for BuildPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<History>()
            .init_resource::<ActiveStroke>()
            .add_message::<StructureChanged>()
            .add_message::<BuildDenied>()
            .add_plugins(highlight::HighlightPlugin)
            .add_systems(
                Update,
                (strokes, undo_redo)
                    .chain()
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

fn strokes(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    pointer: Res<Pointer>,
    mut grid: ResMut<Grid>,
    mut active: ResMut<ActiveStroke>,
    mut history: ResMut<History>,
    mut changed: MessageWriter<StructureChanged>,
    mut denied: MessageWriter<BuildDenied>,
) {
    let mut send =
        |changes: Vec<Change>| changed.write_batch(changes.into_iter().map(StructureChanged));

    if let Some((stroke, button)) = active.0.take() {
        let other = match button {
            MouseButton::Left => MouseButton::Right,
            _ => MouseButton::Left,
        };
        if keys.just_pressed(KeyCode::Escape) || buttons.just_pressed(other) {
            send(stroke.cancel(&mut grid));
            return;
        }
        // A drag picks against the map as it was when it began: walls it places don't pull its
        // end toward the camera, and walls it removes don't let the end slip past them.
        let mut stroke = stroke;
        let end = pointer.pick(|cell| {
            grid.cell(cell).map_or(0.0, |data| {
                cell_top(cell, data.terrain, stroke.structure_before(&grid, cell))
            })
        });
        if let Some(cell) = end {
            send(stroke.update(&mut grid, cell));
        }
        if buttons.pressed(button) {
            active.0 = Some((stroke, button));
        } else {
            history.record(stroke.finish());
        }
        return;
    }

    let Some(cell) = pointer.cell else {
        return;
    };
    for (button, kind) in [
        (MouseButton::Left, StrokeKind::Place(Structure::Wall)),
        (MouseButton::Right, StrokeKind::Remove),
    ] {
        if buttons.just_pressed(button) {
            let (stroke, changes) = Stroke::begin(kind, cell, &mut grid);
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
    mut history: ResMut<History>,
    mut changed: MessageWriter<StructureChanged>,
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
        history.undo(&mut grid)
    } else if (keys.just_pressed(KeyCode::KeyZ) && shift) || keys.just_pressed(KeyCode::KeyY) {
        history.redo(&mut grid)
    } else {
        return;
    };
    changed.write_batch(changes.into_iter().map(StructureChanged));
}
