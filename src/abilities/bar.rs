//! The ability bar in the bottom-left corner, in the wave phase (where the build panel is in the
//! build phase): a slot per ability, with its hotkey and name, a pip per charge (lit when ready;
//! past a handful, the text counts them), and a bar that fills as the next charge reloads, with
//! the seconds left.
//!
//! A ready slot has a bright border, the one being aimed a cyan one. A slot pops when a charge
//! comes back, and flashes red when it can't be used. With no building to give it, the slot is
//! dimmed and says what to build. Clicking a slot is the same as its hotkey.

use super::{
    Ability, AbilityDenied, AbilityPressed, AbilityReady, AbilitySystems, Aiming, Arsenal,
};
use crate::palette;
use crate::tuning::juice::{ABILITY_DENIED_SECONDS, ABILITY_READY_POP};
use crate::waves::Phase;
use bevy::prelude::*;

const FONT_SIZE: f32 = 14.0;
const STATUS_FONT_SIZE: f32 = 12.0;
const SLOT_WIDTH: f32 = 196.0;
const PIP_SIZE: f32 = 8.0;
const BAR_HEIGHT: f32 = 5.0;
/// How fast a slot's pop settles (per second).
const POP_DECAY: f32 = 5.0;
/// Beyond this many charges, a slot says how many instead of showing a pip for each.
const MAX_PIPS: usize = 6;
/// How faint a slot is with no building to give it.
const MISSING_ALPHA: f32 = 0.4;

pub struct BarPlugin;

impl Plugin for BarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_bar)
            .add_systems(OnEnter(Phase::Wave), show_bar::<true>)
            .add_systems(OnExit(Phase::Wave), show_bar::<false>)
            .add_systems(PreUpdate, press_slots.after(bevy::ui::UiSystems::Focus))
            .add_systems(Update, show_slots.after(AbilitySystems));
    }
}

#[derive(Component)]
struct Bar;

#[derive(Component)]
struct Slot {
    ability: Ability,
    /// How much it's swelled from a charge coming back, easing out.
    pop: f32,
    /// Seconds left of flashing red.
    denied: f32,
}

/// Parts of a slot, each knowing its ability.
#[derive(Component, Clone, Copy)]
enum SlotPart {
    Name(Ability),
    Pips(Ability),
    Fill(Ability),
    Status(Ability),
}

#[derive(Component)]
struct Pip;

fn text(value: String, size: f32) -> impl Bundle {
    (
        Text::new(value),
        TextFont::from_font_size(size),
        TextColor(palette::HINT_TEXT),
        Pickable::IGNORE,
    )
}

fn spawn_bar(mut commands: Commands) {
    commands
        .spawn((
            Bar,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(12.0),
                left: Val::Px(14.0),
                column_gap: Val::Px(6.0),
                ..default()
            },
            Visibility::Hidden,
            Pickable::IGNORE,
        ))
        .with_children(|bar| {
            for ability in Ability::ALL {
                bar.spawn((
                    Slot {
                        ability,
                        pop: 0.0,
                        denied: 0.0,
                    },
                    Button,
                    Node {
                        width: Val::Px(SLOT_WIDTH),
                        padding: UiRect::axes(Val::Px(8.0), Val::Px(6.0)),
                        border: UiRect::all(Val::Px(2.0)),
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(4.0),
                        ..default()
                    },
                    BackgroundColor(palette::PANEL),
                    BorderColor::all(palette::PANEL_BORDER),
                    UiTransform::IDENTITY,
                ))
                .with_children(|slot| {
                    // Hotkey and name, with the charge pips on the right.
                    slot.spawn((
                        Node {
                            justify_content: JustifyContent::SpaceBetween,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        Pickable::IGNORE,
                    ))
                    .with_children(|row| {
                        let (_, key) = ability.key();
                        row.spawn((
                            SlotPart::Name(ability),
                            text(format!("{key}  {}", ability.name()), FONT_SIZE),
                        ));
                        row.spawn((
                            SlotPart::Pips(ability),
                            Node {
                                column_gap: Val::Px(3.0),
                                ..default()
                            },
                            Pickable::IGNORE,
                        ));
                    });
                    // The reload bar, and what's left.
                    slot.spawn((
                        Node {
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(6.0),
                            ..default()
                        },
                        Pickable::IGNORE,
                    ))
                    .with_children(|row| {
                        row.spawn((
                            Node {
                                flex_grow: 1.0,
                                height: Val::Px(BAR_HEIGHT),
                                ..default()
                            },
                            BackgroundColor(palette::ABILITY_TRACK),
                            Pickable::IGNORE,
                        ))
                        .with_child((
                            SlotPart::Fill(ability),
                            Node {
                                width: Val::Percent(100.0),
                                height: Val::Percent(100.0),
                                ..default()
                            },
                            BackgroundColor(palette::ABILITY_FULL),
                            Pickable::IGNORE,
                        ));
                        row.spawn((
                            SlotPart::Status(ability),
                            text(String::new(), STATUS_FONT_SIZE),
                        ));
                    });
                });
            }
        });
}

