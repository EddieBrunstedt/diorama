//! The wave loop on screen:
//!
//! - Bottom-right, the wave box. In the build phase it previews the next wave (its counts, packs
//!   and how much tougher it is) above a "Start wave" button; in the wave phase it shows how far
//!   the wave has come, above the speed button.
//! - A label on the map by each nest that will spawn, with what's coming out of it (build phase).
//! - A banner when a wave is cleared.
//! - The game-over screen: waves survived, the best run this session, and a Restart button.

use super::FONT_SIZE;
use crate::camera::MainCamera;
use crate::horde::{BruteQueue, Health, SpawnQueue};
use crate::map::Landmarks;
use crate::palette;
use crate::tuning::juice::{GAME_OVER_DELAY, WAVE_BANNER_SECONDS};
use crate::waves::plan::{Run, Wave};
use crate::waves::{ActiveWave, CurrentWave, FlowButton, GameSpeed, Phase, WaveCleared};
use bevy::prelude::*;

const TITLE_FONT_SIZE: f32 = 20.0;
const BANNER_FONT_SIZE: f32 = 34.0;
const GAME_OVER_FONT_SIZE: f32 = 40.0;
const LABEL_FONT_SIZE: f32 = 13.0;
const BOX_WIDTH: f32 = 230.0;
/// Where the "wave cleared" banner sits, from the top of the screen.
const BANNER_TOP: Val = Val::Percent(22.0);

pub struct WaveHudPlugin;

impl Plugin for WaveHudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (
                spawn_wave_box,
                spawn_nest_labels,
                spawn_banner,
                spawn_game_over,
            ),
        )
        .add_systems(PreUpdate, press_buttons.after(bevy::ui::UiSystems::Focus))
        .add_systems(OnEnter(Phase::GameOver), reset_game_over)
        .add_systems(
            Update,
            (
                show_wave_box,
                show_nest_labels,
                show_banner,
                show_game_over,
                hover_buttons,
            ),
        );
    }
}

#[derive(Component)]
struct WaveBox;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum WaveText {
    Title,
    Counts,
    Details,
    Button,
}

/// The button under the wave box: Start wave in the build phase, the game speed in the wave.
#[derive(Component)]
struct WaveButton;

#[derive(Component)]
struct RestartButton;

fn text(value: &str, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(value),
        TextFont::from_font_size(size),
        TextColor(color),
        Pickable::IGNORE,
    )
}

fn button_node() -> Node {
    Node {
        padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
        margin: UiRect::top(Val::Px(6.0)),
        border: UiRect::all(Val::Px(2.0)),
        border_radius: BorderRadius::all(Val::Px(4.0)),
        justify_content: JustifyContent::Center,
        ..default()
    }
}

