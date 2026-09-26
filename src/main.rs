//! Island Keeper (code name "diorama"): a cozy voxel tower defense.
//!
//! `main` only wires plugins together; each feature area lives in its own module.

mod camera;
mod dev;
mod environment;
mod hearth;
mod island;
mod juice;
mod palette;
mod rng;
mod tuning;
mod voxel;

use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Island Keeper".into(),
                resolution: (1600, 900).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins((
            environment::EnvironmentPlugin,
            camera::CameraPlugin,
            island::IslandPlugin,
            hearth::HearthPlugin,
            juice::JuicePlugin,
            dev::DevPlugin,
        ))
        .run();
}
