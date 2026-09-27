//! Towers: they stand in the maze like walls (the horde breaks them the same way) and shoot the
//! horde. The rules for picking targets and dealing damage are plain data in [`targeting`].
//!
//! - The **autocannon** swings its turret to the enemy in range that's closest to the Command
//!   Center, and hits it the moment it fires (a tracer shows the shot).
//! - The **flamer** points its nozzle at the nearest enemy and burns everything in its cone.
//! - The **mortar** picks the biggest clump in range (it can't reach close by), turns to it, and
//!   lobs a shell where the clump will be when it lands. The blast hurts everything in it, less
//!   toward its edge.
//!
//! A tower's view entity (see [`crate::structures`]) carries its [`Tower`]; the turret is a child
//! that turns. Towers only fire once they've landed.

mod effects;
pub mod meshes;
mod range;
pub mod targeting;

use crate::flow::{Class, FlowFields};
use crate::grid::Kind;
use crate::horde::{Brute, DamageSystems, EnemyIndex, Health, Swarmer};
use crate::juice::{Squash, Tween};
use crate::shapes::ShapeMaterial;
use crate::tuning::horde::{BRUTE_RADIUS, SWARMER_RADIUS};
use crate::tuning::juice::{BRUTE_HIT_SQUASH, SHELL_ARC};
use crate::tuning::towers::*;
use bevy::{platform::collections::HashMap, prelude::*};
use meshes::layout;
use targeting::{
    Candidate, best_clump, clamp_to_range, first, in_cone, in_range, lead, nearest, on_target,
    splash, turn_toward,
};

/// How closely a turret has to point at its target before it fires (radians).
const AIM_TOLERANCE: f32 = 0.12;

pub struct TowersPlugin;

impl Plugin for TowersPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TowerAssets>()
            .init_resource::<ShapeMaterial>()
            .add_message::<TowerFired>()
            .add_message::<ShellLanded>()
            .add_plugins((effects::EffectsPlugin, range::RangePlugin))
            .add_systems(
                Update,
                (
                    attach_turrets,
                    (fire_autocannons, fire_flamers, fire_mortars, fly_shells)
                        .chain()
                        .in_set(TowerSystems)
                        .in_set(DamageSystems),
                    turn_turrets.after(TowerSystems),
                ),
            );
    }
}

/// Towers aiming and firing. Whatever shows their shots runs after it.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TowerSystems;

/// A tower standing on `cell`.
#[derive(Component, Clone, Debug)]
pub struct Tower {
    pub kind: Kind,
    pub cell: IVec2,
    /// Which way the turret points on the ground (unit length).
    pub aim: Vec2,
    /// Seconds until it can fire again (for the flamer, until its next tick).
    cooldown: f32,
    /// What it's aiming at.
    target: Option<Entity>,
    /// Seconds until it looks for a better target (autocannon).
    retarget: f32,
    /// Whether the flamer is burning this frame.
    pub burning: bool,
}

impl Tower {
    pub fn new(kind: Kind, cell: IVec2) -> Self {
        Self {
            kind,
            cell,
            // Facing the nests, up the screen.
            aim: Vec2::NEG_Y,
            cooldown: 0.0,
            target: None,
            retarget: 0.0,
            burning: false,
        }
    }

    /// Where the barrel's tip is, in the world.
    pub fn muzzle(&self, at: Vec3) -> Vec3 {
        let layout = layout(self.kind);
        let muzzle = turret_rotation(self.aim) * layout.muzzle;
        at + Vec3::Y * layout.pivot + muzzle
    }
}

/// The turret on top of a tower: it turns to the tower's aim, and kicks back when it fires.
#[derive(Component, Clone, Debug, Default)]
pub struct Turret {
    /// How far it's kicked back, easing out.
    pub kick: f32,
}

/// A tower fired: from the muzzle at `from`, at `to`.
#[derive(Message, Clone, Copy, Debug)]
pub struct TowerFired {
    pub tower: Entity,
    pub kind: Kind,
    pub from: Vec3,
    pub to: Vec3,
}

