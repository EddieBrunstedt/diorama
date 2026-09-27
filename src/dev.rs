//! Developer helpers that are off unless asked for (except the FPS counter).
//!
//! - A frame time readout sits in the top-right corner (see [`fps`]).
//! - `DIORAMA_SCREENSHOT=shot.png cargo run` saves a screenshot after a few seconds and quits,
//!   for checking visuals without clicking around. `DIORAMA_SCREENSHOT_AT` sets the time in
//!   seconds; a comma-separated list takes several (`shot-1.png`, `shot-2.png`, ...).
//! - `DIORAMA_DEMO=1` queues a few sample orders at startup.
//! - `DIORAMA_SCRIPT="3 move 800 450; 3.1 down; 3.2 up"` fakes mouse input at the given times
//!   (seconds): `move X Y` (logical pixels), `down`/`up` (left button), `shift`/`unshift`, and
//!   `key K` to tap a key (a letter, a digit, or `esc`), e.g. `key b` rings the bell.
//! - `DIORAMA_SPEED=4` runs the game four times as fast, e.g. to get to later waves quickly.
//!   Screenshot and script times are in game time, so they speed up too.
//! - `DIORAMA_SEED=7` fixes the random seed for the tile offers, so scripted runs repeat.

mod fps;

use crate::island::TreeId;
use crate::keeper::{OrderKind, OrderQueue};
use bevy::{
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput, NativeKey},
        mouse::MouseButtonInput,
    },
    picking::PickingSystems,
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
    window::{CursorMoved, PrimaryWindow, WindowEvent},
};

const DEFAULT_SCREENSHOT_AT: f32 = 4.0;
/// Time to let the screenshot reach the disk before quitting.
const EXIT_AFTER: f32 = 1.5;

pub struct DevPlugin;

impl Plugin for DevPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(fps::FpsPlugin);
        if let Ok(path) = std::env::var("DIORAMA_SCREENSHOT") {
            let times: Vec<f32> = std::env::var("DIORAMA_SCREENSHOT_AT")
                .ok()
                .map(|at| {
                    at.split(',')
                        .filter_map(|t| t.trim().parse().ok())
                        .collect()
                })
                .filter(|times: &Vec<f32>| !times.is_empty())
                .unwrap_or(vec![DEFAULT_SCREENSHOT_AT]);
            app.insert_resource(AutoScreenshot {
                path,
                times,
                taken: 0,
            })
            .add_systems(Update, auto_screenshot);
        }
        if std::env::var("DIORAMA_DEMO").is_ok() {
            app.add_systems(Startup, queue_demo_orders);
        }
        if let Some(speed) = std::env::var("DIORAMA_SPEED")
            .ok()
            .and_then(|speed| speed.parse::<f32>().ok())
        {
            app.add_systems(Startup, move |mut time: ResMut<Time<Virtual>>| {
                time.set_relative_speed(speed);
                // Let long frames through, so fast-forwarding doesn't slow down again.
                time.set_max_delta(std::time::Duration::from_secs_f32(0.25 * speed));
            });
        }
        if let Ok(script) = std::env::var("DIORAMA_SCRIPT") {
            let steps =
                parse_script(&script).unwrap_or_else(|error| panic!("DIORAMA_SCRIPT: {error}"));
            app.insert_resource(InputScript { steps, next: 0 })
                .add_systems(First, run_input_script.before(PickingSystems::Input));
        }
    }
}

#[derive(Resource)]
struct AutoScreenshot {
    path: String,
    times: Vec<f32>,
    taken: usize,
}

fn auto_screenshot(
    mut commands: Commands,
    time: Res<Time>,
    mut shot: ResMut<AutoScreenshot>,
    mut exit: MessageWriter<AppExit>,
) {
    let now = time.elapsed_secs();
    if let Some(&at) = shot.times.get(shot.taken)
        && now >= at
    {
        let path = if shot.times.len() == 1 {
            shot.path.clone()
        } else {
            numbered(&shot.path, shot.taken + 1)
        };
        shot.taken += 1;
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
    }
    let last = shot.times.iter().copied().fold(0.0, f32::max);
    if now >= last + EXIT_AFTER {
        exit.write(AppExit::Success);
    }
}

/// `shot.png` → `shot-2.png`.
fn numbered(path: &str, number: usize) -> String {
    match path.rsplit_once('.') {
        Some((stem, extension)) => format!("{stem}-{number}.{extension}"),
        None => format!("{path}-{number}"),
    }
}

/// A mix of orders, including one without enough wood and a few that won't be done before the
/// first wave.
fn queue_demo_orders(mut queue: ResMut<OrderQueue>) {
    let tree = |x, y, index| {
        OrderKind::Chop(TreeId {
            cell: IVec2::new(x, y),
            index,
        })
    };
    for kind in [
        tree(-2, -1, 0),
        tree(-2, -2, 0),
        tree(-1, -2, 1),
        OrderKind::Build(IVec2::new(-1, -1)),
        OrderKind::Walk(Vec2::new(1.2, 1.3)),
        OrderKind::Build(IVec2::new(0, 1)),
        tree(-2, 1, 0),
        tree(2, 1, 0),
        tree(2, 2, 0),
        OrderKind::Build(IVec2::new(1, 1)),
    ] {
        queue.push(kind);
    }
}

