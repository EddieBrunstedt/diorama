//! Developer helpers that are off unless asked for (except the FPS counter).
//!
//! - A frame time readout sits in the top-right corner (see [`fps`]).
//! - `DIORAMA_SCREENSHOT=shot.png cargo run` saves a screenshot after a few seconds and quits,
//!   for checking visuals without clicking around. `DIORAMA_SCREENSHOT_AT` sets the time in
//!   seconds; a comma-separated list takes several (`shot-1.png`, `shot-2.png`, ...).
//! - `DIORAMA_SCRIPT="1 move 800 450; 1.1 down; 1.5 move 900 450; 1.6 up"` fakes input at the
//!   given times (seconds):
//!   - `move X Y` moves the cursor (logical pixels). It moves the window's own cursor too, so
//!     UI buttons can be clicked (on Wayland, winit logs that it can't move the real pointer:
//!     harmless);
//!   - `cell X Y` moves the cursor over the middle of grid cell (X, Y) on the ground, wherever
//!     the window's size puts it;
//!   - `down` / `up` presses or releases a mouse button: `left` (the default), `right` or
//!     `middle`, e.g. `2 down right`;
//!   - `key K` taps a key, `hold K` / `release K` keep one down (a letter, a digit, `esc`,
//!     `space`, `tab`, `ctrl` or `shift`), e.g. `3 hold ctrl; 3.1 key z; 3.2 release ctrl` to
//!     undo;
//!   - `scroll N` scrolls N lines (positive zooms in).
//! - `DIORAMA_SPEED=4` runs the game four times as fast (0.25 for slow motion), on top of the
//!   wave's speed toggle. Screenshot and script times are in game time, so they speed up too.
//! - `DIORAMA_WAVE=15` starts runs at wave 15, e.g. for measuring a big wave.
//! - `DIORAMA_BATCH=3000` makes every wave that many swarmers, for testing how many the game can
//!   take. `DIORAMA_BRUTES=10` does the same for brutes.
//! - `DIORAMA_MONEY=5000` starts runs with that much money.
//! - `DIORAMA_BASE_HP=1000000` gives the Command Center that much HP, e.g. so a big wave plays
//!   out in full for measuring.
//! - `DIORAMA_CHUNKS=20` grows the starting map by that many chunks, each picked at random from
//!   an offer, e.g. for looking at a big map (a restart goes back to the plain starting map).
//! - `DIORAMA_SEED=7` fixes the random seed (see [`RunSeed`]), so scripted runs repeat.
//! - `DIORAMA_FRAME_LOG=1` logs frame times, the size of the horde, and how long the horde's and
//!   the towers' steps take, once a second (see [`frame_log`]).
//! - `DIORAMA_GPU_TIMES=1` logs the GPU time of each render pass every few seconds (see [`gpu`]).

mod fps;
mod frame_log;
mod gpu;

use crate::camera::{CameraRig, MainCamera};
use crate::chunks::offer::claim_at_random;
use crate::command_center::BaseHp;
use crate::economy::StartingMoney;
use crate::grid::Grid;
use crate::grid::cell_center;
use crate::juice::TimeScale;
use crate::map::Landmarks;
use crate::rng::Rng;
use crate::rng::RunSeed;
use crate::waves::WaveSettings;
use bevy::{
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput, NativeKey},
        mouse::{MouseButtonInput, MouseScrollUnit, MouseWheel},
        touch::TouchPhase,
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
        if std::env::var_os("DIORAMA_FRAME_LOG").is_some() {
            app.add_plugins(frame_log::FrameLogPlugin);
        }
        if std::env::var_os("DIORAMA_GPU_TIMES").is_some() {
            app.add_plugins(gpu::GpuTimesPlugin);
        }
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
        if let Some(speed) = env_number::<f32>("DIORAMA_SPEED") {
            app.insert_resource(TimeScale {
                dev: speed,
                ..default()
            })
            .add_systems(Startup, move |mut time: ResMut<Time<Virtual>>| {
                // Let long frames through, so fast-forwarding doesn't slow down again.
                time.set_max_delta(std::time::Duration::from_secs_f32(0.25 * speed.max(1.0)));
            });
        }
        app.insert_resource(WaveSettings {
            first: env_number("DIORAMA_WAVE"),
            swarmers: env_number("DIORAMA_BATCH"),
            brutes: env_number("DIORAMA_BRUTES"),
        });
        if let Some(money) = env_number::<u32>("DIORAMA_MONEY") {
            app.insert_resource(StartingMoney(money));
        }
        if let Some(hp) = env_number::<u32>("DIORAMA_BASE_HP") {
            app.insert_resource(BaseHp(hp));
        }
        if let Some(seed) = env_number::<u64>("DIORAMA_SEED") {
            app.insert_resource(RunSeed(seed));
        }
        if let Some(count) = env_number::<u32>("DIORAMA_CHUNKS") {
            let world = app.world_mut();
            let mut rng = Rng::new(world.resource::<RunSeed>().0 ^ 0xD1CE);
            let landmarks = world.resource::<Landmarks>().clone();
            let mut grid = world.resource_mut::<Grid>();
            claim_at_random(&mut grid, &landmarks, count, &mut rng);
        }
        if let Ok(script) = std::env::var("DIORAMA_SCRIPT") {
            let steps =
                parse_script(&script).unwrap_or_else(|error| panic!("DIORAMA_SCRIPT: {error}"));
            app.insert_resource(InputScript { steps, next: 0 })
                .add_systems(First, run_input_script.before(PickingSystems::Input));
        }
    }
}

