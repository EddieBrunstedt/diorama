//! On-screen text: the Command Center's health and the number of swarmers at the top, and a
//! controls hint in the bottom-left corner. Undo and redo dim when there's nothing to undo or
//! redo.

use crate::build::history::History;
use crate::command_center::CommandCenterHealth;
use crate::horde::{SpawnQueue, Swarmer};
use crate::palette;
use bevy::prelude::*;

const FONT_SIZE: f32 = 15.0;
/// How faint a hint line is while it can't do anything.
const DIMMED_ALPHA: f32 = 0.3;
/// The health bar's size, and the fraction of HP below which it turns red.
const BAR_SIZE: Vec2 = Vec2::new(260.0, 10.0);
const LOW_HEALTH: f32 = 0.3;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (spawn_hint, spawn_status))
            .add_systems(Update, (dim_undo_redo, show_status));
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
    swarmers: Query<(), With<Swarmer>>,
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
    let alive = swarmers.iter().len();
    let count = if queue.waiting > 0 {
        format!("Swarmers  {alive}  (+{} coming)", queue.waiting)
    } else {
        format!("Swarmers  {alive}")
    };
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
                (HintLine::Always, "Space: release swarmers"),
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
