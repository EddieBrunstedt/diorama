//! Small in-house animation helpers: tweens with easing, squash & stretch springs, wobbles,
//! particles, screen shake and hit-stop.

use crate::rng::{Rng, RunSeed};
use crate::tuning::juice::*;
use bevy::{ecs::system::SystemParam, light::NotShadowCaster, prelude::*};
use std::collections::HashMap;

pub struct JuicePlugin;

impl Plugin for JuicePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ParticleAssets>()
            .init_resource::<Shake>()
            .init_resource::<HitStop>()
            .add_systems(Startup, seed_particles)
            .add_systems(
                Update,
                (
                    run_tweens,
                    run_squash,
                    run_wobbles,
                    run_particles,
                    decay_shake,
                ),
            )
            .add_systems(Last, run_hit_stop);
    }
}

#[derive(Clone, Copy, Debug)]
pub enum TweenTarget {
    Translation { from: Vec3, to: Vec3 },
    Scale { from: Vec3, to: Vec3 },
}

/// Animates a transform property once, then triggers [`TweenDone`] and removes itself (or
/// despawns the entity, see [`Tween::then_despawn`]). The entity stays hidden until the tween
/// starts, so spawn it with `Visibility::Hidden` and [`Tween::start_transform`] when there's a
/// delay.
#[derive(Component, Clone, Debug)]
pub struct Tween {
    pub target: TweenTarget,
    pub ease: EaseFunction,
    pub delay: f32,
    pub duration: f32,
    despawn: bool,
    elapsed: f32,
}

impl Tween {
    pub fn new(target: TweenTarget, ease: EaseFunction, delay: f32, duration: f32) -> Self {
        Self {
            target,
            ease,
            delay,
            duration,
            despawn: false,
            elapsed: 0.0,
        }
    }

    /// Despawns the entity when the tween is done, e.g. for something shrinking away.
    pub fn then_despawn(mut self) -> Self {
        self.despawn = true;
        self
    }

    /// The transform the entity should be spawned with (the tween's first frame).
    pub fn start_transform(&self) -> Transform {
        match self.target {
            TweenTarget::Translation { from, .. } => Transform::from_translation(from),
            TweenTarget::Scale { from, .. } => Transform::from_scale(from),
        }
    }
}

/// A [`Tween`] finished (the entity may be despawned right after).
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct TweenDone {
    pub entity: Entity,
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
            commands.trigger(TweenDone { entity });
            if tween.despawn {
                commands.entity(entity).despawn();
            } else {
                commands.entity(entity).remove::<Tween>();
            }
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

/// A decaying rotational shake, e.g. something that refuses a click. Removes itself when done.
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

/// One shared unit cube for all particles, and one material per color.
#[derive(Resource)]
struct ParticleAssets {
    cube: Handle<Mesh>,
    materials: HashMap<[u8; 4], Handle<StandardMaterial>>,
}

impl FromWorld for ParticleAssets {
    fn from_world(world: &mut World) -> Self {
        let cube = world
            .resource_mut::<Assets<Mesh>>()
            .add(Cuboid::from_length(1.0));
        Self {
            cube,
            materials: HashMap::new(),
        }
    }
}

#[derive(Resource)]
struct ParticleRng(Rng);

fn seed_particles(mut commands: Commands, seed: Res<RunSeed>) {
    commands.insert_resource(ParticleRng(Rng::new(seed.0 ^ 0x9A27)));
}

/// A chunk flying out of something, bouncing on the ground and shrinking away.
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
    /// Size of each chunk.
    pub size: f32,
    /// Horizontal speed.
    pub spread: f32,
    /// Upward speed.
    pub lift: f32,
    pub lifetime: f32,
}

/// Spawns particles.
#[derive(SystemParam)]
pub struct Particles<'w> {
    assets: ResMut<'w, ParticleAssets>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
    rng: ResMut<'w, ParticleRng>,
}

