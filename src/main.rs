//! Code name "diorama": a sci-fi roguelite tower defense. A human outpost holds out against an
//! alien horde by shaping its path.
//!
//! `main` only wires plugins together; each feature area lives in its own module.

// Bevy systems naturally take many parameters and spell out their queries.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod build;
mod camera;
mod dev;
mod environment;
mod grid;
mod hud;
mod juice;
mod map;
mod palette;
mod pointer;
mod rng;
mod shapes;
mod structures;
mod tuning;

use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Diorama".into(),
                resolution: (1600, 900).into(),
                // Vsync off during development, so the FPS counter shows true performance.
                present_mode: bevy::window::PresentMode::AutoNoVsync,
                ..default()
            }),
            ..default()
        }))
        .init_resource::<rng::RunSeed>()
        .add_plugins((
            environment::EnvironmentPlugin,
            map::MapPlugin,
            camera::CameraPlugin,
            pointer::PointerPlugin,
            build::BuildPlugin,
            structures::StructuresPlugin,
            hud::HudPlugin,
            juice::JuicePlugin,
            dev::DevPlugin,
        ))
        .run();
}
