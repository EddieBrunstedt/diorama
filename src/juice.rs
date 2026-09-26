//! Small in-house animation helpers: tweens with easing, squash & stretch springs, wobbles,
//! particles, and things flying to where they're counted.

use crate::rng::Rng;
use crate::voxel::{VoxelMaterial, VoxelMesh};
use bevy::{ecs::system::SystemParam, light::NotShadowCaster, prelude::*};
use std::collections::HashMap;

/// Longest frame step the springs integrate at once, so a hitch can't make them explode.
const MAX_SPRING_STEP: f32 = 1.0 / 30.0;
const SQUASH_STIFFNESS: f32 = 280.0;
const SQUASH_DAMPING: f32 = 11.0;
const WOBBLE_DECAY: f32 = 6.0;
const WOBBLE_FREQUENCY: f32 = 28.0;
const WOBBLE_LIFETIME: f32 = 0.8;
const PARTICLE_GRAVITY: f32 = 9.0;
/// Particles bounce on the island's surface.
const PARTICLE_FLOOR: f32 = 0.0;

pub struct JuicePlugin;

impl Plugin for JuicePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VoxelMaterial>()
            .init_resource::<CubeCache>()
            .insert_resource(ParticleRng(Rng::new(0x9A27)))
            .add_systems(
                Update,
                (
                    run_tweens,
                    run_squash,
                    run_wobbles,
                    run_particles,
                    run_flights,
                ),
            );
    }
}

#[derive(Clone, Copy, Debug)]
pub enum TweenTarget {
    Translation { from: Vec3, to: Vec3 },
    Scale { from: Vec3, to: Vec3 },
}

/// Animates a transform property once, then removes itself. The entity stays hidden until the
/// tween starts, so spawn it with `Visibility::Hidden` and [`Tween::start_transform`].
#[derive(Component, Clone, Debug)]
pub struct Tween {
    pub target: TweenTarget,
    pub ease: EaseFunction,
    pub delay: f32,
    pub duration: f32,
    elapsed: f32,
}

impl Tween {
    pub fn new(target: TweenTarget, ease: EaseFunction, delay: f32, duration: f32) -> Self {
        Self {
            target,
            ease,
            delay,
            duration,
            elapsed: 0.0,
        }
    }

    /// Falls from `height` above `rest` and bounces into place.
    pub fn drop_in(rest: Vec3, height: f32, delay: f32) -> Self {
        let from = rest + Vec3::Y * height;
        Self::new(
            TweenTarget::Translation { from, to: rest },
            EaseFunction::BounceOut,
            delay,
            0.75,
        )
    }

    /// Grows from nothing to `scale` with a little overshoot.
    pub fn pop_in(scale: Vec3, delay: f32) -> Self {
        // Not exactly zero: a zero scale makes the transform non-invertible.
        let from = Vec3::splat(0.01);
        Self::new(
            TweenTarget::Scale { from, to: scale },
            EaseFunction::BackOut,
            delay,
            0.4,
        )
    }

    /// Seconds until the tween has finished.
    pub fn remaining(&self) -> f32 {
        (self.delay + self.duration - self.elapsed).max(0.0)
    }

    /// The transform the entity should be spawned with (the tween's first frame).
    pub fn start_transform(&self) -> Transform {
        match self.target {
            TweenTarget::Translation { from, .. } => Transform::from_translation(from),
            TweenTarget::Scale { from, .. } => Transform::from_scale(from),
        }
    }
}

fn run_tweens(
    mut commands: Commands,
    time: Res<Time>,
    mut tweens: Query<(Entity, &mut Tween, &mut Transform, &mut Visibility)>,
) {
    for (entity, mut tween, mut transform, mut visibility) in &mut tweens {
        tween.elapsed += time.delta_secs();
        let t = (tween.elapsed - tween.delay) / tween.duration;
        if t < 0.0 {
            continue;
        }
        visibility.set_if_neq(Visibility::Inherited);
        let k = tween.ease.sample_clamped(t);
        match tween.target {
            TweenTarget::Translation { from, to } => transform.translation = from.lerp(to, k),
            TweenTarget::Scale { from, to } => transform.scale = from.lerp(to, k),
        }
        if t >= 1.0 {
            commands.entity(entity).remove::<Tween>();
        }
    }
}

/// Squash & stretch that springs back to rest. Owns the entity's scale.
#[derive(Component, Clone, Debug)]
pub struct Squash {
    pub rest: Vec3,
    /// Current stretch: positive is taller and thinner, negative is squashed flat.
    value: f32,
    velocity: f32,
}

impl Squash {
    pub fn new(rest: Vec3) -> Self {
        Self {
            rest,
            value: 0.0,
            velocity: 0.0,
        }
    }