fn env_number<T: std::str::FromStr>(name: &str) -> Option<T> {
    let value = std::env::var(name).ok()?;
    let number = value
        .parse()
        .unwrap_or_else(|_| panic!("{name}: not a number: {value:?}"));
    Some(number)
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

/// Where the fake mouse is. Present only while a script runs; overrides the real cursor.
#[derive(Resource, Clone, Copy, Debug)]
pub struct ScriptedCursor(pub Vec2);

#[derive(Clone, Copy, Debug, PartialEq)]
enum Step {
    Move(Vec2),
    Cell(IVec2),
    Button(MouseButton, ButtonState),
    Key(KeyCode, Press),
    Scroll(f32),
}

/// What a key step does with the key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Press {
    Tap,
    Hold,
    Release,
}

/// A key by name: a letter, a digit, `esc`, `space`, `tab`, `ctrl` or `shift`.
fn key_code(name: &str) -> Option<KeyCode> {
    const LETTERS: [KeyCode; 26] = [
        KeyCode::KeyA,
        KeyCode::KeyB,
        KeyCode::KeyC,
        KeyCode::KeyD,
        KeyCode::KeyE,
        KeyCode::KeyF,
        KeyCode::KeyG,
        KeyCode::KeyH,
        KeyCode::KeyI,
        KeyCode::KeyJ,
        KeyCode::KeyK,
        KeyCode::KeyL,
        KeyCode::KeyM,
        KeyCode::KeyN,
        KeyCode::KeyO,
        KeyCode::KeyP,
        KeyCode::KeyQ,
        KeyCode::KeyR,
        KeyCode::KeyS,
        KeyCode::KeyT,
        KeyCode::KeyU,
        KeyCode::KeyV,
        KeyCode::KeyW,
        KeyCode::KeyX,
        KeyCode::KeyY,
        KeyCode::KeyZ,
    ];
    const DIGITS: [KeyCode; 10] = [
        KeyCode::Digit0,
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
    ];
    let name = name.to_ascii_lowercase();
    let code = match name.as_str() {
        "esc" => KeyCode::Escape,
        "space" => KeyCode::Space,
        "tab" => KeyCode::Tab,
        "ctrl" => KeyCode::ControlLeft,
        "shift" => KeyCode::ShiftLeft,
        _ => match *name.as_bytes() {
            [letter @ b'a'..=b'z'] => LETTERS[(letter - b'a') as usize],
            [digit @ b'0'..=b'9'] => DIGITS[(digit - b'0') as usize],
            _ => return None,
        },
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
            let button = || match words.get(2).copied() {
                None | Some("left") => Ok(MouseButton::Left),
                Some("right") => Ok(MouseButton::Right),
                Some("middle") => Ok(MouseButton::Middle),
                Some(other) => Err(format!("unknown mouse button {other:?} in {line:?}")),
            };
            let key = |press| {
                words
                    .get(2)
                    .and_then(|name| key_code(name))
                    .map(|code| Step::Key(code, press))
                    .ok_or_else(|| format!("unknown key in {line:?}"))
            };
            let step = match words.get(1).copied() {
                Some("move") => Step::Move(Vec2::new(number(2)?, number(3)?)),
                Some("cell") => {
                    let (x, y) = (number(2)?, number(3)?);
                    if x.fract() != 0.0 || y.fract() != 0.0 {
                        return Err(format!("expected whole cells in {line:?}"));
                    }
                    Step::Cell(IVec2::new(x as i32, y as i32))
                }
                Some("down") => Step::Button(button()?, ButtonState::Pressed),
                Some("up") => Step::Button(button()?, ButtonState::Released),
                Some("key") => key(Press::Tap)?,
                Some("hold") => key(Press::Hold)?,
                Some("release") => key(Press::Release)?,
                Some("scroll") => Step::Scroll(number(2)?),
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
    mut primary: Single<(Entity, &mut Window), With<PrimaryWindow>>,
    camera: Single<&Camera, With<MainCamera>>,
    rig: Res<CameraRig>,
    mut window_events: MessageWriter<WindowEvent>,
    mut buttons: MessageWriter<MouseButtonInput>,
    mut keyboard: MessageWriter<KeyboardInput>,
    mut wheel: MessageWriter<MouseWheel>,
) {
    let (window, ref mut window_state) = *primary;
    while let Some(&(at, step)) = script.steps.get(script.next)
        && time.elapsed_secs() >= at
    {
        script.next += 1;
        let step = match step {
            Step::Cell(cell) => {
                let center = cell_center(cell);
                let transform = GlobalTransform::from(rig.transform());
                match camera.world_to_viewport(&transform, center) {
                    Ok(position) => Step::Move(position),
                    Err(error) => {
                        warn!("DIORAMA_SCRIPT: cell {cell} isn't on screen: {error:?}");
                        continue;
                    }
                }
            }
            step => step,
        };
        match step {
            Step::Cell(_) => unreachable!("turned into a move above"),
            Step::Move(position) => {
                commands.insert_resource(ScriptedCursor(position));
                // The UI's buttons follow the window's own cursor, not cursor events.
                window_state.set_cursor_position(Some(position));
                window_events.write(WindowEvent::CursorMoved(CursorMoved {
                    window,
                    position,
                    delta: None,
                }));
            }
            Step::Button(button, state) => {
                let input = MouseButtonInput {
                    button,
                    state,
                    window,
                };
                buttons.write(input);
                window_events.write(WindowEvent::MouseButtonInput(input));
            }
            Step::Key(key_code, press) => {
                let states: &[ButtonState] = match press {
                    // Pressed and released within the frame: it still counts as just pressed.
                    Press::Tap => &[ButtonState::Pressed, ButtonState::Released],
                    Press::Hold => &[ButtonState::Pressed],
                    Press::Release => &[ButtonState::Released],
                };
                for &state in states {
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
            Step::Scroll(lines) => {
                wheel.write(MouseWheel {
                    unit: MouseScrollUnit::Line,
                    x: 0.0,
                    y: lines,
                    window,
                    phase: TouchPhase::Moved,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_scripts() {
        let steps = parse_script(
            "1 move 10 20; 1.5 down ;2 up right; 2.5 down middle; 3 hold ctrl; 3.1 key Z; \
             3.2 release ctrl; 4 scroll -2; 5 cell -3 7",
        )
        .unwrap();
        assert_eq!(
            steps,
            vec![
                (1.0, Step::Move(Vec2::new(10.0, 20.0))),
                (1.5, Step::Button(MouseButton::Left, ButtonState::Pressed)),
                (2.0, Step::Button(MouseButton::Right, ButtonState::Released)),
                (2.5, Step::Button(MouseButton::Middle, ButtonState::Pressed)),
                (3.0, Step::Key(KeyCode::ControlLeft, Press::Hold)),
                (3.1, Step::Key(KeyCode::KeyZ, Press::Tap)),
                (3.2, Step::Key(KeyCode::ControlLeft, Press::Release)),
                (4.0, Step::Scroll(-2.0)),
                (5.0, Step::Cell(IVec2::new(-3, 7))),
            ]
        );
        assert!(parse_script("1 jump").is_err());
        assert!(parse_script("x down").is_err());
        assert!(parse_script("1 key f13").is_err());
        assert!(parse_script("1 down sideways").is_err());
        assert!(parse_script("1 cell 0.5 2").is_err());
    }

    #[test]
    fn names_keys() {
        assert_eq!(key_code("a"), Some(KeyCode::KeyA));
        assert_eq!(key_code("Z"), Some(KeyCode::KeyZ));
        assert_eq!(key_code("7"), Some(KeyCode::Digit7));
        assert_eq!(key_code("esc"), Some(KeyCode::Escape));
        assert_eq!(key_code("ab"), None);
    }

    #[test]
    fn numbers_screenshot_paths() {
        assert_eq!(numbered("out/shot.png", 2), "out/shot-2.png");
        assert_eq!(numbered("shot", 1), "shot-1");
    }
}
