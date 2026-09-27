//! Abilities: the arsenal the player fires in the wave phase, from the buildings that give it.
//!
//! - The **sniper shot** (the Command Center): one big hit on one enemy, anywhere on the map. It
//!   snaps to the toughest enemy near the cursor, so brutes come first ([`sniper`]).
//! - The **mortar barrage** (the Artillery Command): a salvo of shells over a circle, landing a
//!   moment after the click, anywhere on the map ([`barrage`]).
//!
//! Each ability has a charge per building that gives it, each reloading on its own once used
//! ([`charges`], plain data). Every charge is ready when a wave starts. A building that's
//! destroyed takes a charge with it (a reloading one first), and with none left the ability is
//! gone until another is built.
//!
//! 1–5 (or a slot on the ability bar, [`bar`]) aims an ability; a left click fires it, and Esc, a
//! right click or the same key again stops aiming. While aiming, the ground shows what a click
//! would hit ([`preview`]). Where the shots land is plain data in [`targeting`].

mod bar;
mod barrage;
pub mod charges;
mod preview;
mod sniper;
pub mod targeting;

use crate::build::{BuildSystems, StructureChanged, UiHovered};
use crate::command_center;
use crate::flow::Class;
use crate::grid::{Grid, Kind, cell_at};
use crate::horde::{DamageSystems, EnemyIndex, Health};
use crate::map::Landmarks;
use crate::pointer::{Pointer, PointerSystems};
use crate::towers::{Enemies, hit_with, radius};
use crate::tuning::abilities::*;
use crate::tuning::juice::SNIPER_FLINCH;
use crate::waves::Phase;
use crate::wear::WearSystems;
use bevy::prelude::*;
use charges::Charges;
use targeting::{Mark, sniper_target};

pub struct AbilitiesPlugin;

impl Plugin for AbilitiesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Arsenal>()
            .init_resource::<Aiming>()
            .add_message::<AbilityPressed>()
            .add_message::<AbilityDenied>()
            .add_message::<AbilityReady>()
            .add_message::<SniperShot>()
            .add_message::<BarrageCalled>()
            .add_plugins((
                bar::BarPlugin,
                preview::PreviewPlugin,
                sniper::SniperPlugin,
                barrage::BarragePlugin,
            ))
            .add_systems(OnEnter(Phase::Wave), (count_buildings, refill).chain())
            .add_systems(OnExit(Phase::Wave), stop_aiming)
            .add_systems(
                Update,
                (
                    follow_buildings.after(BuildSystems).after(WearSystems),
                    (reload, aim, fire)
                        .chain()
                        .after(follow_buildings)
                        .after(PointerSystems)
                        .run_if(in_state(Phase::Wave))
                        .in_set(AbilitySystems)
                        .in_set(DamageSystems),
                ),
            );
    }
}

/// Aiming and firing abilities. Whatever shows them runs after it.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AbilitySystems;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ability {
    Sniper,
    Barrage,
}

/// The key that aims each ability, in [`Ability::ALL`] order.
const ABILITY_KEYS: [(KeyCode, &str); 2] = [(KeyCode::Digit1, "1"), (KeyCode::Digit2, "2")];

impl Ability {
    /// Every ability, in the order the bar shows them.
    pub const ALL: [Ability; 2] = [Ability::Sniper, Ability::Barrage];

    pub fn name(self) -> &'static str {
        match self {
            Ability::Sniper => "Sniper shot",
            Ability::Barrage => "Mortar barrage",
        }
    }

    /// The building that gives it.
    pub fn source(self) -> &'static str {
        match self {
            Ability::Sniper => "Command Center",
            Ability::Barrage => Kind::ArtilleryCommand.name(),
        }
    }

    /// Its hotkey, and how the bar shows it.
    pub fn key(self) -> (KeyCode, &'static str) {
        ABILITY_KEYS[self as usize]
    }
}

/// Every ability's charges.
#[derive(Resource, Clone, Debug)]
pub struct Arsenal {
    /// In [`Ability::ALL`] order.
    charges: [Charges; 2],
}

