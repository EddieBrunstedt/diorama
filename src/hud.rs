//! On-screen text: a controls hint in the bottom-left corner. Undo and redo dim when there's
//! nothing to undo or redo.

use crate::build::history::History;
use crate::palette;
use bevy::prelude::*;

const FONT_SIZE: f32 = 15.0;
/// How faint a hint line is while it can't do anything.
const DIMMED_ALPHA: f32 = 0.3;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_hint)
            .add_systems(Update, dim_undo_redo);
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
