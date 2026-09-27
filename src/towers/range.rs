//! Range rings: how far a tower reaches, shown around the cell under the cursor when a click
//! would build a tower there (in the build phase), and around a tower the cursor is on. A mortar
//! also shows how close it can't reach.

use crate::build::{ActiveStroke, BuildSystems, Tool};
use crate::grid::{Grid, Kind, cell_center};
use crate::palette;
use crate::pointer::Pointer;
use crate::shapes::ring_mesh;
use crate::tuning::towers::{AUTOCANNON_RANGE, FLAMER_RANGE, MORTAR_MIN_RANGE, MORTAR_RANGE};
use crate::waves::Phase;
use bevy::{light::NotShadowCaster, platform::collections::HashMap, prelude::*};

/// How thick a ring's line is, and how high over the ground it floats.
const LINE: f32 = 0.06;
const LIFT: f32 = 0.03;

pub struct RangePlugin;

impl Plugin for RangePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_rings)
            .add_systems(Update, show_rings.after(BuildSystems));
    }
}

/// The ranges `kind` reaches between: nothing closer than the first, nothing further than the
/// second.
pub fn reach(kind: Kind) -> Option<(f32, f32)> {
    match kind {
        Kind::Wall => None,
        Kind::Autocannon => Some((0.0, AUTOCANNON_RANGE)),
        Kind::Flamer => Some((0.0, FLAMER_RANGE)),
        Kind::Mortar => Some((MORTAR_MIN_RANGE, MORTAR_RANGE)),
    }
}

#[derive(Component)]
enum Ring {
    Outer,
    Inner,
}

/// A ring mesh per range (keyed by the radius' bits).
#[derive(Resource)]
struct RingMeshes(HashMap<u32, Handle<Mesh>>);

impl RingMeshes {
    fn get(&self, radius: f32) -> Option<Handle<Mesh>> {
        self.0.get(&radius.to_bits()).cloned()
    }
}

fn spawn_rings(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut rings = HashMap::new();
    for (min, max) in Kind::ALL.into_iter().filter_map(reach) {
        for radius in [min, max].into_iter().filter(|&radius| radius > 0.0) {
            rings
                .entry(radius.to_bits())
                .or_insert_with(|| meshes.add(ring_mesh(radius - LINE, radius, 96)));
        }
    }
    let first = rings.values().next().cloned().unwrap_or_default();
    commands.insert_resource(RingMeshes(rings));
    for (ring, color) in [
        (Ring::Outer, palette::RANGE_RING),
        (Ring::Inner, palette::RANGE_MIN),
    ] {
        let material = materials.add(StandardMaterial {
            base_color: color,
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        });
        commands.spawn((
            ring,
            Mesh3d(first.clone()),
            MeshMaterial3d(material),
            Transform::default(),
            Visibility::Hidden,
            NotShadowCaster,
        ));
    }
}

fn show_rings(
    pointer: Res<Pointer>,
    phase: Res<State<Phase>>,
    grid: Res<Grid>,
    tool: Res<Tool>,
    active: Res<ActiveStroke>,
    ring_meshes: Res<RingMeshes>,
    mut rings: Query<(&Ring, &mut Mesh3d, &mut Transform, &mut Visibility)>,
) {
    // The tower under the cursor, or the one a click would build.
    let shown = pointer
        .cell
        .filter(|_| active.0.is_none())
        .and_then(|cell| {
            let kind = match grid.structure(cell) {
                Some(structure) => structure.kind()?,
                None if *phase.get() == Phase::Build && grid.check_build(cell).is_ok() => tool.0,
                None => return None,
            };
            Some((cell, reach(kind)?))
        });
    for (ring, mut mesh, mut transform, mut visibility) in &mut rings {
        let radius = shown.map(|(cell, (min, max))| {
            let radius = match ring {
                Ring::Outer => max,
                Ring::Inner => min,
            };
            (cell, radius)
        });
        match radius {
            Some((cell, radius)) if let Some(handle) = ring_meshes.get(radius) => {
                visibility.set_if_neq(Visibility::Inherited);
                transform.translation = cell_center(cell) + Vec3::Y * LIFT;
                if mesh.0 != handle {
                    mesh.0 = handle;
                }
            }
            _ => {
                visibility.set_if_neq(Visibility::Hidden);
            }
        }
    }
}
