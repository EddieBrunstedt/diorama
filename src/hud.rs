//! On-screen text: the Command Center's health and the size of the horde at the top, a controls
//! hint in the bottom-left corner, and a small HP bar over the wall under the cursor. Undo and
//! redo dim when there's nothing to undo or redo.

use crate::build::history::History;
use crate::camera::MainCamera;
use crate::command_center::CommandCenterHealth;
use crate::grid::{Grid, cell_center};
use crate::horde::{Brute, BruteQueue, SpawnQueue, Swarmer};
use crate::palette;
use crate::pointer::{Pointer, PointerSystems, column_top};
use bevy::prelude::*;

const FONT_SIZE: f32 = 15.0;
/// How faint a hint line is while it can't do anything.
const DIMMED_ALPHA: f32 = 0.3;
/// The health bar's size, and the fraction of HP below which it turns red.
const BAR_SIZE: Vec2 = Vec2::new(260.0, 10.0);
const LOW_HEALTH: f32 = 0.3;
/// A hovered wall's HP bar: its size (logical pixels), and how far above the wall's top it floats
/// (world units).
const WALL_BAR_SIZE: Vec2 = Vec2::new(40.0, 5.0);
const WALL_BAR_LIFT: f32 = 0.35;
const WALL_BAR_FONT_SIZE: f32 = 12.0;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (spawn_hint, spawn_status, spawn_wall_bar))
            .add_systems(
                Update,
                (
                    dim_undo_redo,
                    show_status,
                    show_wall_bar.after(PointerSystems),
                ),
            );
    }
}

#[derive(Component)]
struct HealthFill;

#[derive(Component)]
struct HealthText;

#[derive(Component)]
struct SwarmerCount;

fn spawn_status(mut commands: Commands) {
    let text = |value: &str| {
        (
            Text::new(value),
            TextFont::from_font_size(FONT_SIZE),
            TextColor(palette::HINT_TEXT),
            Pickable::IGNORE,
        )
    };
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(12.0),
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(4.0),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|status| {
            status.spawn((HealthText, text("")));
            status
                .spawn((
                    Node {
                        width: Val::Px(BAR_SIZE.x),
                        height: Val::Px(BAR_SIZE.y),
                        ..default()
                    },
                    BackgroundColor(palette::HEALTH_EMPTY),
                    Pickable::IGNORE,
                ))
                .with_child((
                    HealthFill,
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(palette::HEALTH),
                    Pickable::IGNORE,
                ));
            status.spawn((SwarmerCount, text("")));
        });
}

