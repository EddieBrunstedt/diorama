//! The prepare clock and the bell at the top of the screen, and banners announcing each phase.
//! During the wave the clock counts the slimes left instead.

use super::icons::Icons;
use super::{font, paper};
use crate::hearth::HearthHealth;
use crate::juice::Shake;
use crate::palette;
use crate::phase::{BELL_KEY, GameState, PrepareClock, RingBell, WaveCleared, Waves, bell_bonus};
use crate::slimes::{WaveRun, WaveSpec};
use bevy::picking::events::{Click, Out, Over, Pointer, Press, Release};
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use std::f32::consts::PI;

const TOP: f32 = 20.0;
/// The last seconds of the prepare phase tick louder: the clock turns red and punches.
const HURRY_SECONDS: f32 = 5.0;
const PUNCH_TIME: f32 = 0.3;
const FLIP_TIME: f32 = 0.6;
/// How quickly the bell button grows in and shrinks away (per second, exponential).
const BELL_SHOW_RATE: f32 = 12.0;
const RING_TIME: f32 = 1.2;
const BANNER_TOP: f32 = 110.0;
const BANNER_IN: f32 = 0.35;
const BANNER_HOLD: f32 = 1.6;
const BANNER_OUT: f32 = 0.45;
/// Screen shake when a wave starts.
const WAVE_SHAKE: f32 = 0.55;

#[derive(Component)]
pub(super) struct ClockPanel {
    label: Entity,
    time: Entity,
    icon: Entity,
    fill: Entity,
    punch: f32,
    flip: f32,
    shown_second: Option<u32>,
    /// Slimes left in the wave, as last shown.
    shown_left: Option<u32>,
}

#[derive(Component)]
pub(super) struct BellButton {
    icon: Entity,
    bonus: Entity,
    hovered: bool,
    pressed: bool,
    /// 0 = hidden, 1 = fully shown.
    shown: f32,
    ring: f32,
}

#[derive(Component)]
pub(super) struct Banner {
    age: f32,
    texts: Vec<(Entity, Color)>,
}

/// "0:24": whole seconds left, rounded up so the clock shows 0:00 only when time is up.
pub fn format_clock(seconds: f32) -> String {
    let whole = seconds.max(0.0).ceil() as u32;
    format!("{}:{:02}", whole / 60, whole % 60)
}

