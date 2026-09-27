//! Getting into the catapult and out again. At the end of a "man" order the keeper hops up onto
//! the catapult's step and stays there, turning with it, until he's called out: right-click or
//! Esc (the order is done), removing its card (cancelled), or the last slime of the wave going.
//! Then he hops back down to where he got in and goes on with the queue.

use super::{Keeper, KeeperBody, OrderKind, OrderQueue, Task};
use crate::catapult::{Catapult, LeaveCatapult, Manned, seat};
use crate::juice::{Burst, Particles, Squash, Tween};
use crate::palette;
use crate::phase::GameState;
use crate::slimes::WaveRun;
use crate::ui::OrderFinished;
use bevy::prelude::*;

/// Seconds to hop up into the catapult, and down again.
const BOARD_TIME: f32 = 0.35;
const LEAVE_TIME: f32 = 0.3;
const HOP_HEIGHT: f32 = 0.22;

pub(super) fn work_catapult_seat(
    mut commands: Commands,
    time: Res<Time>,
    state: Res<State<GameState>>,
    run: Res<WaveRun>,
    mut queue: ResMut<OrderQueue>,
    keeper: Single<(&mut Keeper, &mut Transform), Without<Tween>>,
    mut body: Single<&mut Squash, With<KeeperBody>>,
    catapults: Query<(&GlobalTransform, Has<Manned>), With<Catapult>>,
    mut leave: MessageReader<LeaveCatapult>,
    mut finished: MessageWriter<OrderFinished>,
    mut particles: Particles,
) {
    let (mut keeper, mut transform) = keeper.into_inner();
    let dt = time.delta_secs();
    let asked_out = leave.read().count() > 0;
    // The wave is over (or never was): out he gets.
    let over = *state.get() != GameState::Wave || run.cleared();
    let started = queue.started().copied();

    // On his way to the catapult as the last slime goes: never mind.
    if over
        && matches!(keeper.task, Task::Walking { .. })
        && let Some(order) = started.filter(|order| matches!(order.kind, OrderKind::Man(_)))
    {
        finished.write(OrderFinished(order.id));
        queue.finish();
        keeper.task = Task::Idle;
        return;
    }

    match keeper.task.clone() {
        Task::Manning {
            order,
            catapult,
            stand,
            board,
        } => {
            let Ok((catapult_transform, manned)) = catapults.get(catapult) else {
                keeper.task = Task::Idle;
                return;
            };
            let removed = started.map(|started| started.id) != Some(order);
            if removed || asked_out || over {
                if !removed {
                    finished.write(OrderFinished(order));
                    queue.finish();
                }
                commands.entity(catapult).remove::<Manned>();
                keeper.task = Task::Leaving {
                    from: transform.translation,
                    to: stand,
                    t: 0.0,
                };
                return;
            }
            let seat = seat(catapult_transform);
            if board < 1.0 {
                let k = (board + dt / BOARD_TIME).min(1.0);
                keeper.task = Task::Manning {
                    order,
                    catapult,
                    stand,
                    board: k,
                };
                let from = Vec3::new(stand.x, 0.0, stand.y);
                transform.translation = hop(from, seat, k);
                keeper.face((seat - from).xz());
                if k >= 1.0 {
                    body.pulse(-0.25);
                    commands.entity(catapult).insert(Manned);
                    particles.burst(
                        &mut commands,
                        seat,
                        Burst {
                            colors: &[palette::PLANK, palette::DIRT],
                            count: 6,
                            size: 0.035,
                            spread: 0.8,
                            lift: 0.8,
                            lifetime: 0.4,
                        },
                    );
                }
                return;
            }
            // In: he stands on the step and turns with the catapult.
            transform.translation = seat;
            let forward = catapult_transform.rotation() * Vec3::Z;
            keeper.face(forward.xz());
            if !manned {
                commands.entity(catapult).insert(Manned);
            }
        }
        Task::Leaving { from, to, t } => {
            let k = (t + dt / LEAVE_TIME).min(1.0);
            transform.translation = hop(from, Vec3::new(to.x, 0.0, to.y), k);
            keeper.task = if k >= 1.0 {
                body.pulse(-0.2);
                Task::Idle
            } else {
                Task::Leaving { from, to, t: k }
            };
        }
        _ => {}
    }
}

/// A little hop from `from` to `to`, at `k` (0 to 1) of the way.
fn hop(from: Vec3, to: Vec3, k: f32) -> Vec3 {
    from.lerp(to, k) + Vec3::Y * HOP_HEIGHT * 4.0 * k * (1.0 - k)
}
