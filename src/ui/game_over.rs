//! The game over screen: the hearth went out, how many waves were survived, and a button to play
//! again. It waits a moment first, so the fire can be seen going out.

use super::icons::Icons;
use super::{font, paper};
use crate::palette;
use crate::phase::{GameState, RESTART_KEY, Restart, Waves};
use bevy::picking::events::{Click, Out, Over, Pointer, Press, Release};
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;

/// Seconds after the fire goes out before the screen shows.
const DELAY: f32 = 1.4;
const POP_TIME: f32 = 0.45;
const FADE_TIME: f32 = 0.6;
const OVERLAY_ALPHA: f32 = 0.35;

#[derive(Component)]
pub(super) struct GameOverScreen {
    /// Seconds since game over, while it's on.
    age: Option<f32>,
    panel: Entity,
    score: Entity,
}

#[derive(Component, Default)]
pub(super) struct PlayAgainButton {
    hovered: bool,
    pressed: bool,
}

/// "3 waves", "1 wave".
pub fn waves_text(waves: u32) -> String {
    match waves {
        1 => "1 wave".to_string(),
        waves => format!("{waves} waves"),
    }
}

pub(super) fn spawn_game_over(mut commands: Commands, icons: Res<Icons>) {
    let overlay = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                display: Display::None,
                ..default()
            },
            BackgroundColor(palette::UI_INK.with_alpha(0.0)),
            GlobalZIndex(30),
        ))
        .id();
    let panel = commands
        .spawn((
            Node {
                padding: UiRect::axes(Val::Px(36.0), Val::Px(26.0)),
                border: UiRect::all(Val::Px(2.0)),
                border_radius: BorderRadius::all(Val::Px(18.0)),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(12.0),
                ..default()
            },
            paper(18.0),
            UiTransform::from_scale(Vec2::splat(0.01)),
            Pickable::IGNORE,
            ChildOf(overlay),
        ))
        .id();
    commands.spawn((
        ImageNode::new(icons.hearth.clone()).with_color(Color::WHITE.with_alpha(0.45)),
        Node {
            width: Val::Px(48.0),
            height: Val::Px(48.0),
            ..default()
        },
        Pickable::IGNORE,
        ChildOf(panel),
    ));
    commands.spawn((
        Text::new("The hearth went out"),
        font(38.0),
        TextColor(palette::UI_INK),
        Pickable::IGNORE,
        ChildOf(panel),
    ));
    let row = commands
        .spawn((
            Node {
                column_gap: Val::Px(8.0),
                align_items: AlignItems::Center,
                ..default()
            },
            Pickable::IGNORE,
            ChildOf(panel),
        ))
        .id();
    commands.spawn((
        ImageNode::new(icons.slime.clone()),
        Node {
            width: Val::Px(28.0),
            height: Val::Px(28.0),
            ..default()
        },
        Pickable::IGNORE,
        ChildOf(row),
    ));
    let score = commands
        .spawn((
            Text::default(),
            font(22.0),
            TextColor(palette::UI_INK_SOFT),
            Pickable::IGNORE,
            ChildOf(row),
        ))
        .id();

    let button = commands
        .spawn((
            PlayAgainButton::default(),
            Node {
                margin: UiRect::top(Val::Px(6.0)),
                padding: UiRect::axes(Val::Px(18.0), Val::Px(9.0)),
                border: UiRect::all(Val::Px(2.0)),
                border_radius: BorderRadius::all(Val::Px(14.0)),
                column_gap: Val::Px(8.0),
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(palette::UI_PAPER),
            BorderColor::all(palette::BRASS),
            UiTransform::IDENTITY,
            ChildOf(panel),
        ))
        .observe(on_over)
        .observe(on_out)
        .observe(on_press)
        .observe(on_release)
        .observe(on_click)
        .id();
    commands.spawn((
        Text::new("Play again"),
        font(20.0),
        TextColor(palette::UI_INK),
        Pickable::IGNORE,
        ChildOf(button),
    ));
    commands
        .spawn((
            Node {
                padding: UiRect::axes(Val::Px(5.0), Val::Px(0.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BorderColor::all(palette::UI_INK_SOFT),
            Pickable::IGNORE,
            ChildOf(button),
        ))
        .with_child((
            Text::new(
                format!("{RESTART_KEY:?}")
                    .trim_start_matches("Key")
                    .to_string(),
            ),
            font(12.0),
            TextColor(palette::UI_INK_SOFT),
            Pickable::IGNORE,
        ));

    commands.entity(overlay).insert(GameOverScreen {
        age: None,
        panel,
        score,
    });
}

pub(super) fn update_game_over(
    time: Res<Time>,
    state: Res<State<GameState>>,
    waves: Res<Waves>,
    screen: Single<(&mut GameOverScreen, &mut Node, &mut BackgroundColor)>,
    mut panels: Query<&mut UiTransform, Without<PlayAgainButton>>,
    mut buttons: Query<(&PlayAgainButton, &mut UiTransform, &mut BorderColor)>,
    mut texts: Query<&mut Text>,
) {
    let (mut screen, mut node, mut background) = screen.into_inner();
    if *state.get() != GameState::GameOver {
        screen.age = None;
        if node.display != Display::None {
            node.display = Display::None;
        }
        return;
    }
    let age = screen.age.map_or(0.0, |age| age + time.delta_secs());
    screen.age = Some(age);
    let shown = age - DELAY;
    let display = if shown >= 0.0 {
        Display::Flex
    } else {
        Display::None
    };
    if node.display != display {
        node.display = display;
    }
    if shown < 0.0 {
        return;
    }
    background.0 = palette::UI_INK
        .with_alpha(OVERLAY_ALPHA * EaseFunction::QuadraticOut.sample_clamped(shown / FADE_TIME));
    if let Ok(mut panel) = panels.get_mut(screen.panel) {
        let pop = EaseFunction::BackOut.sample_clamped(shown / POP_TIME);
        let breathe = 1.0 + 0.01 * (age * 3.0).sin();
        *panel = UiTransform {
            scale: Vec2::splat((pop * breathe).max(0.01)),
            rotation: Rot2::radians(-0.05 * (1.0 - pop.min(1.0))),
            ..default()
        };
    }
    let score = format!("You survived {}", waves_text(waves.completed));
    if let Ok(mut text) = texts.get_mut(screen.score)
        && text.0 != score
    {
        text.0 = score;
    }
    for (button, mut transform, mut border) in &mut buttons {
        let scale = if button.pressed {
            0.94
        } else if button.hovered {
            1.06
        } else {
            1.0
        };
        transform.set_if_neq(UiTransform::from_scale(Vec2::splat(scale)));
        border.set_if_neq(BorderColor::all(if button.hovered {
            palette::FLAG
        } else {
            palette::BRASS
        }));
    }
}

fn on_over(over: On<Pointer<Over>>, mut buttons: Query<&mut PlayAgainButton>) {
    if let Ok(mut button) = buttons.get_mut(over.entity) {
        button.hovered = true;
    }
}

fn on_out(out: On<Pointer<Out>>, mut buttons: Query<&mut PlayAgainButton>) {
    if let Ok(mut button) = buttons.get_mut(out.entity) {
        button.hovered = false;
        button.pressed = false;
    }
}

fn on_press(press: On<Pointer<Press>>, mut buttons: Query<&mut PlayAgainButton>) {
    if press.event.button == PointerButton::Primary
        && let Ok(mut button) = buttons.get_mut(press.entity)
    {
        button.pressed = true;
    }
}

fn on_release(release: On<Pointer<Release>>, mut buttons: Query<&mut PlayAgainButton>) {
    if let Ok(mut button) = buttons.get_mut(release.entity) {
        button.pressed = false;
    }
}

fn on_click(click: On<Pointer<Click>>, mut restarts: MessageWriter<Restart>) {
    if click.event.button == PointerButton::Primary {
        restarts.write(Restart);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_waves() {
        assert_eq!(waves_text(0), "0 waves");
        assert_eq!(waves_text(1), "1 wave");
        assert_eq!(waves_text(12), "12 waves");
    }
}