/// The bar is only there in the wave phase.
fn show_bar<const SHOWN: bool>(mut bars: Query<&mut Visibility, With<Bar>>) {
    for mut bar in &mut bars {
        bar.set_if_neq(if SHOWN {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

fn press_slots(
    slots: Query<(&Slot, &Interaction), Changed<Interaction>>,
    mut pressed: MessageWriter<AbilityPressed>,
) {
    for (slot, interaction) in &slots {
        if *interaction == Interaction::Pressed {
            pressed.write(AbilityPressed(slot.ability));
        }
    }
}

/// What a slot says under its name: how long until the next charge, or what to build. With more
/// than one charge, how many are ready.
fn status(ability: Ability, arsenal: &Arsenal) -> String {
    let charges = arsenal.charges(ability);
    if charges.sources() == 0 {
        return format!("needs {}", ability.source());
    }
    let ready = match charges.ready() {
        _ if charges.sources() == 1 => "ready".to_string(),
        count => format!("{count} ready"),
    };
    match (charges.ready(), charges.next_in()) {
        (0, Some(left)) => format!("{}s", left.ceil()),
        (_, Some(left)) => format!("{ready}, next {}s", left.ceil()),
        _ => ready,
    }
}

fn show_slots(
    mut commands: Commands,
    time: Res<Time<Real>>,
    arsenal: Res<Arsenal>,
    aiming: Res<Aiming>,
    mut ready: MessageReader<AbilityReady>,
    mut denied: MessageReader<AbilityDenied>,
    mut slots: Query<(
        &mut Slot,
        &Interaction,
        &mut BorderColor,
        &mut BackgroundColor,
        &mut UiTransform,
    )>,
    mut parts: Query<
        (
            Entity,
            &SlotPart,
            Option<&mut Text>,
            Option<&mut TextColor>,
            Option<&mut Node>,
            Option<&mut BackgroundColor>,
            Option<&Children>,
        ),
        Without<Slot>,
    >,
    mut pips: Query<&mut BackgroundColor, (With<Pip>, Without<Slot>, Without<SlotPart>)>,
) {
    let dt = time.delta_secs();
    let ready: Vec<Ability> = ready.read().map(|ready| ready.0).collect();
    let denied: Vec<Ability> = denied.read().map(|denied| denied.0).collect();
    for (mut slot, interaction, mut border, mut background, mut transform) in &mut slots {
        let ability = slot.ability;
        if ready.contains(&ability) {
            slot.pop = ABILITY_READY_POP;
        }
        if denied.contains(&ability) {
            slot.denied = ABILITY_DENIED_SECONDS;
        }
        slot.pop = (slot.pop - slot.pop * POP_DECAY * dt).max(0.0);
        slot.denied = (slot.denied - dt).max(0.0);
        // The denied flash shakes a little, too.
        let shake = if slot.denied > 0.0 {
            (slot.denied * 60.0).sin() * 3.0 * slot.denied / ABILITY_DENIED_SECONDS
        } else {
            0.0
        };
        transform.scale = Vec2::splat(1.0 + slot.pop);
        transform.translation = Val2::px(shake, 0.0);
        let charges = arsenal.charges(ability);
        let color = if slot.denied > 0.0 {
            palette::BROKE
        } else if aiming.0 == Some(ability) {
            palette::PANEL_CHOSEN
        } else if charges.ready() > 0 {
            palette::ABILITY_READY
        } else {
            palette::PANEL_BORDER
        };
        border.set_if_neq(BorderColor::all(color));
        let fill = match (interaction, aiming.0 == Some(ability)) {
            (Interaction::None, false) => palette::PANEL,
            _ => palette::PANEL_HOVER,
        };
        background.set_if_neq(BackgroundColor(fill));
    }

    for (entity, part, text, text_color, node, background, children) in &mut parts {
        match *part {
            SlotPart::Name(ability) => {
                let alpha = if arsenal.charges(ability).sources() == 0 {
                    MISSING_ALPHA
                } else {
                    1.0
                };
                if let Some(mut color) = text_color {
                    let faded = palette::HINT_TEXT.with_alpha(palette::HINT_TEXT.alpha() * alpha);
                    color.set_if_neq(TextColor(faded));
                }
            }
            SlotPart::Status(ability) => {
                let label = status(ability, &arsenal);
                if let Some(mut text) = text
                    && text.0 != label
                {
                    text.0 = label;
                }
            }
            SlotPart::Fill(ability) => {
                let charges = arsenal.charges(ability);
                let (width, color) = match charges.next_progress() {
                    _ if charges.sources() == 0 => (0.0, palette::ABILITY_RELOAD),
                    Some(progress) if charges.ready() == 0 => (progress, palette::ABILITY_RELOAD),
                    _ => (1.0, palette::ABILITY_FULL),
                };
                if let Some(mut node) = node {
                    node.width = Val::Percent(width * 100.0);
                }
                if let Some(mut background) = background {
                    background.set_if_neq(BackgroundColor(color));
                }
            }
            SlotPart::Pips(ability) => {
                // A pip per charge: respawn them when the number of buildings changes.
                let charges = arsenal.charges(ability);
                let pips_now = children.map_or(0, |children| children.len());
                let wanted = match charges.sources() as usize {
                    count if count > MAX_PIPS => 0,
                    count => count,
                };
                if pips_now != wanted {
                    commands.entity(entity).despawn_related::<Children>();
                    commands.entity(entity).with_children(|row| {
                        for _ in 0..wanted {
                            row.spawn((
                                Pip,
                                Node {
                                    width: Val::Px(PIP_SIZE),
                                    height: Val::Px(PIP_SIZE),
                                    ..default()
                                },
                                BackgroundColor(palette::ABILITY_PIP_EMPTY),
                                Pickable::IGNORE,
                            ));
                        }
                    });
                    continue;
                }
                let Some(children) = children else { continue };
                for (i, child) in children.iter().enumerate() {
                    if let Ok(mut pip) = pips.get_mut(child) {
                        let lit = (i as u32) < charges.ready();
                        pip.set_if_neq(BackgroundColor(if lit {
                            palette::ABILITY_PIP
                        } else {
                            palette::ABILITY_PIP_EMPTY
                        }));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_say_whats_left() {
        let mut arsenal = Arsenal::default();
        assert_eq!(
            status(Ability::Barrage, &arsenal),
            "needs Artillery Command"
        );
        assert_eq!(status(Ability::Sniper, &arsenal), "ready");
        arsenal.charges_mut(Ability::Sniper).use_one();
        arsenal.charges_mut(Ability::Sniper).tick(3.5);
        assert_eq!(status(Ability::Sniper, &arsenal), "12s");
        let barrage = arsenal.charges_mut(Ability::Barrage);
        barrage.set_sources(3);
        barrage.use_one();
        assert_eq!(status(Ability::Barrage, &arsenal), "2 ready, next 30s");
    }
}