impl Default for Arsenal {
    fn default() -> Self {
        Self {
            // The Command Center is always there; Artillery Commands have to be built.
            charges: [
                Charges::new(SNIPER_COOLDOWN, 1),
                Charges::new(BARRAGE_COOLDOWN, 0),
            ],
        }
    }
}

impl Arsenal {
    pub fn charges(&self, ability: Ability) -> &Charges {
        &self.charges[ability as usize]
    }

    fn charges_mut(&mut self, ability: Ability) -> &mut Charges {
        &mut self.charges[ability as usize]
    }
}

/// The ability being aimed, if any: a left click fires it.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Aiming(pub Option<Ability>);

/// A slot on the ability bar was clicked: the same as its hotkey.
#[derive(Message, Clone, Copy, Debug)]
pub struct AbilityPressed(pub Ability);

/// The player asked for an ability that couldn't do it: none ready, or (for the sniper) no enemy
/// near the click.
#[derive(Message, Clone, Copy, Debug)]
pub struct AbilityDenied(pub Ability);

/// A charge of the ability came back.
#[derive(Message, Clone, Copy, Debug)]
pub struct AbilityReady(pub Ability);

/// The sniper shot fired from `from` and hit an enemy of `class` at `to`.
#[derive(Message, Clone, Copy, Debug)]
pub struct SniperShot {
    pub class: Class,
    pub from: Vec3,
    pub to: Vec3,
}

/// A barrage was called in on `center` (on the ground).
#[derive(Message, Clone, Copy, Debug)]
pub struct BarrageCalled {
    pub center: Vec2,
}

/// Where the sniper shot comes from: the light on top of the Command Center's antenna.
pub fn sniper_muzzle(landmarks: &Landmarks) -> Vec3 {
    landmarks.command_center.center() + command_center::ANTENNA
}

/// The enemy the sniper would hit for a click at `cursor`, and where it is. `lookup` gives an
/// enemy's HP and position, if it's still there.
pub fn sniper_mark(
    index: &EnemyIndex,
    cursor: Vec2,
    lookup: impl Fn(Entity) -> Option<(f32, Vec2)>,
) -> Option<Mark<(Entity, Class)>> {
    let marks = index
        .near(cursor, SNIPER_SNAP)
        .filter_map(|(_, entity, class)| {
            let (health, at) = lookup(entity)?;
            Some(Mark {
                id: (entity, class),
                at,
                radius: radius(class),
                health,
            })
        });
    sniper_target(cursor, SNIPER_SNAP, marks)
}

/// Whether a barrage can be called in on the ground point `at`: only over the map.
pub fn barrage_allowed(grid: &Grid, at: Vec2) -> bool {
    grid.cell(cell_at(Vec3::new(at.x, 0.0, at.y))).is_some()
}

/// The Artillery Commands standing give the barrage its charges.
fn count_buildings(grid: Res<Grid>, mut arsenal: ResMut<Arsenal>) {
    let standing = grid.count(Kind::ArtilleryCommand) as u32;
    arsenal.charges_mut(Ability::Barrage).set_sources(standing);
}

/// Follows Artillery Commands built, sold and destroyed.
fn follow_buildings(
    mut changes: MessageReader<StructureChanged>,
    grid: Res<Grid>,
    arsenal: ResMut<Arsenal>,
) {
    let artillery = |structure: Option<crate::grid::Structure>| {
        structure.and_then(|structure| structure.kind()) == Some(Kind::ArtilleryCommand)
    };
    let changed = changes
        .read()
        .any(|change| artillery(change.0.before) || artillery(change.0.after));
    if changed {
        count_buildings(grid, arsenal);
    }
}

/// Every charge is ready when a wave starts.
fn refill(mut arsenal: ResMut<Arsenal>) {
    for ability in Ability::ALL {
        arsenal.charges_mut(ability).refill();
    }
}

fn stop_aiming(mut aiming: ResMut<Aiming>) {
    aiming.0 = None;
}

