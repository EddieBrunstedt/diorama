//! The build panel in the bottom-left corner, in the build phase: a button per thing to
//! build, with its hotkey and price. The chosen one is outlined, and prices the money doesn't
//! cover turn red. Clicking a button picks it, like its hotkey.

use super::{TOOL_KEYS, Tool};
use crate::economy::{Wallet, price};
use crate::grid::Kind;
use crate::palette;
use crate::waves::Phase;
use bevy::prelude::*;

const FONT_SIZE: f32 = 14.0;
const PRICE_FONT_SIZE: f32 = 13.0;
const BUTTON_WIDTH: f32 = 116.0;

pub struct PanelPlugin;

impl Plugin for PanelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UiHovered>()
            .add_systems(Startup, spawn_panel)
            .add_systems(OnEnter(Phase::Build), show_panel::<true>)
            .add_systems(OnExit(Phase::Build), show_panel::<false>)
            .add_systems(
                PreUpdate,
                (press_buttons, hover_ui).after(bevy::ui::UiSystems::Focus),
            )
            .add_systems(Update, show_buttons);
    }
}

/// Whether the cursor is over a button (the panel's, or the HUD's), so clicks there don't build
/// on the map behind it.
#[derive(Resource, Default, Debug, PartialEq, Eq)]
pub struct UiHovered(pub bool);

#[derive(Component)]
struct Panel;

#[derive(Component, Clone, Copy)]
struct PanelButton(Kind);

#[derive(Component, Clone, Copy)]
struct PriceText(Kind);

fn spawn_panel(mut commands: Commands) {
    let text = |value: String, size: f32, color: Color| {
        (
            Text::new(value),
            TextFont::from_font_size(size),
            TextColor(color),
            Pickable::IGNORE,
        )
    };
    commands
        .spawn((
            Panel,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(12.0),
                // On the left, leaving the right to the wave box (see `hud`).
                left: Val::Px(14.0),
                column_gap: Val::Px(6.0),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|panel| {
            for ((_, key), kind) in TOOL_KEYS.into_iter().zip(Kind::ALL) {
                panel
                    .spawn((
                        PanelButton(kind),
                        Button,
                        Node {
                            min_width: Val::Px(BUTTON_WIDTH),
                            padding: UiRect::axes(Val::Px(8.0), Val::Px(5.0)),
                            border: UiRect::all(Val::Px(2.0)),
                            border_radius: BorderRadius::all(Val::Px(4.0)),
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(1.0),
                            ..default()
                        },
                        BackgroundColor(palette::PANEL),
                        BorderColor::all(palette::PANEL_BORDER),
                    ))
                    .with_children(|button| {
                        button.spawn(text(
                            format!("{key}  {}", kind.name()),
                            FONT_SIZE,
                            palette::HINT_TEXT,
                        ));
                        button.spawn((
                            PriceText(kind),
                            text(format!("{}", price(kind)), PRICE_FONT_SIZE, palette::MONEY),
                        ));
                    });
            }
        });
}

/// The panel is only there in the build phase.
fn show_panel<const SHOWN: bool>(mut panels: Query<&mut Visibility, With<Panel>>) {
    for mut panel in &mut panels {
        panel.set_if_neq(if SHOWN {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

fn press_buttons(buttons: Query<(&PanelButton, &Interaction)>, mut tool: ResMut<Tool>) {
    for (button, interaction) in &buttons {
        if *interaction == Interaction::Pressed {
            tool.set_if_neq(Tool(button.0));
        }
    }
}

fn hover_ui(
    buttons: Query<(&Interaction, &InheritedVisibility), With<Button>>,
    mut hovered: ResMut<UiHovered>,
) {
    let over = buttons
        .iter()
        .any(|(interaction, visible)| visible.get() && *interaction != Interaction::None);
    hovered.set_if_neq(UiHovered(over));
}

fn show_buttons(
    tool: Res<Tool>,
    wallet: Res<Wallet>,
    mut buttons: Query<(
        &PanelButton,
        &Interaction,
        &mut BorderColor,
        &mut BackgroundColor,
    )>,
    mut prices: Query<(&PriceText, &mut TextColor)>,
) {
    for (button, interaction, mut border, mut background) in &mut buttons {
        let chosen = button.0 == tool.0;
        border.set_if_neq(BorderColor::all(if chosen {
            palette::PANEL_CHOSEN
        } else {
            palette::PANEL_BORDER
        }));
        let fill = match interaction {
            Interaction::None => palette::PANEL,
            _ => palette::PANEL_HOVER,
        };
        background.set_if_neq(BackgroundColor(fill));
    }
    for (price_text, mut color) in &mut prices {
        let affordable = wallet.money() >= price(price_text.0);
        color.set_if_neq(TextColor(if affordable {
            palette::MONEY
        } else {
            palette::BROKE
        }));
    }
}