/// A mortar shell hit the ground at `at`.
#[derive(Message, Clone, Copy, Debug)]
pub struct ShellLanded {
    pub at: Vec3,
}

/// A mortar shell in flight.
#[derive(Component, Clone, Debug)]
pub struct Shell {
    from: Vec3,
    to: Vec3,
    age: f32,
}

impl Shell {
    /// Where it is `age` seconds into its flight: along the line, plus an arc.
    fn position(&self, age: f32) -> Vec3 {
        let t = (age / MORTAR_FLIGHT_SECONDS).clamp(0.0, 1.0);
        self.from.lerp(self.to, t) + Vec3::Y * SHELL_ARC * 4.0 * t * (1.0 - t)
    }
}

/// Each tower's base and turret meshes (walls have none), and the shell's.
#[derive(Resource)]
pub struct TowerAssets {
    towers: HashMap<Kind, (Handle<Mesh>, Handle<Mesh>)>,
    shell: Handle<Mesh>,
}

impl TowerAssets {
    pub fn base(&self, kind: Kind) -> Handle<Mesh> {
        self.towers
            .get(&kind)
            .map(|(base, _)| base.clone())
            .unwrap_or_default()
    }

    fn turret(&self, kind: Kind) -> Handle<Mesh> {
        self.towers
            .get(&kind)
            .map(|(_, turret)| turret.clone())
            .unwrap_or_default()
    }
}

impl FromWorld for TowerAssets {
    fn from_world(world: &mut World) -> Self {
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        let towers = Kind::ALL
            .into_iter()
            .filter(|kind| kind.is_tower())
            .map(|kind| {
                let base = meshes.add(meshes::base_mesh(kind));
                let turret = meshes.add(meshes::turret_mesh(kind));
                (kind, (base, turret))
            })
            .collect();
        Self {
            towers,
            shell: meshes.add(meshes::shell_mesh()),
        }
    }
}

/// The rotation that turns a model facing -Z toward `aim` on the ground.
fn turret_rotation(aim: Vec2) -> Quat {
    Quat::from_rotation_y((-aim.x).atan2(-aim.y))
}

/// How big an enemy of `class` is, for cones and blasts that catch its edge.
fn radius(class: Class) -> f32 {
    match class {
        Class::Swarmer => SWARMER_RADIUS,
        Class::Brute => BRUTE_RADIUS,
    }
}

fn attach_turrets(
    mut commands: Commands,
    towers: Query<(Entity, &Tower), Added<Tower>>,
    assets: Res<TowerAssets>,
    material: Res<ShapeMaterial>,
) {
    for (entity, tower) in &towers {
        let layout = layout(tower.kind);
        commands.entity(entity).with_child((
            Turret::default(),
            Mesh3d(assets.turret(tower.kind)),
            MeshMaterial3d(material.0.clone()),
            Transform::from_xyz(0.0, layout.pivot, 0.0).with_rotation(turret_rotation(tower.aim)),
        ));
    }
}

fn turn_turrets(
    time: Res<Time>,
    towers: Query<(&Tower, &Children)>,
    mut turrets: Query<(&mut Turret, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (tower, children) in &towers {
        for &child in children {
            let Ok((mut turret, mut transform)) = turrets.get_mut(child) else {
                continue;
            };
            turret.kick = (turret.kick - dt * KICK_RECOVERY).max(0.0);
            let rotation = turret_rotation(tower.aim);
            transform.rotation = rotation;
            transform.translation =
                Vec3::Y * layout(tower.kind).pivot + rotation * Vec3::Z * turret.kick;
        }
    }
}

/// How fast a turret's kick recovers (cells per second).
const KICK_RECOVERY: f32 = 0.8;

/// An enemy a tower could hit, looked up by what the horde's index says is near.
type Enemies<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Health,
        &'static Transform,
        Option<&'static Swarmer>,
        Option<&'static Brute>,
        Option<&'static mut Squash>,
    ),
