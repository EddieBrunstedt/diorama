//! Island Keeper (code name "diorama"): a cozy voxel tower defense.
//!
//! `main` only wires plugins together; each feature area lives in its own module.

// Bevy systems naturally take many parameters and spell out their queries.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod camera;
mod dev;
mod environment;
mod hearth;
mod island;
mod juice;
mod keeper;
mod palette;
mod phase;
mod rng;
mod towers;
mod tuning;
mod ui;
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
            towers::TowersPlugin,
            phase::PhasePlugin,
            keeper::KeeperPlugin,
            ui::UiPlugin,
            juice::JuicePlugin,
            dev::DevPlugin,
        ))
        .run();
}