/// Where the fake mouse is. Present only while a script runs; overrides the real cursor.
#[derive(Resource, Clone, Copy, Debug)]
pub struct ScriptedCursor(pub Vec2);

#[derive(Clone, Copy, Debug, PartialEq)]
enum Step {
    Move(Vec2),
    Down,
    Up,
    Shift(bool),
    Key(KeyCode),
}

/// A key by name: a letter, a digit, or `esc`.
fn key_code(name: &str) -> Option<KeyCode> {
    let upper = name.to_ascii_uppercase();
    let code = match upper.as_str() {
        "ESC" => KeyCode::Escape,
        "A" => KeyCode::KeyA,
        "B" => KeyCode::KeyB,
        "R" => KeyCode::KeyR,
        "1" => KeyCode::Digit1,
        "2" => KeyCode::Digit2,
        "3" => KeyCode::Digit3,
        _ => return None,
    };
    Some(code)
}

#[derive(Resource)]
struct InputScript {
    steps: Vec<(f32, Step)>,
    next: usize,
}

fn parse_script(script: &str) -> Result<Vec<(f32, Step)>, String> {
    script
        .split(';')
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            let words: Vec<&str> = line.split_whitespace().collect();
            let number = |index: usize| -> Result<f32, String> {
                words
                    .get(index)
                    .and_then(|word| word.parse().ok())
                    .ok_or_else(|| format!("expected a number in {line:?}"))
            };
            let step = match words.get(1).copied() {
                Some("move") => Step::Move(Vec2::new(number(2)?, number(3)?)),
                Some("down") => Step::Down,
                Some("up") => Step::Up,
                Some("shift") => Step::Shift(true),
                Some("unshift") => Step::Shift(false),
                Some("key") => Step::Key(
                    words
                        .get(2)
                        .and_then(|name| key_code(name))
                        .ok_or_else(|| format!("unknown key in {line:?}"))?,
                ),
                _ => return Err(format!("unknown step {line:?}")),
            };
            Ok((number(0)?, step))
        })
        .collect()
}

fn run_input_script(
    mut commands: Commands,
    time: Res<Time>,
    mut script: ResMut<InputScript>,
    window: Single<Entity, With<PrimaryWindow>>,
    mut window_events: MessageWriter<WindowEvent>,
    mut buttons: MessageWriter<MouseButtonInput>,
    mut keyboard: MessageWriter<KeyboardInput>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
) {
    let window = *window;
    while let Some(&(at, step)) = script.steps.get(script.next)
        && time.elapsed_secs() >= at
    {
        script.next += 1;
        let button = |state| MouseButtonInput {
            button: MouseButton::Left,
            state,
            window,
        };
        match step {
            Step::Move(position) => {
                commands.insert_resource(ScriptedCursor(position));
                window_events.write(WindowEvent::CursorMoved(CursorMoved {
                    window,
                    position,
                    delta: None,
                }));
            }
            Step::Down | Step::Up => {
                let state = if step == Step::Down {
                    ButtonState::Pressed
                } else {
                    ButtonState::Released
                };
                buttons.write(button(state));
                window_events.write(WindowEvent::MouseButtonInput(button(state)));
            }
            Step::Shift(true) => keys.press(KeyCode::ShiftLeft),
            Step::Shift(false) => keys.release(KeyCode::ShiftLeft),
            Step::Key(key_code) => {
                // Pressed and released within the frame: it still counts as just pressed.
                for state in [ButtonState::Pressed, ButtonState::Released] {
                    keyboard.write(KeyboardInput {
                        key_code,
                        logical_key: Key::Unidentified(NativeKey::Unidentified),
                        state,
                        text: None,
                        repeat: false,
                        window,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_scripts() {
        let steps = parse_script("1 move 10 20; 1.5 down ;2 up; 3 shift; 4 key b").unwrap();
        assert_eq!(
            steps,
            vec![
                (1.0, Step::Move(Vec2::new(10.0, 20.0))),
                (1.5, Step::Down),
                (2.0, Step::Up),
                (3.0, Step::Shift(true)),
                (4.0, Step::Key(KeyCode::KeyB)),
            ]
        );
        assert!(parse_script("1 jump").is_err());
        assert!(parse_script("x down").is_err());
        assert!(parse_script("1 key f13").is_err());
    }

    #[test]
    fn numbers_screenshot_paths() {
        assert_eq!(numbered("out/shot.png", 2), "out/shot-2.png");
        assert_eq!(numbered("shot", 1), "shot-1");
    }
}