pub(super) fn spawn_clock(mut commands: Commands, icons: Res<Icons>) {
    let row = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(TOP),
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            Pickable::IGNORE,
        ))
        .id();
    // The clock stays centered; the bell hangs off its right side, so the clock doesn't move
    // when the bell comes and goes.
    let anchor = commands
        .spawn((Node::default(), Pickable::IGNORE, ChildOf(row)))
        .id();

    // The clock: an hourglass, which wave is next, and the time left, with a bar that drains.
    let clock = commands
        .spawn((
            Node {
                padding: UiRect::new(Val::Px(12.0), Val::Px(18.0), Val::Px(7.0), Val::Px(11.0)),
                border: UiRect::all(Val::Px(2.0)),
                border_radius: BorderRadius::all(Val::Px(14.0)),
                column_gap: Val::Px(8.0),
                align_items: AlignItems::Center,
                ..default()
            },
            paper(14.0),
            UiTransform::IDENTITY,
            Pickable::IGNORE,
            ChildOf(anchor),
        ))
        .id();
    let icon = commands
        .spawn((
            ImageNode::new(icons.hourglass.clone()),
            Node {
                width: Val::Px(36.0),
                height: Val::Px(36.0),
                ..default()
            },
            UiTransform::IDENTITY,
            Pickable::IGNORE,
            ChildOf(clock),
        ))
        .id();
    let column = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                min_width: Val::Px(92.0),
                ..default()
            },
            Pickable::IGNORE,
            ChildOf(clock),
        ))
        .id();
    let label = commands
        .spawn((
            Text::default(),
            font(12.0),
            TextColor(palette::UI_INK_SOFT),
            Pickable::IGNORE,
            ChildOf(column),
        ))
        .id();
    let time = commands
        .spawn((
            Text::default(),
            font(26.0),
            TextColor(palette::UI_INK),
            Pickable::IGNORE,
            ChildOf(column),
        ))
        .id();
    let bar = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(14.0),
                right: Val::Px(14.0),
                bottom: Val::Px(5.0),
                height: Val::Px(4.0),
                border_radius: BorderRadius::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(palette::UI_PAPER_DARK),
            Pickable::IGNORE,
            ChildOf(clock),
        ))
        .id();
    let fill = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                border_radius: BorderRadius::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(palette::BRASS),
            Pickable::IGNORE,
            ChildOf(bar),
        ))
        .id();
    commands.entity(clock).insert(ClockPanel {
        label,
        time,
        icon,
        fill,
        punch: 0.0,
        flip: 0.0,
        shown_second: None,
        shown_left: None,
    });

    // The bell: rings in the wave early, for bonus coin.
    let bell = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(100.0),
                top: Val::Px(3.0),
                margin: UiRect::left(Val::Px(12.0)),
                padding: UiRect::axes(Val::Px(12.0), Val::Px(7.0)),
                border: UiRect::all(Val::Px(2.0)),
                border_radius: BorderRadius::all(Val::Px(14.0)),
                column_gap: Val::Px(8.0),
                align_items: AlignItems::Center,
                display: Display::None,
                ..default()
            },
            BackgroundColor(palette::UI_PAPER.with_alpha(0.95)),
            BorderColor::all(palette::BRASS),
            BoxShadow::new(
                Color::BLACK.with_alpha(0.18),
                Val::Px(0.0),
                Val::Px(3.0),
                Val::Px(0.0),
                Val::Px(7.0),
            ),
            UiTransform::from_scale(Vec2::splat(0.01)),
            ChildOf(anchor),
        ))
        .id();
    let bell_icon = commands
        .spawn((
            ImageNode::new(icons.bell.clone()),
            Node {
                width: Val::Px(36.0),
                height: Val::Px(36.0),
                ..default()
            },
            UiTransform::IDENTITY,
            Pickable::IGNORE,
            ChildOf(bell),
        ))
        .id();
    let bell_column = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(2.0),
                ..default()
            },
            Pickable::IGNORE,
            ChildOf(bell),
        ))
        .id();
    let title_row = commands
        .spawn((
            Node {
                column_gap: Val::Px(6.0),
                align_items: AlignItems::Center,
                ..default()
            },
            Pickable::IGNORE,
            ChildOf(bell_column),
        ))
        .id();
    commands.spawn((
        Text::new("Ring bell"),
        font(15.0),
        TextColor(palette::UI_INK),
        Pickable::IGNORE,
        ChildOf(title_row),
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
            ChildOf(title_row),
        ))
        .with_child((
            Text::new(
                format!("{BELL_KEY:?}")
                    .trim_start_matches("Key")
                    .to_string(),
            ),
            font(11.0),
            TextColor(palette::UI_INK_SOFT),
            Pickable::IGNORE,
        ));
    let bonus_row = commands
        .spawn((
            Node {
                column_gap: Val::Px(4.0),
                align_items: AlignItems::Center,
                ..default()
            },
            Pickable::IGNORE,
            ChildOf(bell_column),
        ))
        .id();
    commands.spawn((
        ImageNode::new(icons.coin.clone()),
        Node {
            width: Val::Px(14.0),
            height: Val::Px(14.0),
            ..default()
        },
        Pickable::IGNORE,
        ChildOf(bonus_row),
    ));
    let bonus = commands
        .spawn((
            Text::default(),
            font(12.0),
            TextColor(crate::voxel::shade(palette::BRASS, 0.75)),
            Pickable::IGNORE,
            ChildOf(bonus_row),
        ))
        .id();
    commands
        .entity(bell)
        .insert(BellButton {
            icon: bell_icon,
            bonus,
            hovered: false,
            pressed: false,
            shown: 0.0,
            ring: 0.0,
        })
        .observe(on_bell_over)
        .observe(on_bell_out)
        .observe(on_bell_press)
        .observe(on_bell_release)
        .observe(on_bell_click);
}

