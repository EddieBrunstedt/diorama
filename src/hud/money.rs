//! The money counter in the top-left corner. Kills send gold pips flying to it (a few a frame),
//! and it pulses as they land. A brute's kill, and a cleared wave, pop their reward over the map.
//! When something can't be paid for, the counter flashes red and shakes.

use crate::camera::MainCamera;
use crate::economy::{CantAfford, EconomySystems, Wallet, WaveCleared, kill_reward};
use crate::flow::Class;
use crate::horde::EnemyKilled;
use crate::map::Landmarks;
use crate::palette;
use crate::tuning::juice::*;
use bevy::prelude::*;

const FONT_SIZE: f32 = 22.0;
const LABEL_FONT_SIZE: f32 = 12.0;
const POP_FONT_SIZE: f32 = 16.0;
/// A pip's size, how high its path bows up (logical pixels), and how far a reward pop rises.
const PIP_SIZE: f32 = 7.0;
const PIP_ARC: f32 = 60.0;
const POP_RISE: f32 = 36.0;
/// How far the counter shakes when it's broke (logical pixels).
const BROKE_SHAKE: f32 = 5.0;

pub struct MoneyPlugin;

impl Plugin for MoneyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_counter).add_systems(
            Update,
            (send_pips, fly_pips, pop_rewards, show_counter)
                .chain()
                .after(EconomySystems),
        );
    }
}

#[derive(Component, Default)]
struct MoneyCounter {
    /// How much it's swelled from pips landing, easing back to 0.
    pulse: f32,
    /// Seconds left of flashing red.
    broke: f32,
}

/// A gold pip flying from a kill (at `from`, on screen) to the counter.
#[derive(Component)]
struct Pip {
    from: Vec2,
    age: f32,
}

/// A reward popping up over the map where it was earned, rising and fading.
#[derive(Component)]
struct RewardPop {
    at: Vec3,
    age: f32,
}

fn spawn_counter(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(14.0),
                top: Val::Px(10.0),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|corner| {
            corner.spawn((
                Text::new("Money"),
                TextFont::from_font_size(LABEL_FONT_SIZE),
                TextColor(palette::HINT_TEXT),
                Pickable::IGNORE,
            ));
            corner.spawn((
                MoneyCounter::default(),
                Text::new(""),
                TextFont::from_font_size(FONT_SIZE),
                TextColor(palette::MONEY),
                UiTransform::IDENTITY,
                Pickable::IGNORE,
            ));
        });
}

/// Where the counter's middle is on screen (logical pixels).
fn counter_position(counter: (&UiGlobalTransform, &ComputedNode)) -> Vec2 {
    let (transform, node) = counter;
    transform.translation * node.inverse_scale_factor()
}

fn send_pips(
    mut commands: Commands,
    mut killed: MessageReader<EnemyKilled>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    pips: Query<(), With<Pip>>,
) {
    let (camera, transform) = *camera;
    let mut alive = pips.iter().len();
    let mut sent = 0;
    for kill in killed.read() {
        if kill.class == Class::Brute {
            commands.spawn(pop(kill.at, kill_reward(kill.class)));
        }
        if sent >= MAX_PIPS_PER_FRAME || alive >= MAX_PIPS {
            continue;
        }
        let Ok(from) = camera.world_to_viewport(transform, kill.at) else {
            continue;
        };
        sent += 1;
        alive += 1;
        commands.spawn((
            Pip { from, age: 0.0 },
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(from.x - PIP_SIZE / 2.0),
                top: Val::Px(from.y - PIP_SIZE / 2.0),
                width: Val::Px(PIP_SIZE),
                height: Val::Px(PIP_SIZE),
                ..default()
            },
            BackgroundColor(palette::MONEY),
            Pickable::IGNORE,
        ));
    }
}

fn pop(at: Vec3, amount: u32) -> impl Bundle {
    (
        RewardPop { at, age: 0.0 },
        Node {
            position_type: PositionType::Absolute,
            ..default()
        },
        Text::new(format!("+{amount}")),
        TextFont::from_font_size(POP_FONT_SIZE),
        TextColor(palette::MONEY),
        Visibility::Hidden,
        Pickable::IGNORE,
    )
}

fn fly_pips(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut pips: Query<(Entity, &mut Pip, &mut Node)>,
    mut counter: Single<(&mut MoneyCounter, &UiGlobalTransform, &ComputedNode)>,
) {
    let (counter, transform, node) = &mut *counter;
    let to = counter_position((transform, node));
    for (entity, mut pip, mut pip_node) in &mut pips {
        pip.age += time.delta_secs();
        let t = pip.age / PIP_SECONDS;
        if t >= 1.0 {
            commands.entity(entity).despawn();
            counter.pulse = 1.0;
            continue;
        }
        let k = EaseFunction::CubicIn.sample_clamped(t);
        let at = pip.from.lerp(to, k) - Vec2::Y * PIP_ARC * (4.0 * t * (1.0 - t));
        pip_node.left = Val::Px(at.x - PIP_SIZE / 2.0);
        pip_node.top = Val::Px(at.y - PIP_SIZE / 2.0);
    }
}

fn pop_rewards(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut cleared: MessageReader<WaveCleared>,
    landmarks: Res<Landmarks>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut pops: Query<(
        Entity,
        &mut RewardPop,
        &mut Node,
        &mut Visibility,
        &mut TextColor,
        &ComputedNode,
    )>,
) {
    for wave in cleared.read() {
        let above = landmarks.command_center.center() + Vec3::Y * 2.0;
        commands.spawn(pop(above, wave.bonus));
    }
    let (camera, transform) = *camera;
    for (entity, mut pop, mut node, mut visibility, mut color, computed) in &mut pops {
        pop.age += time.delta_secs();
        let t = pop.age / REWARD_POP_SECONDS;
        let Ok(at) = camera.world_to_viewport(transform, pop.at) else {
            continue;
        };
        if t >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        visibility.set_if_neq(Visibility::Inherited);
        let size = computed.size() * computed.inverse_scale_factor();
        let rise = EaseFunction::QuadraticOut.sample_clamped(t) * POP_RISE;
        node.left = Val::Px(at.x - size.x / 2.0);
        node.top = Val::Px(at.y - size.y - rise);
        color.0 = palette::MONEY.with_alpha(1.0 - t * t);
    }
}

fn show_counter(
    time: Res<Time<Real>>,
    wallet: Res<Wallet>,
    mut broke: MessageReader<CantAfford>,
    mut counter: Single<(
        &mut MoneyCounter,
        &mut Text,
        &mut TextColor,
        &mut UiTransform,
    )>,
) {
    let dt = time.delta_secs();
    let (counter, text, color, transform) = &mut *counter;
    if broke.read().count() > 0 {
        counter.broke = BROKE_SECONDS;
    }
    counter.broke = (counter.broke - dt).max(0.0);
    counter.pulse = (counter.pulse - dt * 6.0).max(0.0);
    let label = wallet.money().to_string();
    if text.0 != label {
        text.0 = label;
    }
    let shaking = counter.broke > 0.0;
    color.set_if_neq(TextColor(if shaking {
        palette::BROKE
    } else {
        palette::MONEY
    }));
    let shake = if shaking {
        (time.elapsed_secs() * 70.0).sin() * BROKE_SHAKE * counter.broke / BROKE_SECONDS
    } else {
        0.0
    };
    transform.translation = Val2::px(shake, 0.0);
    transform.scale = Vec2::splat(1.0 + counter.pulse * MONEY_PULSE);
}
