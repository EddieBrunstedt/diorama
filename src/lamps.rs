//! Lamps: the small lights that come on as the light fades (tower lanterns, glowing arrow tips,
//! burning boulders), the flash of a boulder landing, and the materials that glow with the mood.
//!
//! How brightly lamps burn and how much slimes glow comes from the current mood
//! ([`crate::mood`]): unlit on a calm afternoon, glowing through the dusk of a wave.

use crate::hearth::wobble;
use crate::mood::{CurrentMood, MoodSystems};
use crate::palette;
use bevy::prelude::*;

/// How strongly lit lamp glass glows at full dusk.
const LAMP_GLOW: f32 = 8.0;
/// How strongly boulders smoulder at full dusk.
const EMBER_GLOW: f32 = 1.6;
/// How strongly slimes glow at full dusk.
const SLIME_GLOW: f32 = 0.22;
/// How much lamps flicker.
const FLICKER: f32 = 0.12;

pub struct LampsPlugin;

impl Plugin for LampsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GlowMaterials>().add_systems(
            Update,
            (glow_with_mood, light_lamps, fade_flashes).after(MoodSystems),
        );
    }
}

/// Materials whose glow follows the mood.
#[derive(Resource)]
pub struct GlowMaterials {
    /// Lamp glass: pale in daylight, glowing at dusk.
    pub lamp: Handle<StandardMaterial>,
    /// Glass of a lamp that's out (a broken tower's lantern).
    pub lamp_out: Handle<StandardMaterial>,
    /// Slimes and their goo: the shared vertex-colored voxel look, glowing faintly at dusk.
    pub slime: Handle<StandardMaterial>,
    /// Catapult boulders: vertex-colored, smouldering at dusk.
    pub ember: Handle<StandardMaterial>,
}

impl FromWorld for GlowMaterials {
    fn from_world(world: &mut World) -> Self {
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        let glass = |materials: &mut Assets<StandardMaterial>| {
            materials.add(StandardMaterial {
                base_color: palette::LANTERN_GLASS,
                perceptual_roughness: 0.5,
                ..default()
            })
        };
        Self {
            lamp: glass(&mut materials),
            lamp_out: glass(&mut materials),
            slime: materials.add(StandardMaterial {
                base_color: Color::WHITE,
                perceptual_roughness: 0.9,
                reflectance: 0.2,
                ..default()
            }),
            ember: materials.add(StandardMaterial {
                base_color: Color::WHITE,
                perceptual_roughness: 0.9,
                ..default()
            }),
        }
    }
}

/// A point light that burns as brightly as the mood's lamps. Put it on an entity with a
/// [`PointLight`] (see [`lamp`]).
#[derive(Component, Debug)]
pub struct Lamp {
    pub lumens: f32,
    /// Lamps that are out stay dark whatever the mood.
    pub lit: bool,
    /// Where it is in its flicker, so lamps don't flicker in step.
    phase: f32,
}

/// A lamp giving off `lumens` at full dusk, out to `range`.
pub fn lamp(lumens: f32, range: f32, phase: f32) -> impl Bundle {
    (
        Lamp {
            lumens,
            lit: true,
            phase,
        },
        PointLight {
            color: palette::LAMPLIGHT,
            intensity: 0.0,
            range,
            radius: 0.02,
            shadow_maps_enabled: false,
            ..default()
        },
    )
}

/// A brief burst of light, fading out over `lifetime`, whatever the mood.
#[derive(Component, Debug)]
pub struct Flash {
    lumens: f32,
    age: f32,
    lifetime: f32,
}

pub fn flash(at: Vec3, lumens: f32, range: f32, lifetime: f32) -> impl Bundle {
    (
        Flash {
            lumens,
            age: 0.0,
            lifetime,
        },
        PointLight {
            color: palette::IMPACT_FLASH,
            intensity: lumens,
            range,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_translation(at),
    )
}

fn glow_with_mood(
    current: Res<CurrentMood>,
    glow: Res<GlowMaterials>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !current.is_changed() {
        return;
    }
    let mood = &current.mood;
    if let Some(mut lamp) = materials.get_mut(&glow.lamp) {
        lamp.emissive = palette::LAMP_GLOW.to_linear() * LAMP_GLOW * mood.lamps;
    }
    if let Some(mut slime) = materials.get_mut(&glow.slime) {
        slime.emissive = palette::SLIME_GLOW.to_linear() * SLIME_GLOW * mood.slime_glow;
    }
    if let Some(mut ember) = materials.get_mut(&glow.ember) {
        ember.emissive = palette::LAMP_GLOW.to_linear() * EMBER_GLOW * mood.lamps;
    }
}

fn light_lamps(
    time: Res<Time>,
    current: Res<CurrentMood>,
    mut lamps: Query<(&Lamp, &mut PointLight)>,
) {
    let t = time.elapsed_secs();
    let lamps_on = current.mood.lamps;
    for (lamp, mut light) in &mut lamps {
        let intensity = if lamp.lit {
            lamp.lumens * lamps_on * (1.0 + FLICKER * wobble(t, lamp.phase))
        } else {
            0.0
        };
        light.intensity = intensity;
    }
}

fn fade_flashes(
    mut commands: Commands,
    time: Res<Time>,
    mut flashes: Query<(Entity, &mut Flash, &mut PointLight)>,
) {
    for (entity, mut flash, mut light) in &mut flashes {
        flash.age += time.delta_secs();
        let left = 1.0 - flash.age / flash.lifetime;
        if left <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        light.intensity = flash.lumens * left * left;
    }
}