fn show_status(
    health: Res<CommandCenterHealth>,
    queue: Res<SpawnQueue>,
    brute_queue: Res<BruteQueue>,
    swarmers: Query<(), With<Swarmer>>,
    brutes: Query<(), With<Brute>>,
    mut fill: Single<(&mut Node, &mut BackgroundColor), With<HealthFill>>,
    mut health_text: Single<&mut Text, (With<HealthText>, Without<SwarmerCount>)>,
    mut count_text: Single<&mut Text, (With<SwarmerCount>, Without<HealthText>)>,
) {
    if health.is_changed() {
        let fraction = health.hp as f32 / health.max.max(1) as f32;
        let (node, color) = &mut *fill;
        node.width = Val::Percent(fraction * 100.0);
        color.0 = if fraction < LOW_HEALTH {
            palette::HEALTH_LOW
        } else {
            palette::HEALTH
        };
        health_text.0 = format!("Command Center  {} / {}", health.hp, health.max);
    }
    let count = |name: &str, alive: usize, waiting: u32| {
        if waiting > 0 {
            format!("{name}  {alive}  (+{waiting} coming)")
        } else {
            format!("{name}  {alive}")
        }
    };
    let count = format!(
        "{}      {}",
        count("Swarmers", swarmers.iter().len(), queue.waiting),
        count("Brutes", brutes.iter().len(), brute_queue.waiting),
    );
    if count_text.0 != count {
        count_text.0 = count;
    }
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum HintLine {
    Always,
    Undo,
    Redo,
}

fn spawn_hint(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(14.0),
                bottom: Val::Px(12.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(2.0),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|hint| {
            for (line, text) in [
                (HintLine::Always, "Left click: wall (drag for a line)"),
                (HintLine::Always, "Right click: remove (drag for a line)"),
                (
                    HintLine::Always,
                    "Space: release swarmers    B: release brutes",
                ),
                (HintLine::Undo, "Ctrl+Z: undo"),
                (HintLine::Redo, "Ctrl+Shift+Z: redo"),
                (HintLine::Always, "WASD / middle drag: pan    Scroll: zoom"),
            ] {
                hint.spawn((
                    line,
                    Text::new(text),
                    TextFont::from_font_size(FONT_SIZE),
                    TextColor(palette::HINT_TEXT),
                    Pickable::IGNORE,
                ));
            }
        });
}

fn dim_undo_redo(history: Res<History>, mut lines: Query<(&HintLine, &mut TextColor)>) {
    if !history.is_changed() {
        return;
    }
    for (line, mut color) in &mut lines {
        let usable = match line {
            HintLine::Always => true,
            HintLine::Undo => history.can_undo(),
            HintLine::Redo => history.can_redo(),
        };
        let alpha = palette::HINT_TEXT.alpha() * if usable { 1.0 } else { DIMMED_ALPHA };
        color.set_if_neq(TextColor(palette::HINT_TEXT.with_alpha(alpha)));
    }
}

#[derive(Component)]
struct WallBar;

#[derive(Component)]
struct WallBarFill;

#[derive(Component)]
struct WallBarText;

fn spawn_wall_bar(mut commands: Commands) {
    commands
        .spawn((
            WallBar,
            Node {
                position_type: PositionType::Absolute,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(1.0),
                ..default()
            },
            Visibility::Hidden,
            Pickable::IGNORE,
        ))
        .with_children(|bar| {
            bar.spawn((
                WallBarText,
                Text::new(""),
                TextFont::from_font_size(WALL_BAR_FONT_SIZE),
                TextColor(palette::HINT_TEXT),
                Pickable::IGNORE,
            ));
            bar.spawn((
                Node {
                    width: Val::Px(WALL_BAR_SIZE.x),
                    height: Val::Px(WALL_BAR_SIZE.y),
                    padding: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(palette::WALL_BAR_BACK),
                Pickable::IGNORE,
            ))
            .with_child((
                WallBarFill,
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(palette::HEALTH),
                Pickable::IGNORE,
            ));
        });
}

/// Floats an HP bar over the wall under the cursor.
fn show_wall_bar(
    pointer: Res<Pointer>,
    grid: Res<Grid>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut bar: Single<(&mut Node, &mut Visibility, &ComputedNode), With<WallBar>>,
    mut fill: Single<(&mut Node, &mut BackgroundColor), (With<WallBarFill>, Without<WallBar>)>,
    mut text: Single<&mut Text, With<WallBarText>>,
) {
    let (node, visibility, computed) = &mut *bar;
    let (camera, transform) = *camera;
    // Only structures the horde can break have HP to show.
    let above = pointer.cell.and_then(|cell| {
        let health = grid.structure(cell)?.health()?;
        let top = cell_center(cell) + Vec3::Y * (column_top(&grid, cell) + WALL_BAR_LIFT);
        let at = camera.world_to_viewport(transform, top).ok()?;
        Some((at, health))
    });
    let Some((at, (hp, max))) = above else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    visibility.set_if_neq(Visibility::Inherited);
    // Centered over the wall, sitting on the point above it.
    let size = computed.size() * computed.inverse_scale_factor();
    node.left = Val::Px(at.x - size.x / 2.0);
    node.top = Val::Px(at.y - size.y);
    let fraction = hp as f32 / max.max(1) as f32;
    let (fill_node, color) = &mut *fill;
    fill_node.width = Val::Percent(fraction * 100.0);
    color.0 = if fraction < LOW_HEALTH {
        palette::HEALTH_LOW
    } else {
        palette::HEALTH
    };
    let label = format!("{hp} / {max}");
    if text.0 != label {
        text.0 = label;
    }
}
