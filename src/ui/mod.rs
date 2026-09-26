//! The on-screen UI: the order queue, the wood counter, numbered markers over order targets, a
//! label at the cursor, and popups rising from the island.

mod hud;
mod icons;
mod queue_panel;

use crate::keeper::{OrderId, OrderKind, PlanSystems, PlannedOrder, Problem};
use crate::palette;
use crate::tuning::TOWER_WOOD_COST;
use bevy::prelude::*;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Popup>()
            .add_message::<FlashOrder>()
            .add_message::<OrderFinished>()
            .init_resource::<icons::Icons>()
            .add_systems(Startup, (hud::spawn_hud, queue_panel::spawn_panel))
            .add_systems(
                Update,
                (
                    queue_panel::sync_cards,
                    queue_panel::animate_cards,
                    queue_panel::update_tooltip,
                    hud::update_wood_counter,
                    hud::spawn_popups,
                    hud::animate_popups,
                    hud::update_markers,
                    hud::update_cursor_label,
                )
                    .chain()
                    .after(PlanSystems),
            );
    }
}

/// Text that pops up over a spot on the island and floats away.
#[derive(Message, Clone, Debug)]
pub struct Popup {
    pub at: Vec3,
    pub text: String,
    pub color: Color,
}

impl Popup {
    pub fn new(at: Vec3, text: impl Into<String>, color: Color) -> Self {
        Self {
            at,
            text: text.into(),
            color,
        }
    }

    pub fn problem(at: Vec3, text: impl Into<String>) -> Self {
        Self::new(at, text, palette::PROBLEM)
    }
}

/// Draws attention to an order's card (e.g. when its target is clicked again).
#[derive(Message, Clone, Copy, Debug)]
pub struct FlashOrder(pub OrderId);

/// An order was carried out (as opposed to removed), so its card leaves with a flourish.
#[derive(Message, Clone, Copy, Debug)]
pub struct OrderFinished(pub OrderId);

pub fn order_color(kind: &OrderKind) -> Color {
    match kind {
        OrderKind::Chop(_) => palette::ORDER_CHOP,
        OrderKind::Build(_) => palette::ORDER_BUILD,
        OrderKind::Walk(_) => palette::ORDER_WALK,
    }
}

pub fn order_title(kind: &OrderKind) -> &'static str {
    match kind {
        OrderKind::Chop(_) => "Chop tree",
        OrderKind::Build(_) => "Build tower",
        OrderKind::Walk(_) => "Walk here",
    }
}

/// The wood change shown on a card, e.g. "+1" or "-3".
pub fn wood_label(kind: &OrderKind) -> String {
    match kind.wood_change() {
        0 => String::new(),
        change => format!("{change:+}"),
    }
}

/// Explains a planned order's problem, given the wood in the backpack just before it.
pub fn problem_text(planned: &PlannedOrder, wood_before: u32) -> Option<String> {
    Some(match planned.problem? {
        Problem::NotEnoughWood => {
            format!("Not enough wood: needs {TOWER_WOOD_COST}, will have {wood_before}")
        }
        Problem::BackpackFull => "Backpack will be full".to_string(),
        Problem::Unreachable => Problem::Unreachable.message().to_string(),
    })
}

/// Rounded paper panel styling shared by the HUD.
fn paper(radius: f32) -> (BackgroundColor, BorderColor, BoxShadow) {
    (
        BackgroundColor(palette::UI_PAPER.with_alpha(0.92)),
        BorderColor::all(palette::UI_PAPER_DARK),
        BoxShadow::new(
            Color::BLACK.with_alpha(0.18),
            Val::Px(0.0),
            Val::Px(3.0),
            Val::Px(0.0),
            Val::Px(radius / 2.0),
        ),
    )
}

fn font(size: f32) -> TextFont {
    TextFont::from_font_size(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wood_labels_are_signed() {
        assert_eq!(wood_label(&OrderKind::Build(IVec2::ZERO)), "-3");
        assert_eq!(wood_label(&OrderKind::Walk(Vec2::ZERO)), "");
    }
}