impl Particles<'_> {
    fn material(&mut self, color: Color) -> Handle<StandardMaterial> {
        let key = color.to_srgba().to_u8_array();
        let materials = &mut self.materials;
        self.assets
            .materials
            .entry(key)
            .or_insert_with(|| {
                materials.add(StandardMaterial {
                    base_color: color,
                    perceptual_roughness: 0.95,
                    ..default()
                })
            })
            .clone()
    }

    /// Particles flying out from `at`, starting spread around it by up to `radius`.
    pub fn burst(&mut self, commands: &mut Commands, at: Vec3, radius: f32, burst: Burst) {
        for _ in 0..burst.count {
            let rng = &mut self.rng.0;
            let angle = rng.range(0.0, std::f32::consts::TAU);
            let out = Vec3::new(angle.cos(), 0.0, angle.sin());
            let speed = burst.spread * rng.range(0.4, 1.0);
            let velocity = out * speed + Vec3::Y * burst.lift * rng.range(0.6, 1.2);
            let spin = Vec3::new(
                rng.range(-9.0, 9.0),
                rng.range(-9.0, 9.0),
                rng.range(-9.0, 9.0),
            );
            let start = at + out * radius * rng.range(0.5, 1.0);
            let size = burst.size * rng.range(0.7, 1.2);
            let lifetime = burst.lifetime * rng.range(0.7, 1.2);
            let color = rng.pick(burst.colors);
            let material = self.material(color);
            commands.spawn((
                Particle {
                    velocity,
                    spin,
                    age: 0.0,
                    lifetime,
                    size,
                },
                Mesh3d(self.assets.cube.clone()),
                MeshMaterial3d(material),
                Transform::from_translation(start).with_scale(Vec3::splat(size)),
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
        let bottom = particle.size / 2.0;
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

/// Screen shake. Add trauma for a big moment; it wears off by itself. The camera applies
/// [`Shake::offset`]. Shaking grows with trauma squared, so small amounts stay subtle.
#[derive(Resource, Default, Debug)]
pub struct Shake {
    trauma: f32,
}

impl Shake {
    // Nothing is big enough for an uncapped shake yet; big hits arrive with towers (milestone 4).
    #[allow(dead_code)]
    pub fn add(&mut self, trauma: f32) {
        self.trauma = (self.trauma + trauma).min(1.0);
    }

    /// Adds trauma, but only up to `cap`, so many small bumps in a row (e.g. a long line of
    /// walls landing) don't add up to a big shake.
    pub fn add_capped(&mut self, trauma: f32, cap: f32) {
        if self.trauma < cap {
            self.trauma = (self.trauma + trauma).min(cap);
        }
    }

    /// The camera offset at time `t`, in the camera's right/up plane, as a fraction of
    /// [`SHAKE_MAX_OFFSET`] (each axis within -1..1).
    pub fn offset(&self, t: f32) -> Vec2 {
        let strength = self.trauma * self.trauma;
        // Out-of-phase sines: shaky, but smooth enough not to look like a glitch.
        let x = (t * 47.0).sin() * 0.6 + (t * 83.0 + 1.3).sin() * 0.4;
        let y = (t * 53.0 + 2.1).sin() * 0.6 + (t * 71.0 + 0.4).sin() * 0.4;
        Vec2::new(x, y) * strength
    }
}

fn decay_shake(time: Res<Time>, mut shake: ResMut<Shake>) {
    if shake.trauma > 0.0 {
        shake.trauma = (shake.trauma - SHAKE_DECAY * time.delta_secs()).max(0.0);
    }
}

/// Hit-stop: the game all but freezes for a moment on a big hit, so it lands with weight. Counted
/// in real time, since game time is what it stops.
#[derive(Resource, Default, Debug)]
pub struct HitStop {
    /// Real seconds left.
    left: f32,
    /// The game speed to go back to (the dev speed-up may have changed it).
    speed: Option<f32>,
}

impl HitStop {
    /// Freezes the game for `seconds` (real time), on top of any freeze still going.
    // Nothing hits hard enough yet; big hits arrive with towers (milestone 4).
    #[allow(dead_code)]
    pub fn add(&mut self, seconds: f32) {
        self.left = (self.left + seconds).min(MAX_HIT_STOP);
    }
}

fn run_hit_stop(real: Res<Time<Real>>, mut stop: ResMut<HitStop>, mut time: ResMut<Time<Virtual>>) {
    if stop.left > 0.0 {
        let speed = *stop.speed.get_or_insert(time.relative_speed());
        time.set_relative_speed(speed * HIT_STOP_SPEED);
        stop.left -= real.delta_secs();
    } else if let Some(speed) = stop.speed.take() {
        time.set_relative_speed(speed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capped_shake_stops_adding_up() {
        let mut shake = Shake::default();
        for _ in 0..10 {
            shake.add_capped(0.2, 0.5);
        }
        assert_eq!(shake.trauma, 0.5);
        shake.add(0.9);
        assert_eq!(shake.trauma, 1.0);
        shake.add_capped(0.2, 0.5);
        assert_eq!(shake.trauma, 1.0, "a cap never lowers the trauma");
    }
}
