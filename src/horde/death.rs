//! Enemies dying: whatever ran out of [`Health`] in the damage step is cleared away and bursts
//! into gibs. A brute goes down hard: a big burst, a heavy shake and a moment of hit-stop (at
//! most one a second, so a heavy wave dying doesn't stutter).

use super::{Brute, DamageSystems, Health};
use crate::flow::Class;
use crate::juice::{Burst, HitStop, Particles, Shake};
use crate::palette;
use crate::tuning::juice::*;
use bevy::prelude::*;

pub struct DeathPlugin;

impl Plugin for DeathPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<EnemyKilled>().add_systems(
            Update,
            (remove_dead, burst_into_gibs)
                .chain()
                .in_set(DeathSystems)
                .after(DamageSystems),
        );
    }
}

/// Clears away the dead. Whatever reacts to kills (money, juice) runs after it.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DeathSystems;

/// An enemy of `class` died at `at`.
#[derive(Message, Clone, Copy, Debug)]
pub struct EnemyKilled {
    pub at: Vec3,
    pub class: Class,
}

fn remove_dead(
    mut commands: Commands,
    enemies: Query<(Entity, &Health, &Transform, Has<Brute>)>,
    mut killed: MessageWriter<EnemyKilled>,
) {
    for (entity, health, transform, brute) in &enemies {
        if health.is_alive() {
            continue;
        }
        commands.entity(entity).despawn();
        let class = if brute { Class::Brute } else { Class::Swarmer };
        killed.write(EnemyKilled {
            at: transform.translation,
            class,
        });
    }
}

fn burst_into_gibs(
    mut commands: Commands,
    real: Res<Time<Real>>,
    mut killed: MessageReader<EnemyKilled>,
    mut particles: Particles,
    mut shake: ResMut<Shake>,
    mut hit_stop: ResMut<HitStop>,
    mut last_stop: Local<Option<f32>>,
) {
    // A few bursts a frame read as a massacre; a whole pile bursting at once would flood the map.
    let mut bursts = 0;
    for kill in killed.read() {
        match kill.class {
            Class::Swarmer => {
                if bursts >= MAX_DEATH_BURSTS {
                    continue;
                }
                bursts += 1;
                particles.burst(
                    &mut commands,
                    kill.at + Vec3::Y * 0.12,
                    0.05,
                    Burst {
                        colors: &palette::GIBS,
                        count: GIB_COUNT,
                        size: GIB_SIZE,
                        spread: GIB_SPREAD,
                        lift: GIB_LIFT,
                        lifetime: GIB_LIFETIME,
                    },
                );
            }
            Class::Brute => {
                particles.burst(
                    &mut commands,
                    kill.at + Vec3::Y * 0.4,
                    0.4,
                    Burst {
                        colors: &palette::BRUTE_GIBS,
                        count: BRUTE_GIB_COUNT,
                        size: GIB_SIZE * 2.2,
                        spread: GIB_SPREAD * 1.5,
                        lift: GIB_LIFT * 1.3,
                        lifetime: GIB_LIFETIME * 1.8,
                    },
                );
                shake.add(BRUTE_DEATH_SHAKE);
                let now = real.elapsed_secs();
                if last_stop.is_none_or(|last| now - last >= BRUTE_HIT_STOP_COOLDOWN) {
                    *last_stop = Some(now);
                    hit_stop.add(BRUTE_DEATH_HIT_STOP);
                }
            }
        }
    }
}