pub(super) fn update_clock(
    time: Res<Time>,
    state: Res<State<GameState>>,
    clock: Res<PrepareClock>,
    waves: Res<Waves>,
    run: Res<WaveRun>,
    icon_images: Res<Icons>,
    panel: Single<(&mut ClockPanel, &mut UiTransform)>,
    mut texts: Query<(&mut Text, &mut TextColor)>,
    mut nodes: Query<(&mut Node, &mut BackgroundColor)>,
    mut icons: Query<(&mut UiTransform, &mut ImageNode), Without<ClockPanel>>,
) {
    let (mut panel, mut transform) = panel.into_inner();
    let dt = time.delta_secs();
    let state = *state.get();
    let preparing = matches!(state, GameState::Intro | GameState::Prepare);
    let hurry = state == GameState::Prepare && clock.left <= HURRY_SECONDS;

    let slimes_left = |left: u32| match left {
        0 => "Cleared!".to_string(),
        1 => "1 slime".to_string(),
        left => format!("{left} slimes"),
    };
    let (label, text, color) = match state {
        GameState::Intro | GameState::Prepare => (
            format!("WAVE {} IN", waves.current()),
            format_clock(clock.left),
            if hurry {
                palette::PROBLEM
            } else {
                palette::UI_INK
            },
        ),
        GameState::Wave => (
            format!("WAVE {}", waves.current()),
            slimes_left(run.left()),
            palette::WAVE_HOLD,
        ),
        GameState::GameOver => (
            format!("WAVE {}", waves.current()),
            "Lost".to_string(),
            palette::PROBLEM,
        ),
    };
    // Punch on every second of the last few, and on every slime gone during the wave.
    let second = clock.left.ceil() as u32;
    if hurry && panel.shown_second.is_some_and(|shown| shown != second) {
        panel.punch = PUNCH_TIME;
    }
    panel.shown_second = Some(second);
    let left = (state == GameState::Wave).then(|| run.left());
    if left.is_some() && panel.shown_left.is_some() && left != panel.shown_left {
        panel.punch = PUNCH_TIME;
    }
    panel.shown_left = left;
    for (entity, value, color) in [
        (panel.label, label, palette::UI_INK_SOFT),
        (panel.time, text, color),
    ] {
        if let Ok((mut text, mut text_color)) = texts.get_mut(entity) {
            if text.0 != value {
                text.0 = value;
            }
            text_color.set_if_neq(TextColor(color));
        }
    }
    if let Ok((mut node, mut background)) = nodes.get_mut(panel.fill) {
        let fraction = match state {
            GameState::Intro | GameState::Prepare => clock.left / clock.total.max(1e-3),
            GameState::Wave => run.left() as f32 / run.spec.count.max(1) as f32,
            GameState::GameOver => 0.0,
        };
        node.width = Val::Percent(fraction * 100.0);
        background.set_if_neq(BackgroundColor(match state {
            GameState::Wave => palette::SLIME,
            _ if preparing && !hurry => palette::BRASS,
            _ => palette::PROBLEM,
        }));
    }

    panel.punch = (panel.punch - dt).max(0.0);
    panel.flip = (panel.flip - dt).max(0.0);
    let k = panel.punch / PUNCH_TIME;
    let scale = 1.0 + 0.14 * k * (k * PI).sin().max(0.0) * 2.0;
    transform.set_if_neq(UiTransform::from_scale(Vec2::splat(scale)));
    if let Ok((mut icon, mut image)) = icons.get_mut(panel.icon) {
        // The hourglass flips over when a prepare phase starts, and shivers when time is short.
        // During the wave a slime takes its place and bounces.
        let wanted = if preparing {
            &icon_images.hourglass
        } else {
            &icon_images.slime
        };
        if image.image != *wanted {
            image.image = wanted.clone();
        }
        let t = time.elapsed_secs();
        if preparing {
            let flip = PI * EaseFunction::BackOut.sample_clamped(1.0 - panel.flip / FLIP_TIME);
            let shiver = if hurry { 0.12 * (t * 30.0).sin() } else { 0.0 };
            icon.rotation = Rot2::radians(PI - flip + shiver);
            icon.scale = Vec2::ONE;
        } else {
            let bounce = (t * 6.0).sin().abs();
            icon.rotation = Rot2::IDENTITY;
            icon.scale = Vec2::new(1.0 + 0.08 * (1.0 - bounce), 1.0 + 0.08 * bounce);
        }
    }
}