fn reload(time: Res<Time>, mut arsenal: ResMut<Arsenal>, mut ready: MessageWriter<AbilityReady>) {
    for ability in Ability::ALL {
        if arsenal.charges_mut(ability).tick(time.delta_secs()) > 0 {
            ready.write(AbilityReady(ability));
        }
    }
}

/// Keys and the bar's slots pick what to aim; Esc or a right click stops aiming.
fn aim(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut pressed: MessageReader<AbilityPressed>,
    arsenal: Res<Arsenal>,
    mut aiming: ResMut<Aiming>,
    mut denied: MessageWriter<AbilityDenied>,
) {
    let mut asked: Vec<Ability> = pressed.read().map(|pressed| pressed.0).collect();
    if !keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]) {
        asked.extend(
            Ability::ALL
                .into_iter()
                .filter(|ability| keys.just_pressed(ability.key().0)),
        );
    }
    for ability in asked {
        if aiming.0 == Some(ability) {
            aiming.0 = None;
        } else if arsenal.charges(ability).ready() > 0 {
            aiming.0 = Some(ability);
        } else {
            denied.write(AbilityDenied(ability));
        }
    }
    if keys.just_pressed(KeyCode::Escape) || mouse.just_pressed(MouseButton::Right) {
        aiming.0 = None;
    }
    // Its last charge went with the building that gave it.
    if let Some(ability) = aiming.0
        && arsenal.charges(ability).ready() == 0
    {
        aiming.0 = None;
    }
}

/// A left click fires what's aimed, and uses a charge. A sniper shot with no enemy near the
/// click, or a barrage off the map, does nothing and keeps aiming.
fn fire(
    mouse: Res<ButtonInput<MouseButton>>,
    ui: Res<UiHovered>,
    pointer: Res<Pointer>,
    grid: Res<Grid>,
    landmarks: Res<Landmarks>,
    index: EnemyIndex,
    mut enemies: Enemies,
    mut arsenal: ResMut<Arsenal>,
    mut aiming: ResMut<Aiming>,
    mut denied: MessageWriter<AbilityDenied>,
    mut sniped: MessageWriter<SniperShot>,
    mut called: MessageWriter<BarrageCalled>,
) {
    let Some(ability) = aiming.0 else {
        return;
    };
    if !mouse.just_pressed(MouseButton::Left) || ui.0 {
        return;
    }
    let Some(cursor) = pointer.ground.map(|ground| ground.xz()) else {
        return;
    };
    match ability {
        Ability::Sniper => {
            let mark = sniper_mark(&index, cursor, |entity| {
                let (health, transform, ..) = enemies.get(entity).ok()?;
                Some((health.0, transform.translation.xz()))
            });
            let Some(mark) = mark else {
                denied.write(AbilityDenied(ability));
                return;
            };
            if !arsenal.charges_mut(ability).use_one() {
                return;
            }
            let (target, class) = mark.id;
            hit_with(&mut enemies, target, SNIPER_DAMAGE, Some(SNIPER_FLINCH));
            sniped.write(SniperShot {
                class,
                from: sniper_muzzle(&landmarks),
                to: Vec3::new(mark.at.x, radius(class) * 0.6, mark.at.y),
            });
        }
        Ability::Barrage => {
            if !barrage_allowed(&grid, cursor) {
                denied.write(AbilityDenied(ability));
                return;
            }
            if !arsenal.charges_mut(ability).use_one() {
                return;
            }
            called.write(BarrageCalled { center: cursor });
        }
    }
    aiming.0 = None;
}

/// Enemies' HP and position, read-only: for previews.
pub type EnemyLookup<'w, 's> = Query<'w, 's, (&'static Health, &'static Transform)>;

/// An enemy's HP and position, if it's still there.
pub fn look_up(enemies: &EnemyLookup, entity: Entity) -> Option<(f32, Vec2)> {
    let (health, transform) = enemies.get(entity).ok()?;
    Some((health.0, transform.translation.xz()))
}
