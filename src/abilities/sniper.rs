//! What the sniper shot looks like: a flash on the Command Center's antenna, a white-hot tracer
//! to the target, sparks and a spray of gibs where it hits, a jolt through the Command Center, a
//! moment of hit-stop and a shake. (The damage is dealt where it's fired, in the abilities'
//! `fire`; a brute it hits flinches hard.)

use super::{AbilitySystems, SniperShot};
use crate::command_center::CommandCenterView;
use crate::flow::Class;
use crate::juice::{Fleeting, HitStop, Look, Particles, Shake, Spray, Squash};
use crate::palette;
use crate::tuning::juice::*;
use bevy::{light::NotShadowCaster, prelude::*};

pub struct SniperPlugin;

impl Plugin for SniperPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SniperAssets>()
            .add_systems(Update, show_shots.after(AbilitySystems));
    }
}

#[derive(Resource)]
struct SniperAssets {
    cube: Handle<Mesh>,
    tracer: Handle<StandardMaterial>,
}

impl FromWorld for SniperAssets {
    fn from_world(world: &mut World) -> Self {
        let cube = world
            .resource_mut::<Assets<Mesh>>()
            .add(Cuboid::from_length(1.0));
        let tracer = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial {
                base_color: (palette::SNIPER_TRACER.to_linear() * GLOW_BRIGHTNESS)
                    .with_alpha(1.0)
                    .into(),
                unlit: true,
                ..default()
            });
        Self { cube, tracer }
    }
}

fn show_shots(
    mut commands: Commands,
    mut shots: MessageReader<SniperShot>,
    assets: Res<SniperAssets>,
    mut command_center: Query<&mut Squash, With<CommandCenterView>>,
    mut particles: Particles,
    mut shake: ResMut<Shake>,
    mut hit_stop: ResMut<HitStop>,
) {
    for shot in shots.read() {
        let along = shot.to - shot.from;
        let Some(direction) = along.try_normalize() else {
            continue;
        };
        // The flash on the antenna, and the tracer.
        let flash = Vec3::splat(0.35);
        commands.spawn((
            Fleeting::new(SNIPER_TRACER_SECONDS, Look::Shrink(flash)),
            Mesh3d(assets.cube.clone()),
            MeshMaterial3d(assets.tracer.clone()),
            Transform::from_translation(shot.from)
                .looking_to(direction, Vec3::Y)
                .with_scale(flash),
            NotShadowCaster,
        ));
        let tracer = Vec3::new(SNIPER_TRACER_WIDTH, SNIPER_TRACER_WIDTH, along.length());
        commands.spawn((
            Fleeting::new(SNIPER_TRACER_SECONDS, Look::Thin(tracer)),
            Mesh3d(assets.cube.clone()),
            MeshMaterial3d(assets.tracer.clone()),
            Transform::from_translation(shot.from + along / 2.0)
                .looking_to(direction, Vec3::Y)
                .with_scale(tracer),
            NotShadowCaster,
        ));
        // Sparks and gibs thrown on through the target.
        let through = (direction * Vec3::new(1.0, 0.0, 1.0) + Vec3::Y * 0.6).normalize();
        particles.spray(
            &mut commands,
            shot.to,
            Spray {
                colors: &palette::SPARKS,
                count: 14,
                size: 0.07,
                direction: through,
                cone: 0.6,
                speed: (3.0, 7.0),
                gravity: 0.8,
                grow: 1.0,
                lifetime: 0.3,
                glow: true,
            },
        );
        let gibs: &[Color] = match shot.class {
            Class::Swarmer => &palette::GIBS,
            Class::Brute => &palette::BRUTE_GIBS,
        };
        particles.spray(
            &mut commands,
            shot.to,
            Spray {
                colors: gibs,
                count: 10,
                size: GIB_SIZE * 1.4,
                direction: through,
                cone: 0.5,
                speed: (2.0, 5.0),
                gravity: 1.0,
                grow: 1.0,
                lifetime: GIB_LIFETIME * 1.4,
                glow: false,
            },
        );
        for mut squash in &mut command_center {
            squash.pulse(-0.06);
        }
        hit_stop.add(SNIPER_HIT_STOP);
        shake.add(SNIPER_SHAKE);
    }
}