pub(super) fn update_bell(
    time: Res<Time>,
    state: Res<State<GameState>>,
    clock: Res<PrepareClock>,
    bell: Single<(
        &mut BellButton,
        &mut Node,
        &mut UiTransform,
        &mut BorderColor,
    )>,
    mut texts: Query<&mut Text>,
    mut icons: Query<&mut UiTransform, Without<BellButton>>,
) {
    let (mut bell, mut node, mut transform, mut border) = bell.into_inner();
    let dt = time.delta_secs();
    let t = time.elapsed_secs();
    let wanted = if *state.get() == GameState::Prepare {
        1.0
    } else {
        0.0
    };
    bell.shown = bell.shown.lerp(wanted, 1.0 - (-BELL_SHOW_RATE * dt).exp());
    if wanted == 0.0 && bell.shown < 0.02 {
        bell.shown = 0.0;
    }
    let display = if bell.shown > 0.0 || wanted > 0.0 {
        Display::Flex
    } else {
        Display::None
    };
    if node.display != display {
        node.display = display;
    }
    bell.ring = (bell.ring - dt).max(0.0);

    let pop = if wanted > 0.0 {
        EaseFunction::BackOut.sample_clamped(bell.shown)
    } else {
        bell.shown
    };
    let mut scale = pop.max(0.01);
    if bell.pressed {
        scale *= 0.94;
    } else if bell.hovered {
        scale *= 1.05;
    }
    transform.set_if_neq(UiTransform::from_scale(Vec2::splat(scale)));
    border.set_if_neq(BorderColor::all(if bell.hovered {
        palette::FLAG
    } else {
        palette::BRASS
    }));

    let bonus = format!("+{} coin", bell_bonus(clock.left));
    if let Ok(mut text) = texts.get_mut(bell.bonus)
        && text.0 != bonus
    {
        text.0 = bonus;
    }
    if let Ok(mut icon) = icons.get_mut(bell.icon) {
        // Swings hard when rung, jiggles when hovered, and sways now and then to be noticed.
        let k = bell.ring / RING_TIME;
        let swing = if k > 0.0 {
            0.6 * k * (t * 24.0).sin()
        } else if bell.hovered {
            0.15 * (t * 18.0).sin()
        } else {
            let every = (t * 0.5).fract();
            0.12 * (every * 40.0).sin() * (1.0 - every * 5.0).max(0.0)
        };
        icon.rotation = Rot2::radians(swing);
    }
}

fn on_bell_over(over: On<Pointer<Over>>, mut bells: Query<&mut BellButton>) {
    if let Ok(mut bell) = bells.get_mut(over.entity) {
        bell.hovered = true;
    }
}

fn on_bell_out(out: On<Pointer<Out>>, mut bells: Query<&mut BellButton>) {
    if let Ok(mut bell) = bells.get_mut(out.entity) {
        bell.hovered = false;
        bell.pressed = false;
    }
}

fn on_bell_press(press: On<Pointer<Press>>, mut bells: Query<&mut BellButton>) {
    if press.event.button == PointerButton::Primary
        && let Ok(mut bell) = bells.get_mut(press.entity)
    {
        bell.pressed = true;
    }
}

fn on_bell_release(release: On<Pointer<Release>>, mut bells: Query<&mut BellButton>) {
    if let Ok(mut bell) = bells.get_mut(release.entity) {
        bell.pressed = false;
    }
}

fn on_bell_click(click: On<Pointer<Click>>, mut rings: MessageWriter<RingBell>) {
    if click.event.button == PointerButton::Primary {
        rings.write(RingBell);
    }
}

pub(super) fn announce_prepare(
    commands: Commands,
    waves: Res<Waves>,
    banners: Query<&mut Banner>,
    mut panel: Single<&mut ClockPanel>,
) {
    panel.flip = FLIP_TIME;
    spawn_banner(
        commands,
        banners,
        "Prepare!",
        palette::UI_PAPER,
        None,
        format!("Wave {} comes when the sand runs out", waves.current()),
    );
}

pub(super) fn announce_wave(
    commands: Commands,
    icons: Res<Icons>,
    waves: Res<Waves>,
    banners: Query<&mut Banner>,
    mut shake: ResMut<Shake>,
    mut bell: Single<&mut BellButton>,
) {
    shake.add(WAVE_SHAKE);
    let subtitle = match waves.bell_bonus {
        Some(bonus) => {
            bell.ring = RING_TIME;
            format!("+{bonus} coin for ringing early")
        }
        None => format!(
            "{} slimes are coming down the road",
            WaveSpec::for_wave(waves.current()).count
        ),
    };
    let icon = waves.bell_bonus.map(|_| icons.coin.clone());
    spawn_banner(
        commands,
        banners,
        &format!("Wave {}!", waves.current()),
        palette::SLIME,
        icon,
        subtitle,
    );
}