fn spawn_wave_box(mut commands: Commands) {
    commands
        .spawn((
            WaveBox,
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(14.0),
                bottom: Val::Px(12.0),
                width: Val::Px(BOX_WIDTH),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Stretch,
                row_gap: Val::Px(2.0),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|wave| {
            wave.spawn((
                WaveText::Title,
                text("", TITLE_FONT_SIZE, palette::BANNER_TEXT),
            ));
            wave.spawn((WaveText::Counts, text("", FONT_SIZE, palette::HINT_TEXT)));
            wave.spawn((WaveText::Details, text("", FONT_SIZE, palette::HINT_TEXT)));
            wave.spawn((
                WaveButton,
                Button,
                button_node(),
                BackgroundColor(palette::START_BUTTON),
                BorderColor::all(palette::PANEL_CHOSEN),
            ))
            .with_child((WaveText::Button, text("", FONT_SIZE, palette::BANNER_TEXT)));
        });
}

/// "120 swarmers, 2 brutes".
fn counts(swarmers: u32, brutes: u32) -> String {
    let plural = |count: u32, one: &str, many: &str| {
        format!("{count} {}", if count == 1 { one } else { many })
    };
    match (swarmers, brutes) {
        (_, 0) => plural(swarmers, "swarmer", "swarmers"),
        (0, _) => plural(brutes, "brute", "brutes"),
        _ => format!(
            "{}, {}",
            plural(swarmers, "swarmer", "swarmers"),
            plural(brutes, "brute", "brutes")
        ),
    }
}

/// What the wave box says about a wave that hasn't started yet.
fn preview(wave: &Wave) -> [String; 4] {
    let title = if wave.heavy {
        format!("Wave {}  (heavy)", wave.number)
    } else {
        format!("Wave {}", wave.number)
    };
    let tougher = ((wave.toughness - 1.0) * 100.0).round() as i32;
    let details = if tougher > 0 {
        format!("{} packs, enemy HP +{tougher}%", wave.packs)
    } else {
        format!("{} packs", wave.packs)
    };
    [
        title,
        counts(wave.swarmers, wave.brutes),
        details,
        format!("Start wave {}  (Space)", wave.number),
    ]
}

fn show_wave_box(
    phase: Res<State<Phase>>,
    wave: Res<CurrentWave>,
    active: Res<ActiveWave>,
    speed: Res<GameSpeed>,
    swarmers: Res<SpawnQueue>,
    brutes: Res<BruteQueue>,
    enemies: Query<(), With<Health>>,
    mut wave_box: Single<&mut Visibility, With<WaveBox>>,
    mut texts: Query<(&WaveText, &mut Text, &mut TextColor)>,
) {
    let lines = match phase.get() {
        Phase::Build => preview(&wave.0),
        Phase::Wave => {
            let (released, packs) = active.0.progress();
            let left = enemies.iter().len() as u32 + swarmers.waiting + brutes.waiting;
            [
                format!("Wave {}", wave.0.number),
                format!("Pack {} of {packs}", released.max(1)),
                format!("{left} left"),
                format!("Speed {}x  (Tab)", speed.factor()),
            ]
        }
        Phase::GameOver => {
            wave_box.set_if_neq(Visibility::Hidden);
            return;
        }
    };
    wave_box.set_if_neq(Visibility::Inherited);
    for (line, mut text, mut color) in &mut texts {
        let index = match line {
            WaveText::Title => 0,
            WaveText::Counts => 1,
            WaveText::Details => 2,
            WaveText::Button => 3,
        };
        if text.0 != lines[index] {
            text.0 = lines[index].clone();
        }
        if *line == WaveText::Title {
            color.set_if_neq(TextColor(if wave.0.heavy {
                palette::HEAVY_WAVE
            } else {
                palette::BANNER_TEXT
            }));
        }
    }
}

fn press_buttons(
    phase: Res<State<Phase>>,
    wave_button: Query<&Interaction, (Changed<Interaction>, With<WaveButton>)>,
    restart: Query<&Interaction, (Changed<Interaction>, With<RestartButton>)>,
    mut flow: MessageWriter<FlowButton>,
) {
    if wave_button.iter().any(|i| *i == Interaction::Pressed) {
        match phase.get() {
            Phase::Build => {
                flow.write(FlowButton::StartWave);
            }
            Phase::Wave => {
                flow.write(FlowButton::Speed);
            }
            Phase::GameOver => {}
        }
    }
    if restart.iter().any(|i| *i == Interaction::Pressed) {
        flow.write(FlowButton::Restart);
    }
}

fn hover_buttons(
    mut buttons: Query<
        (&Interaction, &mut BackgroundColor),
        Or<(With<WaveButton>, With<RestartButton>)>,
    >,
) {
    for (interaction, mut background) in &mut buttons {
        background.set_if_neq(BackgroundColor(match interaction {
            Interaction::None => palette::START_BUTTON,
            _ => palette::START_BUTTON_HOVER,
        }));
    }
}

/// A label on the map by a nest, saying what will come out of it.
#[derive(Component)]
struct NestLabel(usize);

fn spawn_nest_labels(mut commands: Commands, landmarks: Res<Landmarks>) {
    for index in 0..landmarks.nests.len() {
        commands.spawn((
            NestLabel(index),
            Node {
                position_type: PositionType::Absolute,
                padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)),
                border_radius: BorderRadius::all(Val::Px(3.0)),
                ..default()
            },
            BackgroundColor(palette::PANEL),
            text("", LABEL_FONT_SIZE, palette::PATH_PREVIEW),
            Visibility::Hidden,
        ));
    }
}

