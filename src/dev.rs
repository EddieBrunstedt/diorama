//! Developer helpers that are off unless asked for.
//!
//! `DIORAMA_SCREENSHOT=shot.png cargo run` saves a screenshot after a few seconds and quits,
//! for checking visuals without clicking around. `DIORAMA_SCREENSHOT_AT` sets the time in seconds.

use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
};

const DEFAULT_SCREENSHOT_AT: f32 = 4.0;
/// Time to let the screenshot reach the disk before quitting.
const EXIT_AFTER: f32 = 1.5;

pub struct DevPlugin;

impl Plugin for DevPlugin {
    fn build(&self, app: &mut App) {
        let Ok(path) = std::env::var("DIORAMA_SCREENSHOT") else {
            return;
        };
        let at = std::env::var("DIORAMA_SCREENSHOT_AT")
            .ok()
            .and_then(|at| at.parse().ok())
            .unwrap_or(DEFAULT_SCREENSHOT_AT);
        app.insert_resource(AutoScreenshot {
            path,
            at,
            taken: false,
        })
        .add_systems(Update, auto_screenshot);
    }
}

#[derive(Resource)]
struct AutoScreenshot {
    path: String,
    at: f32,
    taken: bool,
}

fn auto_screenshot(
    mut commands: Commands,
    time: Res<Time>,
    mut shot: ResMut<AutoScreenshot>,
    mut exit: MessageWriter<AppExit>,
) {
    let now = time.elapsed_secs();
    if !shot.taken && now >= shot.at {
        shot.taken = true;
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(shot.path.clone()));
    }
    if now >= shot.at + EXIT_AFTER {
        exit.write(AppExit::Success);
    }
}