/// A banner when a wave is survived.
pub(super) fn announce_cleared(
    commands: Commands,
    icons: Res<Icons>,
    health: Res<HearthHealth>,
    mut cleared: MessageReader<WaveCleared>,
    banners: Query<&mut Banner>,
) {
    let Some(&WaveCleared(number)) = cleared.read().last() else {
        return;
    };
    let subtitle = if health.hp == health.max {
        "Not a scratch on the hearth".to_string()
    } else {
        format!("The hearth held: {}/{}", health.hp, health.max)
    };
    spawn_banner(
        commands,
        banners,
        &format!("Wave {number} survived!"),
        palette::FLAG,
        Some(icons.hearth.clone()),
        subtitle,
    );
}

fn spawn_banner(
    mut commands: Commands,
    mut banners: Query<&mut Banner>,
    title: &str,
    color: Color,
    icon: Option<Handle<Image>>,
    subtitle: String,
) {
    // A new banner sends the old one on its way.
    for mut banner in &mut banners {
        banner.age = banner.age.max(BANNER_IN + BANNER_HOLD);
    }
    let shadow = |alpha: f32| TextShadow {
        offset: Vec2::new(2.0, 3.0),
        color: palette::UI_INK.with_alpha(alpha),
    };
    let banner = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(BANNER_TOP),
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(2.0),
                ..default()
            },
            UiTransform::from_scale(Vec2::splat(0.01)),
            GlobalZIndex(12),
            Pickable::IGNORE,
        ))
        .id();
    let title = commands
        .spawn((
            Text::new(title),
            font(48.0),
            TextColor(color),
            shadow(0.75),
            Pickable::IGNORE,
            ChildOf(banner),
        ))
        .id();
    let row = commands
        .spawn((
            Node {
                column_gap: Val::Px(6.0),
                align_items: AlignItems::Center,
                ..default()
            },
            Pickable::IGNORE,
            ChildOf(banner),
        ))
        .id();
    if let Some(icon) = icon {
        commands.spawn((
            ImageNode::new(icon),
            Node {
                width: Val::Px(24.0),
                height: Val::Px(24.0),
                ..default()
            },
            Pickable::IGNORE,
            ChildOf(row),
        ));
    }
    let subtitle = commands
        .spawn((
            Text::new(subtitle),
            font(20.0),
            TextColor(palette::UI_PAPER),
            shadow(0.75),
            Pickable::IGNORE,
            ChildOf(row),
        ))
        .id();
    commands.entity(banner).insert(Banner {
        age: 0.0,
        texts: vec![(title, color), (subtitle, palette::UI_PAPER)],
    });
}

pub(super) fn animate_banners(
    mut commands: Commands,
    time: Res<Time>,
    mut banners: Query<(Entity, &mut Banner, &mut UiTransform)>,
    mut texts: Query<(&mut TextColor, &mut TextShadow)>,
    mut images: Query<&mut ImageNode>,
    children: Query<&Children>,
) {
    for (entity, mut banner, mut transform) in &mut banners {
        banner.age += time.delta_secs();
        let age = banner.age;
        if age >= BANNER_IN + BANNER_HOLD + BANNER_OUT {
            commands.entity(entity).despawn();
            continue;
        }
        let pop = EaseFunction::BackOut.sample_clamped(age / BANNER_IN);
        let out = ((age - BANNER_IN - BANNER_HOLD) / BANNER_OUT).clamp(0.0, 1.0);
        let rise = -36.0 * EaseFunction::QuadraticIn.sample_clamped(out);
        // A slow breath while it's up.
        let breathe = 1.0 + 0.02 * (age * 4.0).sin();
        *transform = UiTransform {
            translation: Val2::new(Val::Px(0.0), Val::Px(rise)),
            scale: Vec2::splat((pop * breathe).max(0.01)),
            rotation: Rot2::radians(0.04 * (1.0 - pop.min(1.0))),
        };
        let alpha = 1.0 - out;
        for &(text, color) in &banner.texts {
            if let Ok((mut text_color, mut shadow)) = texts.get_mut(text) {
                text_color.0 = color.with_alpha(alpha);
                shadow.color = palette::UI_INK.with_alpha(0.75 * alpha);
            }
        }
        for child in children.iter_descendants(entity) {
            if let Ok(mut image) = images.get_mut(child) {
                image.color = Color::WHITE.with_alpha(alpha);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_rounds_up_to_whole_seconds() {
        assert_eq!(format_clock(30.0), "0:30");
        assert_eq!(format_clock(24.2), "0:25");
        assert_eq!(format_clock(0.01), "0:01");
        assert_eq!(format_clock(0.0), "0:00");
        assert_eq!(format_clock(75.0), "1:15");
        assert_eq!(format_clock(-2.0), "0:00");
    }
}
