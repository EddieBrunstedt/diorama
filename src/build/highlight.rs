//! The hover highlight: corner brackets and a faint fill on the cell under the cursor, or on
//! every cell of a drag. The color says what a click does there: build (cyan), nothing (red), or
//! remove (orange, over a wall).

use super::{ActiveStroke, BuildDenied, BuildSystems, history::StrokeKind};
use crate::grid::{Blocked, Grid};
use crate::juice::Squash;
use crate::palette;
use crate::pointer::{Pointer, column_top};
use crate::shapes::{BoxMesh, bracket_mesh};
use crate::tuning::juice::{DENIED_FLASH_SECONDS, HOVER_POP};
use bevy::{light::NotShadowCaster, prelude::*};

/// Bracket shape: half size, corner arm length, thickness and height.
const BRACKET: (f32, f32, f32, f32) = (0.5, 0.2, 0.07, 0.05);
/// How much the brackets glow past their color, and how see-through the fill is.
const BRACKET_BRIGHTNESS: f32 = 1.6;
const FILL_ALPHA: f32 = 0.2;
/// Lift above whatever the cell's top is, so the fill doesn't flicker into it.
const LIFT: f32 = 0.012;

pub struct HighlightPlugin;

impl Plugin for HighlightPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HighlightAssets>()
            .init_resource::<Slots>()
            .add_systems(Update, show_highlights.after(BuildSystems));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Look {
    Build,
    Blocked,
    Remove,
}

#[derive(Resource)]
struct HighlightAssets {
    bracket: Handle<Mesh>,
    fill: Handle<Mesh>,
    /// Bracket and fill materials for each look, in `Look` order.
    materials: [(Handle<StandardMaterial>, Handle<StandardMaterial>); 3],
}

impl HighlightAssets {
    fn materials(&self, look: Look) -> &(Handle<StandardMaterial>, Handle<StandardMaterial>) {
        &self.materials[look as usize]
    }
}

impl FromWorld for HighlightAssets {
    fn from_world(world: &mut World) -> Self {
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        let (outer, arm, thickness, height) = BRACKET;
        let bracket = meshes.add(bracket_mesh(outer, arm, thickness, height));
        let mut quad = BoxMesh::default();
        quad.quad(Vec2::splat(-0.5), Vec2::splat(0.5), 0.0, Color::WHITE);
        let fill = meshes.add(quad.build());
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        let mut pair = |color: Color| {
            let bright = color.to_linear() * BRACKET_BRIGHTNESS;
            (
                materials.add(StandardMaterial {
                    base_color: bright.with_alpha(1.0).into(),
                    unlit: true,
                    ..default()
                }),
                materials.add(StandardMaterial {
                    base_color: color.with_alpha(FILL_ALPHA),
                    alpha_mode: AlphaMode::Blend,
                    unlit: true,
                    ..default()
                }),
            )
        };
        Self {
            bracket,
            fill,
            materials: [
                pair(palette::HOVER_OK),
                pair(palette::HOVER_BLOCKED),
                pair(palette::HOVER_REMOVE),
            ],
        }
    }
}

/// One highlighted cell: the brackets, with the fill as a child.
#[derive(Component)]
struct Slot {
    fill: Entity,
}

/// Highlight entities, reused from frame to frame. The first one is always the cursor's cell.
#[derive(Resource, Default)]
struct Slots(Vec<Entity>);

/// What to highlight, the cursor's cell first.
fn wanted(grid: &Grid, pointer: &Pointer, active: &ActiveStroke) -> Vec<(IVec2, Look)> {
    match &active.0 {
        None => pointer
            .cell
            .map(|cell| {
                let look = match grid.check_build(cell) {
                    Ok(()) => Look::Build,
                    Err(Blocked::Occupied(_)) => Look::Remove,
                    Err(_) => Look::Blocked,
                };
                vec![(cell, look)]
            })
            .unwrap_or_default(),
        Some((stroke, _)) => stroke
            .cells()
            .into_iter()
            .rev()
            .map(|cell| {
                let look = match stroke.kind {
                    StrokeKind::Remove => Look::Remove,
                    StrokeKind::Place(_) if stroke.changed(cell) => Look::Build,
                    StrokeKind::Place(_) => Look::Blocked,
                };
                (cell, look)
            })
            .collect(),
    }
}

fn show_highlights(
    mut commands: Commands,
    time: Res<Time<Real>>,
    grid: Res<Grid>,
    pointer: Res<Pointer>,
    active: Res<ActiveStroke>,
    assets: Res<HighlightAssets>,
    mut denials: MessageReader<BuildDenied>,
    mut slots: ResMut<Slots>,
    mut brackets: Query<(
        &Slot,
        &mut Transform,
        &mut Visibility,
        &mut MeshMaterial3d<StandardMaterial>,
        &mut Squash,
    )>,
    mut fills: Query<&mut MeshMaterial3d<StandardMaterial>, Without<Slot>>,
    mut last_cursor: Local<Option<IVec2>>,
    mut flash: Local<Option<(IVec2, f32)>>,
) {
    let mut wanted = wanted(&grid, &pointer, &active);

    // A refused click flashes the cell red for a moment and pops the brackets.
    let mut pop = None;
    for denial in denials.read() {
        *flash = Some((denial.cell, time.elapsed_secs() + DENIED_FLASH_SECONDS));
        pop = Some(-HOVER_POP * 1.6);
    }
    if let Some((cell, until)) = *flash {
        if time.elapsed_secs() > until {
            *flash = None;
        } else if let Some(first) = wanted.first_mut()
            && first.0 == cell
        {
            first.1 = Look::Blocked;
        }
    }
    let cursor = wanted.first().map(|&(cell, _)| cell);
    if cursor != *last_cursor && cursor.is_some() {
        pop = pop.or(Some(-HOVER_POP));
    }
    *last_cursor = cursor;

    while slots.0.len() < wanted.len() {
        let fill = commands
            .spawn((
                Mesh3d(assets.fill.clone()),
                MeshMaterial3d(assets.materials(Look::Build).1.clone()),
                Transform::default(),
                NotShadowCaster,
            ))
            .id();
        let slot = commands
            .spawn((
                Slot { fill },
                Mesh3d(assets.bracket.clone()),
                MeshMaterial3d(assets.materials(Look::Build).0.clone()),
                Transform::default(),
                Visibility::Hidden,
                Squash::new(Vec3::ONE),
                NotShadowCaster,
            ))
            .add_child(fill)
            .id();
        slots.0.push(slot);
    }

    for (index, &entity) in slots.0.iter().enumerate() {
        // Slots spawned this frame show up next frame, a frame late at most.
        let Ok((slot, mut transform, mut visibility, mut bracket_material, mut squash)) =
            brackets.get_mut(entity)
        else {
            continue;
        };
        let Some(&(cell, look)) = wanted.get(index) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        visibility.set_if_neq(Visibility::Inherited);
        let center = crate::grid::cell_center(cell);
        transform.translation = center + Vec3::Y * (column_top(&grid, cell) + LIFT);
        let (bracket, fill) = assets.materials(look);
        if bracket_material.0 != *bracket {
            bracket_material.0 = bracket.clone();
            if let Ok(mut fill_material) = fills.get_mut(slot.fill) {
                fill_material.0 = fill.clone();
            }
        }
        if index == 0
            && let Some(amount) = pop
        {
            squash.pulse(amount);
        }
    }
}
