//! Grey-box lighting: a low-ish sun that throws readable shadows, and a cool fill light.

use crate::palette;
use bevy::{
    light::{CascadeShadowConfigBuilder, DirectionalLightShadowMap},
    prelude::*,
};

const SUN_ILLUMINANCE: f32 = 9_000.0;
const SKYLIGHT_BRIGHTNESS: f32 = 450.0;
/// Where the sun shines from: up and to the left of the camera, so shadows fall down-right.
const SUN_FROM: Vec3 = Vec3::new(-0.6, 1.2, 0.35);

pub struct EnvironmentPlugin;

impl Plugin for EnvironmentPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(palette::BACKGROUND))
            .insert_resource(GlobalAmbientLight {
                color: palette::SKYLIGHT,
                brightness: SKYLIGHT_BRIGHTNESS,
                ..default()
            })
            .insert_resource(DirectionalLightShadowMap { size: 4096 })
            .add_systems(Startup, spawn_sun);
    }
}

fn spawn_sun(mut commands: Commands) {
    commands.spawn((
        DirectionalLight {
            color: palette::SUN,
            illuminance: SUN_ILLUMINANCE,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_translation(SUN_FROM).looking_at(Vec3::ZERO, Vec3::Y),
        CascadeShadowConfigBuilder {
            num_cascades: 2,
            minimum_distance: 1.0,
            first_cascade_far_bound: 40.0,
            maximum_distance: 140.0,
            ..default()
        }
        .build(),
    ));
}