fn show_nest_labels(
    time: Res<Time<Real>>,
    phase: Res<State<Phase>>,
    wave: Res<CurrentWave>,
    landmarks: Res<Landmarks>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut labels: Query<(
        &NestLabel,
        &mut Node,
        &mut Text,
        &mut TextColor,
        &mut Visibility,
        &ComputedNode,
    )>,
) {
    let (camera, transform) = *camera;
    let nests = landmarks.nests.len().max(1) as u32;
    for (label, mut node, mut text, mut color, mut visibility, computed) in &mut labels {
        let Some(nest) = landmarks.nests.get(label.0) else {
            continue;
        };
        // Beside the nest, clear of the path preview's arrows coming out of it.
        let at = nest.center() + Vec3::new(nest.size as f32 / 2.0 + 0.5, 0.0, 0.0);
        let shown = *phase.get() == Phase::Build;
        let Some(at) = shown
            .then(|| camera.world_to_viewport(transform, at).ok())
            .flatten()
        else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        // The swarmers pick a nest at random each, so each nest gets about its share.
        let value = format!(
            "{} coming",
            counts(wave.0.swarmers / nests, wave.0.brutes / nests)
        );
        if text.0 != value {
            text.0 = value;
        }
        let size = computed.size() * computed.inverse_scale_factor();
        node.left = Val::Px(at.x);
        node.top = Val::Px(at.y - size.y / 2.0);
        let pulse = 0.75 + 0.25 * (time.elapsed_secs() * 3.0).sin();
        color.0 = palette::PATH_PREVIEW.with_alpha(pulse);
        visibility.set_if_neq(Visibility::Inherited);
    }
}

/// The "wave cleared" banner, and how long it's been up (real seconds).
#[derive(Component, Default)]
struct Banner {
    age: Option<f32>,
}

#[derive(Component)]
struct BannerBonus;

fn spawn_banner(mut commands: Commands) {
    commands
        .spawn((
            Banner::default(),
            Node {
                position_type: PositionType::Absolute,
                top: BANNER_TOP,
                width: Val::Percent(100.0),
                padding: UiRect::vertical(Val::Px(8.0)),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                ..default()
            },
            // A dark band across the screen, so it reads over anything on the map.
            BackgroundColor(palette::PANEL),
            Visibility::Hidden,
            UiTransform::IDENTITY,
            Pickable::IGNORE,
        ))
        .with_children(|banner| {
            banner.spawn(text("", BANNER_FONT_SIZE, palette::BANNER_TEXT));
            banner.spawn((BannerBonus, text("", FONT_SIZE + 2.0, palette::MONEY)));
        });
}

fn show_banner(
    time: Res<Time<Real>>,
    mut cleared: MessageReader<WaveCleared>,
    mut banner: Single<(
        &mut Banner,
        &mut Visibility,
        &mut UiTransform,
        &mut BackgroundColor,
        &Children,
    )>,
    mut texts: Query<(&mut Text, &mut TextColor, Has<BannerBonus>)>,
) {
    let (banner, visibility, transform, band, children) = &mut *banner;
    if let Some(wave) = cleared.read().last() {
        banner.age = Some(0.0);
        for child in children.iter() {
            if let Ok((mut text, _, bonus)) = texts.get_mut(child) {
                text.0 = if bonus {
                    format!("+{} bonus", wave.bonus)
                } else {
                    format!("Wave {} cleared", wave.wave)
                };
            }
        }
    }
    let Some(age) = banner.age.as_mut() else {
        return;
    };
    *age += time.delta_secs();
    let t = *age / WAVE_BANNER_SECONDS;
    if t >= 1.0 {
        banner.age = None;
        visibility.set_if_neq(Visibility::Hidden);
        return;
    }
    visibility.set_if_neq(Visibility::Inherited);
    // Slams in big, settles, and fades out at the end.
    let pop = 1.0 + 0.35 * (1.0 - EaseFunction::QuadraticOut.sample_clamped(t * 6.0));
    transform.scale = Vec2::splat(pop);
    let alpha = 1.0 - EaseFunction::QuadraticIn.sample_clamped((t - 0.7) / 0.3);
    band.0 = palette::PANEL.with_alpha(palette::PANEL.alpha() * alpha);
    for child in children.iter() {
        if let Ok((_, mut color, bonus)) = texts.get_mut(child) {
            let base = if bonus {
                palette::MONEY
            } else {
                palette::BANNER_TEXT
            };
            color.0 = base.with_alpha(alpha);
        }
    }
}