    /// Squashes (negative) or stretches (positive) right now, then springs back.
    pub fn pulse(&mut self, amount: f32) {
        self.value = amount;
        self.velocity = 0.0;
    }

    fn scale(&self) -> Vec3 {
        let stretch = 1.0 + self.value.max(-0.7);
        let thin = 1.0 / stretch.sqrt();
        self.rest * Vec3::new(thin, stretch, thin)
    }
}

fn run_squash(time: Res<Time>, mut springs: Query<(&mut Squash, &mut Transform)>) {
    let dt = time.delta_secs().min(MAX_SPRING_STEP);
    for (mut spring, mut transform) in &mut springs {
        if spring.value == 0.0 && spring.velocity == 0.0 {
            transform.scale = spring.rest;
            continue;
        }
        let acceleration = -SQUASH_STIFFNESS * spring.value - SQUASH_DAMPING * spring.velocity;
        spring.velocity += acceleration * dt;
        spring.value += spring.velocity * dt;
        if spring.value.abs() < 1e-4 && spring.velocity.abs() < 1e-3 {
            spring.value = 0.0;
            spring.velocity = 0.0;
        }
        transform.scale = spring.scale();
    }
}

/// A decaying rotational shake, e.g. a tree being hit. Removes itself when done.
#[derive(Component, Clone, Debug)]
pub struct Wobble {
    /// The rotation to shake around and return to.
    pub rest: Quat,
    axis: Dir3,
    amplitude: f32,
    age: f32,
}

impl Wobble {
    /// A new shake around `axis`. Pass the entity's current wobble, if any, so a hit during a
    /// shake keeps the true resting rotation.
    pub fn new(
        existing: Option<&Wobble>,
        transform: &Transform,
        axis: Vec3,
        amplitude: f32,
    ) -> Self {
        Self {
            rest: existing.map_or(transform.rotation, |wobble| wobble.rest),
            axis: Dir3::new(axis).unwrap_or(Dir3::X),
            amplitude,
            age: 0.0,
        }
    }
}

fn run_wobbles(
    mut commands: Commands,
    time: Res<Time>,
    mut wobbles: Query<(Entity, &mut Wobble, &mut Transform)>,
) {
    for (entity, mut wobble, mut transform) in &mut wobbles {
        wobble.age += time.delta_secs();
        if wobble.age >= WOBBLE_LIFETIME {
            transform.rotation = wobble.rest;
            commands.entity(entity).remove::<Wobble>();
            continue;
        }
        let angle = wobble.amplitude
            * (-wobble.age * WOBBLE_DECAY).exp()
            * (wobble.age * WOBBLE_FREQUENCY).sin();
        transform.rotation = Quat::from_axis_angle(*wobble.axis, angle) * wobble.rest;
    }
}

/// Shared vertex-colored unit cubes, one per color, for particles and small props.
#[derive(Resource, Default)]
pub struct CubeCache(HashMap<[u8; 4], Handle<Mesh>>);

#[derive(Resource)]
pub struct ParticleRng(pub Rng);

/// A voxel chunk flying out of something, bouncing and shrinking away.
#[derive(Component, Clone, Debug)]
pub struct Particle {
    velocity: Vec3,
    spin: Vec3,
    age: f32,
    lifetime: f32,
    size: f32,
}

/// How a burst of particles looks and moves.
#[derive(Clone, Copy, Debug)]
pub struct Burst<'a> {
    pub colors: &'a [Color],
    pub count: usize,
    /// Size of each chunk in world units.
    pub size: f32,
    /// Horizontal speed.
    pub spread: f32,
    /// Upward speed.
    pub lift: f32,
    pub lifetime: f32,
}

/// Spawns particles and other small voxel bits.
#[derive(SystemParam)]
pub struct Particles<'w> {
    meshes: ResMut<'w, Assets<Mesh>>,
    cache: ResMut<'w, CubeCache>,
    material: Res<'w, VoxelMaterial>,
    rng: ResMut<'w, ParticleRng>,
}

