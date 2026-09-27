//! What the mouse points at: the cursor's ray hits the ground plane, and that gives a cell.
//! Nothing is picked by mesh, so tall things never hide the cell behind them.

use crate::camera::{CameraRig, MainCamera};
use crate::dev::ScriptedCursor;
use crate::grid::cell_at;
use bevy::{prelude::*, window::PrimaryWindow};

pub struct PointerPlugin;

impl Plugin for PointerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Pointer>()
            .add_systems(Update, update_pointer.in_set(PointerSystems));
    }
}

/// Runs [`update_pointer`]. Anything reading [`Pointer`] should run after it.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PointerSystems;

/// Where the cursor points this frame.
#[derive(Resource, Default, Debug)]
pub struct Pointer {
    /// The point on the ground plane, or `None` when the cursor is outside the window.
    pub ground: Option<Vec3>,
    /// The cell under that point (it may be outside the map).
    pub cell: Option<IVec2>,
}

/// The cursor in logical pixels: the dev input script's fake cursor if one runs, else the real one.
pub fn cursor_position(window: &Window, scripted: Option<&ScriptedCursor>) -> Option<Vec2> {
    scripted
        .map(|cursor| cursor.0)
        .or_else(|| window.cursor_position())
}

fn update_pointer(
    window: Single<&Window, With<PrimaryWindow>>,
    scripted: Option<Res<ScriptedCursor>>,
    camera: Single<&Camera, With<MainCamera>>,
    rig: Res<CameraRig>,
    mut pointer: ResMut<Pointer>,
) {
    let ground = cursor_position(&window, scripted.as_deref())
        .and_then(|cursor| rig.ground_point(&camera, cursor));
    pointer.ground = ground;
    pointer.cell = ground.map(cell_at);
}