#[derive(Component, Default)]
struct GameOverScreen {
    /// Real seconds since the fall.
    age: f32,
}

#[derive(Component)]
struct GameOverScore;

fn spawn_game_over(mut commands: Commands) {
    commands
        .spawn((
            GameOverScreen::default(),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                // High up, leaving the ruin in the middle of the map in view.
                padding: UiRect::top(Val::Percent(12.0)),
                row_gap: Val::Px(6.0),
                ..default()
            },
            BackgroundColor(palette::GAME_OVER_BACKDROP.with_alpha(0.0)),
            GlobalZIndex(10),
            Visibility::Hidden,
        ))
        .with_children(|screen| {
            screen.spawn(text(
                "The outpost has fallen",
                GAME_OVER_FONT_SIZE,
                palette::GAME_OVER_TITLE,
            ));
            screen.spawn((
                GameOverScore,
                text("", TITLE_FONT_SIZE, palette::BANNER_TEXT),
            ));
            screen
                .spawn((
                    RestartButton,
                    Button,
                    button_node(),
                    BackgroundColor(palette::START_BUTTON),
                    BorderColor::all(palette::PANEL_CHOSEN),
                ))
                .with_child(text("Restart  (R)", FONT_SIZE, palette::BANNER_TEXT));
        });
}

fn reset_game_over(mut screen: Single<&mut GameOverScreen>) {
    screen.age = 0.0;
}

fn show_game_over(
    time: Res<Time<Real>>,
    phase: Res<State<Phase>>,
    run: Res<Run>,
    mut screen: Single<(&mut GameOverScreen, &mut Visibility, &mut BackgroundColor)>,
    mut score: Single<&mut Text, With<GameOverScore>>,
) {
    let (screen, visibility, backdrop) = &mut *screen;
    // It shows up once the blow-up has played out.
    if *phase.get() != Phase::GameOver || screen.age < GAME_OVER_DELAY {
        screen.age += time.delta_secs();
        visibility.set_if_neq(Visibility::Hidden);
        return;
    }
    screen.age += time.delta_secs();
    visibility.set_if_neq(Visibility::Inherited);
    let fade = ((screen.age - GAME_OVER_DELAY) / 0.4).min(1.0);
    let alpha = palette::GAME_OVER_BACKDROP.alpha() * fade;
    backdrop.set_if_neq(BackgroundColor(
        palette::GAME_OVER_BACKDROP.with_alpha(alpha),
    ));
    let waves = |count: u32| {
        if count == 1 {
            "1 wave".to_string()
        } else {
            format!("{count} waves")
        }
    };
    let line = format!(
        "You held out for {}.  Best this session: {}.",
        waves(run.cleared),
        waves(run.best)
    );
    if score.0 != line {
        score.0 = line;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::waves::plan::wave;

    #[test]
    fn previews_read_well() {
        assert_eq!(
            preview(&wave(1)),
            [
                "Wave 1".to_string(),
                "100 swarmers".to_string(),
                "2 packs".to_string(),
                "Start wave 1  (Space)".to_string(),
            ]
        );
        let heavy = preview(&wave(5));
        assert_eq!(heavy[0], "Wave 5  (heavy)");
        assert_eq!(heavy[1], "120 swarmers, 2 brutes");
        assert_eq!(heavy[2], "3 packs, enemy HP +24%");
        assert_eq!(counts(0, 1), "1 brute");
    }
}