impl Particles<'_> {
    /// The mesh assets, for systems that spawn other meshes alongside particles.
    pub fn meshes(&mut self) -> &mut Assets<Mesh> {
        &mut self.meshes
    }

    /// A unit cube in `color` using the shared voxel material.
    pub fn cube(&mut self, color: Color) -> (Mesh3d, MeshMaterial3d<StandardMaterial>) {
        let key = color.to_srgba().to_u8_array();
        let meshes = &mut self.meshes;
        let mesh = self
            .cache
            .0
            .entry(key)
            .or_insert_with(|| {
                let mut mesh = VoxelMesh::default();
                // Voxel units: 10 voxels make one world unit.
                mesh.cuboid(Vec3::splat(-5.0), Vec3::splat(5.0), color);
                meshes.add(mesh.build())
            })
            .clone();
        (Mesh3d(mesh), MeshMaterial3d(self.material.0.clone()))
    }

    pub fn burst(&mut self, commands: &mut Commands, at: Vec3, burst: Burst) {
        for _ in 0..burst.count {
            let rng = &mut self.rng.0;
            let angle = rng.range(0.0, std::f32::consts::TAU);
            let speed = burst.spread * rng.range(0.4, 1.0);
            let velocity = Vec3::new(
                angle.cos() * speed,
                burst.lift * rng.range(0.6, 1.2),
                angle.sin() * speed,
            );
            let spin = Vec3::new(
                rng.range(-9.0, 9.0),
                rng.range(-9.0, 9.0),
                rng.range(-9.0, 9.0),
            );
            let size = burst.size * rng.range(0.7, 1.2);
            let lifetime = burst.lifetime * rng.range(0.7, 1.2);
            let color = rng.pick(burst.colors);
            commands.spawn((
                Particle {
                    velocity,
                    spin,
                    age: 0.0,
                    lifetime,
                    size,
                },
                self.cube(color),
                Transform::from_translation(at).with_scale(Vec3::splat(size)),
                NotShadowCaster,
            ));
        }
    }
}

fn run_particles(
    mut commands: Commands,
    time: Res<Time>,
    mut particles: Query<(Entity, &mut Particle, &mut Transform)>,
) {
    let dt = time.delta_secs().min(MAX_SPRING_STEP);
    for (entity, mut particle, mut transform) in &mut particles {
        particle.age += dt;
        let life = particle.age / particle.lifetime;
        if life >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        particle.velocity.y -= PARTICLE_GRAVITY * dt;
        transform.translation += particle.velocity * dt;
        let bottom = PARTICLE_FLOOR + particle.size / 2.0;
        if transform.translation.y < bottom && particle.velocity.y < 0.0 {
            transform.translation.y = bottom;
            particle.velocity.y *= -0.35;
            particle.velocity.x *= 0.6;
            particle.velocity.z *= 0.6;
            particle.spin *= 0.5;
        }
        let spin = particle.spin * dt;
        transform.rotation =
            Quat::from_euler(EulerRot::XYZ, spin.x, spin.y, spin.z) * transform.rotation;
        // Full size for most of its life, then shrink away.
        let shrink = ((1.0 - life) / 0.4).min(1.0);
        transform.scale = Vec3::splat(particle.size * shrink);
    }
}

/// Where a [`Flight`] goes.
#[derive(Clone, Copy, Debug)]
pub enum FlightTarget {
    Point(Vec3),
    /// Follows a (possibly moving) entity, at an offset in its local space.
    Entity(Entity, Vec3),
}

/// Flies an entity in an arc to a target, then triggers [`Landed`] and despawns it.
#[derive(Component, Clone, Debug)]
pub struct Flight {
    from: Vec3,
    to: FlightTarget,
    height: f32,
    duration: f32,
    age: f32,
    spin: f32,
}

impl Flight {
    pub fn new(from: Vec3, to: FlightTarget, height: f32, duration: f32) -> Self {
        Self {
            from,
            to,
            height,
            duration,
            age: 0.0,
            spin: 0.0,
        }
    }

    /// Tumbles around the vertical axis while flying, in radians per second.
    pub fn with_spin(mut self, spin: f32) -> Self {
        self.spin = spin;
        self
    }
}

/// A [`Flight`] reached its target. The entity is despawned right after.
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct Landed {
    pub entity: Entity,
}

fn run_flights(
    mut commands: Commands,
    time: Res<Time>,
    mut flights: Query<(Entity, &mut Flight, &mut Transform)>,
    targets: Query<&GlobalTransform>,
) {
    for (entity, mut flight, mut transform) in &mut flights {
        flight.age += time.delta_secs();
        let t = (flight.age / flight.duration).min(1.0);
        let to = match flight.to {
            FlightTarget::Point(point) => point,
            FlightTarget::Entity(target, offset) => match targets.get(target) {
                Ok(global) => global.transform_point(offset),
                Err(_) => flight.from,
            },
        };
        let along = EaseFunction::SineInOut.sample_clamped(t);
        transform.translation =
            flight.from.lerp(to, along) + Vec3::Y * flight.height * 4.0 * t * (1.0 - t);
        transform.rotate_y(flight.spin * time.delta_secs());
        if t >= 1.0 {
            commands.trigger(Landed { entity });
            commands.entity(entity).despawn();
        }
    }
}