>;

/// An enemy, and what class it is.
type Target = (Entity, Class);

/// The living enemies within `reach` of `from`, as targets, at where they are exactly.
fn candidates(
    index: &EnemyIndex,
    enemies: &Enemies,
    fields: &FlowFields,
    from: Vec2,
    reach: f32,
) -> Vec<Candidate<Target>> {
    index
        .near(from, reach)
        .filter_map(|(_, entity, class)| {
            let (health, transform, ..) = enemies.get(entity).ok()?;
            if !health.is_alive() {
                return None;
            }
            let at = transform.translation.xz();
            let field = fields.get(class);
            let progress = field.distance(field.node_at(at));
            let weight = match class {
                Class::Swarmer => 1.0,
                Class::Brute => MORTAR_BRUTE_WEIGHT,
            };
            Some(Candidate {
                id: (entity, class),
                at,
                progress: if progress.is_finite() {
                    progress
                } else {
                    f32::MAX
                },
                weight,
            })
        })
        .collect()
}

/// Hits `entity` for `amount`. Brutes flinch.
fn hit(enemies: &mut Enemies, entity: Entity, amount: f32, flinch: bool) {
    if let Ok((mut health, _, _, brute, squash)) = enemies.get_mut(entity) {
        health.hurt(amount);
        if flinch
            && brute.is_some()
            && let Some(mut squash) = squash
        {
            squash.pulse(BRUTE_HIT_SQUASH);
        }
    }
}

/// Where `target` is, if it's still alive.
fn alive_at(enemies: &Enemies, target: Entity) -> Option<Vec2> {
    let (health, transform, ..) = enemies.get(target).ok()?;
    health.is_alive().then(|| transform.translation.xz())
}

fn fire_autocannons(
    time: Res<Time>,
    fields: Res<FlowFields>,
    index: EnemyIndex,
    mut enemies: Enemies,
    mut towers: Query<(Entity, &mut Tower, &Transform), Without<Tween>>,
    mut fired: MessageWriter<TowerFired>,
) {
    let dt = time.delta_secs();
    for (entity, mut tower, transform) in &mut towers {
        if tower.kind != Kind::Autocannon {
            continue;
        }
        tower.cooldown = (tower.cooldown - dt).max(0.0);
        tower.retarget -= dt;
        let from = transform.translation.xz();
        let range = AUTOCANNON_RANGE;
        // Keep tracking the target between looks, as long as it's alive and in range.
        let current = tower
            .target
            .and_then(|target| alive_at(&enemies, target))
            .filter(|&at| in_range(from, at, 0.0, range));
        let target_at = match current {
            Some(at) if tower.retarget > 0.0 => at,
            _ => {
                tower.retarget = RETARGET_SECONDS;
                let nearby = candidates(&index, &enemies, &fields, from, range);
                let Some(target) = first(from, range, nearby) else {
                    tower.target = None;
                    continue;
                };
                tower.target = Some(target.id.0);
                target.at
            }
        };
        let toward = target_at - from;
        tower.aim = turn_toward(tower.aim, toward, AUTOCANNON_TURN_RATE, dt);
        if tower.cooldown > 0.0 || !on_target(tower.aim, toward, AIM_TOLERANCE) {
            continue;
        }
        let Some(target) = tower.target else {
            continue;
        };
        tower.cooldown = 1.0 / AUTOCANNON_SHOTS_PER_SECOND;
        hit(&mut enemies, target, AUTOCANNON_DAMAGE, true);
        fired.write(TowerFired {
            tower: entity,
            kind: tower.kind,
            from: tower.muzzle(transform.translation),
            to: Vec3::new(target_at.x, 0.15, target_at.y),
        });
    }
}

