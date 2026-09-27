//! The keeper's animation: waddling, turning, tool swings, shrugging, and logs on his back.

use super::work::STRIKE_AT;
use super::{
    Backpack, BackpackLog, IncomingLog, Keeper, KeeperArm, KeeperBody, KeeperFoot, OrderKind,
    OrderQueue, Task, Tool,
};
use crate::juice::Tween;
use crate::phase::{GameState, keeper_working};
use bevy::prelude::*;
use std::f32::consts::PI;

/// World units per step, for the walk cycle.
pub const STRIDE: f32 = 0.19;
const TURN_RATE: f32 = 14.0;
/// Arm angles (radians around x): hanging with the tool pointing forward, and wound up overhead.
const ARM_REST: f32 = 0.25;
const ARM_WOUND_UP: f32 = -2.3;
const ARM_HAMMER_WOUND_UP: f32 = -1.5;
const ARM_FOLLOW_THROUGH: f32 = 0.55;

type BodyFilter = (With<KeeperBody>, Without<Keeper>);
type ArmFilter = (With<KeeperArm>, Without<Keeper>, Without<KeeperBody>);
type FootFilter = (Without<Keeper>, Without<KeeperBody>, Without<KeeperArm>);

pub(super) fn animate_keeper(
    time: Res<Time>,
    state: Res<State<GameState>>,
    queue: Res<OrderQueue>,
    mut keeper: Query<(&mut Keeper, &mut Transform)>,
    mut body: Query<&mut Transform, BodyFilter>,
    mut arm: Query<&mut Transform, ArmFilter>,
    mut feet: Query<(&KeeperFoot, &mut Transform), FootFilter>,
    mut tools: Query<(&Tool, &mut Visibility)>,
) {
    let Ok((mut keeper, mut transform)) = keeper.single_mut() else {
        return;
    };
    let (Ok(mut body), Ok(mut arm)) = (body.single_mut(), arm.single_mut()) else {
        return;
    };
    let dt = time.delta_secs();
    let t = time.elapsed_secs();

    let turn = Quat::from_rotation_y(keeper.facing);
    transform.rotation = transform.rotation.slerp(turn, (TURN_RATE * dt).min(1.0));

    // While he waits (a tile is being picked), he stands idle mid-order.
    let paused = !keeper_working(*state.get());
    let walking = !paused && matches!(keeper.task, Task::Walking { .. });
    // Each step is half a cycle of the walk wave: left foot, then right.
    let wave = keeper.walk_phase * PI;
    let (mut bob, mut roll, mut lean) = (0.0, 0.0, 0.0);
    let mut arm_angle = ARM_REST + 0.05 * (t * 2.0).sin();
    if walking {
        bob = wave.sin().abs() * 0.035;
        roll = wave.sin() * 0.09;
        lean = 0.1;
        arm_angle = ARM_REST + wave.sin() * 0.55;
    } else if !paused && let Task::Working { elapsed, swing, .. } = keeper.task {
        let hammer = matches!(queue.started().map(|o| o.kind), Some(OrderKind::Build(_)));
        let wound_up = if hammer {
            ARM_HAMMER_WOUND_UP
        } else {
            ARM_WOUND_UP
        };
        arm_angle = swing_angle((elapsed / swing).fract(), wound_up);
        lean = 0.12 * (arm_angle - ARM_REST).max(0.0);
    } else {
        // Idle: breathe and glance around now and then.
        bob = 0.004 * (t * 2.4).sin();
        roll = 0.02 * (t * 0.9).sin();
    }
    if keeper.shrug > 0.0 {
        keeper.shrug = (keeper.shrug - dt).max(0.0);
        let strength = keeper.shrug / 0.7;
        roll = (t * 22.0).sin() * 0.18 * strength;
        arm_angle = ARM_REST - 1.4 * strength;
    }
    body.translation.y = bob;
    body.rotation = Quat::from_rotation_z(roll) * Quat::from_rotation_x(lean);
    arm.rotation = Quat::from_rotation_x(arm_angle);

    for (foot, mut transform) in &mut feet {
        let (lift, stride) = if walking {
            let side = wave + foot.0 as f32 * PI;
            (side.sin().max(0.0) * 0.045, side.cos() * 0.06)
        } else {
            (0.0, 0.0)
        };
        transform.translation.y = lift;
        transform.translation.z = stride;
    }

    // Hammer for building (including on the way to the site), axe otherwise.
    let tool = match queue.started().map(|order| order.kind) {
        Some(OrderKind::Build(_)) => Tool::Hammer,
        _ => Tool::Axe,
    };
    for (kind, mut visibility) in &mut tools {
        visibility.set_if_neq(if *kind == tool {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

/// The arm angle at `phase` (0..1) of a tool swing: a slow wind-up, a fast strike landing at
/// [`STRIKE_AT`], and a recovery back to rest.
fn swing_angle(phase: f32, wound_up: f32) -> f32 {
    const WIND_UP_END: f32 = 0.62;
    if phase < WIND_UP_END {
        let k = EaseFunction::CubicOut.sample_clamped(phase / WIND_UP_END);
        ARM_REST.lerp(wound_up, k)
    } else if phase < STRIKE_AT {
        let k = EaseFunction::QuadraticIn
            .sample_clamped((phase - WIND_UP_END) / (STRIKE_AT - WIND_UP_END));
        wound_up.lerp(ARM_FOLLOW_THROUGH, k)
    } else {
        let k = EaseFunction::SineInOut.sample_clamped((phase - STRIKE_AT) / (1.0 - STRIKE_AT));
        ARM_FOLLOW_THROUGH.lerp(ARM_REST, k)
    }
}

/// Wood that has landed in the backpack (logs still flying in don't count yet).
pub fn shown_wood(backpack: &Backpack, incoming: usize) -> u32 {
    backpack.wood.saturating_sub(incoming as u32)
}

/// Shows one log on the backpack per piece of wood, popping new ones in.
pub(super) fn show_backpack_logs(
    mut commands: Commands,
    backpack: Res<Backpack>,
    incoming: Query<(), With<IncomingLog>>,
    mut logs: Query<(Entity, &mut BackpackLog, &mut Visibility)>,
) {
    let shown = shown_wood(&backpack, incoming.iter().count()) as usize;
    for (entity, mut log, mut visibility) in &mut logs {
        let show = log.index < shown;
        if show == log.shown {
            continue;
        }
        log.shown = show;
        if show {
            commands
                .entity(entity)
                .insert(Tween::pop_in(Vec3::ONE, 0.0));
        } else {
            commands.entity(entity).remove::<Tween>();
            *visibility = Visibility::Hidden;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swing_starts_and_ends_at_rest_and_strikes_forward() {
        assert!((swing_angle(0.0, ARM_WOUND_UP) - ARM_REST).abs() < 1e-5);
        assert!((swing_angle(0.9999, ARM_WOUND_UP) - ARM_REST).abs() < 1e-3);
        assert!((swing_angle(STRIKE_AT, ARM_WOUND_UP) - ARM_FOLLOW_THROUGH).abs() < 1e-5);
        assert!(swing_angle(0.6, ARM_WOUND_UP) < -2.0);
    }
}