/// Flamers burn in ticks: every `FLAMER_TICK_SECONDS` everything in the cone takes that much of
/// the damage per second, and the nozzle picks the nearest enemy again. In between, it keeps
/// turning to follow its target.
fn fire_flamers(
    time: Res<Time>,
    fields: Res<FlowFields>,
    index: EnemyIndex,
    mut enemies: Enemies,
    mut towers: Query<(&mut Tower, &Transform), Without<Tween>>,
) {
    let dt = time.delta_secs();
    let half_angle = FLAMER_CONE_DEGREES.to_radians() / 2.0;
    for (mut tower, transform) in &mut towers {
        if tower.kind != Kind::Flamer {
            continue;
        }
        let from = transform.translation.xz();
        if let Some(at) = tower.target.and_then(|target| alive_at(&enemies, target)) {
            tower.aim = turn_toward(tower.aim, at - from, FLAMER_TURN_RATE, dt);
        }
        tower.cooldown -= dt;
        if tower.cooldown > 0.0 {
            continue;
        }
        tower.cooldown = (tower.cooldown + FLAMER_TICK_SECONDS).max(0.0);
        let range = FLAMER_RANGE;
        // Anything the flames can reach, brutes by their edge.
        let nearby: Vec<_> = candidates(&index, &enemies, &fields, from, range + BRUTE_RADIUS)
            .into_iter()
            .filter(|candidate| candidate.at.distance(from) <= range + radius(candidate.id.1))
            .collect();
        let target = nearest(from, f32::INFINITY, nearby.iter().copied());
        tower.target = target.map(|target| target.id.0);
        tower.burning = target.is_some();
        if target.is_none() {
            continue;
        }
        let aim = tower.aim;
        for candidate in &nearby {
            let (enemy, class) = candidate.id;
            if in_cone(from, aim, half_angle, range, candidate.at, radius(class)) {
                hit(
                    &mut enemies,
                    enemy,
                    FLAMER_DAMAGE_PER_SECOND * FLAMER_TICK_SECONDS,
                    false,
                );
            }
        }
    }
}

fn fire_mortars(
    mut commands: Commands,
    time: Res<Time>,
    fields: Res<FlowFields>,
    index: EnemyIndex,
    enemies: Enemies,
    assets: Res<TowerAssets>,
    material: Res<ShapeMaterial>,
    mut towers: Query<(Entity, &mut Tower, &Transform), Without<Tween>>,
    mut fired: MessageWriter<TowerFired>,
) {
    let dt = time.delta_secs();
    for (entity, mut tower, transform) in &mut towers {
        if tower.kind != Kind::Mortar {
            continue;
        }
        tower.cooldown = (tower.cooldown - dt).max(0.0);
        if tower.cooldown > 0.0 {
            continue;
        }
        let from = transform.translation.xz();
        let in_reach = |at: Vec2| in_range(from, at, MORTAR_MIN_RANGE, MORTAR_RANGE);

        // Keep the target it's turning to, as long as it's alive and in reach.
        let current = tower.target.and_then(|target| {
            let (health, transform, ..) = enemies.get(target).ok()?;
            let at = transform.translation.xz();
            (health.is_alive() && in_reach(at)).then_some(at)
        });
        let target_at = match current {
            Some(at) => at,
            None => {
                let nearby: Vec<_> = candidates(&index, &enemies, &fields, from, MORTAR_RANGE)
                    .into_iter()
                    .filter(|candidate| in_reach(candidate.at))
                    .collect();
                let weight_near = |at: Vec2| {
                    nearby
                        .iter()
                        .filter(|other| other.at.distance(at) <= MORTAR_SPLASH_RADIUS)
                        .map(|other| other.weight)
                        .sum()
                };
                let Some(clump) = best_clump(&nearby, MORTAR_CANDIDATES, weight_near) else {
                    tower.target = None;
                    continue;
                };
                tower.target = Some(clump.id.0);
                clump.at
            }
        };
        let toward = target_at - from;
        tower.aim = turn_toward(tower.aim, toward, MORTAR_TURN_RATE, dt);
        if !on_target(tower.aim, toward, AIM_TOLERANCE) {
            continue;
        }

        // Fire where it will be when the shell lands.
        let velocity = tower
            .target
            .and_then(|target| enemies.get(target).ok())
            .map_or(Vec2::ZERO, |(_, _, swarmer, brute, _)| {
                swarmer
                    .map(Swarmer::velocity)
                    .or(brute.map(Brute::velocity))
                    .unwrap_or_default()
            });
        let landing = clamp_to_range(
            from,
            lead(target_at, velocity, MORTAR_FLIGHT_SECONDS),
            MORTAR_MIN_RANGE,
            MORTAR_RANGE,
        );
        tower.cooldown = MORTAR_RELOAD_SECONDS;
        tower.target = None;
        let muzzle = tower.muzzle(transform.translation);
        let to = Vec3::new(landing.x, 0.0, landing.y);
        let shell = Shell {
            from: muzzle,
            to,
            age: 0.0,
        };
        commands.spawn((
            Mesh3d(assets.shell.clone()),
            MeshMaterial3d(material.0.clone()),
            Transform::from_translation(muzzle),
            shell,
        ));
        fired.write(TowerFired {
            tower: entity,
            kind: tower.kind,
            from: muzzle,
            to,
        });
    }
}

fn fly_shells(
    mut commands: Commands,
    time: Res<Time>,
    index: EnemyIndex,
    mut enemies: Enemies,
    mut shells: Query<(Entity, &mut Shell, &mut Transform), Without<Health>>,
    mut landed: MessageWriter<ShellLanded>,
) {
    let dt = time.delta_secs();
    for (entity, mut shell, mut transform) in &mut shells {
        shell.age += dt;
        let at = shell.position(shell.age);
        let ahead = shell.position(shell.age + 0.02);
        transform.translation = at;
        if let Some(heading) = (ahead - at).try_normalize() {
            transform.look_to(heading, Vec3::Y);
        }
        if shell.age < MORTAR_FLIGHT_SECONDS {
            continue;
        }
        commands.entity(entity).despawn();
        let middle = shell.to.xz();
        let caught: Vec<(Entity, f32)> = index
            .near(middle, MORTAR_SPLASH_RADIUS)
            .filter_map(|(_, enemy, class)| {
                let (_, transform, ..) = enemies.get(enemy).ok()?;
                let distance = transform.translation.xz().distance(middle);
                let damage = splash(
                    distance,
                    radius(class),
                    MORTAR_SPLASH_RADIUS,
                    MORTAR_DAMAGE,
                    MORTAR_EDGE_DAMAGE,
                )?;
                Some((enemy, damage))
            })
            .collect();
        for (enemy, damage) in caught {
            hit(&mut enemies, enemy, damage, true);
        }
        landed.write(ShellLanded { at: shell.to });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shells_arc_from_muzzle_to_target() {
        let shell = Shell {
            from: Vec3::new(0.0, 1.0, 0.0),
            to: Vec3::new(8.0, 0.0, 0.0),
            age: 0.0,
        };
        assert_eq!(shell.position(0.0), shell.from);
        assert_eq!(shell.position(MORTAR_FLIGHT_SECONDS), shell.to);
        let top = shell.position(MORTAR_FLIGHT_SECONDS / 2.0);
        assert!((top.y - (0.5 + SHELL_ARC)).abs() < 1e-4, "{top}");
    }

    #[test]
    fn muzzles_follow_the_aim() {
        let mut tower = Tower::new(Kind::Autocannon, IVec2::ZERO);
        tower.aim = Vec2::X;
        let muzzle = tower.muzzle(Vec3::ZERO);
        assert!(muzzle.x > 0.5 && muzzle.z.abs() < 1e-4, "{muzzle}");
        tower.aim = Vec2::NEG_Y;
        let muzzle = tower.muzzle(Vec3::ZERO);
        assert!(muzzle.z < -0.5, "{muzzle}");
    }
}
