//! The tile offer: a hand of cards on the right after each survived wave. Click a card (or press
//! its number) to pick it, then place the tile on the island's edge. The picked card pops and
//! glows; once the tile is placed it hops away and the others slide off.

use super::icons::Icons;
use super::{font, paper, tile_color, tile_name};
use crate::camera::CoveredRight;
use crate::island::TileKind;
use crate::offer::{PickCard, TileOffer};
use crate::palette;
use crate::phase::GameState;
use crate::tuning::TREES_PER_FOREST_TILE;
use bevy::picking::events::{Click, Out, Over, Pointer, Press, Release};
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;

const RIGHT: f32 = 24.0;
const CARD_WIDTH: f32 = 220.0;
const CARD_HEIGHT: f32 = 72.0;
const PANEL_SLIDE: f32 = 280.0;
const PANEL_IN_TIME: f32 = 0.4;
/// Seconds after placing before the panel slides away.
const PANEL_OUT_DELAY: f32 = 0.2;
const PANEL_OUT_TIME: f32 = 0.35;
const DEAL_DELAY: f32 = 0.15;
/// Extra delay per card, so they're dealt one after another.
const DEAL_STAGGER: f32 = 0.09;
const DEAL_TIME: f32 = 0.35;
const PUNCH_TIME: f32 = 0.35;
const LEAVE_TIME: f32 = 0.35;
/// How quickly cards move to where they want to be (per second, exponential).
const EASE_RATE: f32 = 16.0;

#[derive(Component)]
pub(super) struct OfferPanel {
    /// The offer whose cards are dealt.
    round: u32,
    /// Seconds since the cards were dealt.
    age: f32,
    cards: Entity,
    hint: Entity,
}

#[derive(Component)]
pub(super) struct OfferCard {
    index: usize,
    kind: TileKind,
    hovered: bool,
    pressed: bool,
    picked: bool,
    punch: f32,
    /// Sideways offset (pixels), eased toward where the card wants to be.
    shift: f32,
    icon: Entity,
}

pub(super) fn spawn_offer_panel(mut commands: Commands) {
    // A full-height column on the right that centers the panel; it doesn't block the island.
    let column = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(RIGHT),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                display: Display::None,
                ..default()
            },
            Pickable::IGNORE,
        ))
        .id();
    let panel = commands
        .spawn((
            Node {
                padding: UiRect::all(Val::Px(12.0)),
                border: UiRect::all(Val::Px(2.0)),
                border_radius: BorderRadius::all(Val::Px(16.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(10.0),
                ..default()
            },
            paper(16.0),
            ChildOf(column),
        ))
        .id();
    let header = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(2.0),
                ..default()
            },
            Pickable::IGNORE,
            ChildOf(panel),
        ))
        .id();
    commands.spawn((
        Text::new("NEW TILE"),
        font(14.0),
        TextColor(palette::UI_INK),
        Pickable::IGNORE,
        ChildOf(header),
    ));
    let hint = commands
        .spawn((
            Text::default(),
            font(12.0),
            TextColor(palette::UI_INK_SOFT),
            Node {
                max_width: Val::Px(CARD_WIDTH),
                ..default()
            },
            Pickable::IGNORE,
            ChildOf(header),
        ))
        .id();
    let cards = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(10.0),
                ..default()
            },
            Pickable::IGNORE,
            ChildOf(panel),
        ))
        .id();
    commands.entity(column).insert(OfferPanel {
        round: 0,
        age: 0.0,
        cards,
        hint,
    });
}

/// What a tile is good for, in a few words.
fn tile_blurb(kind: TileKind) -> String {
    match kind {
        TileKind::Forest => format!("{TREES_PER_FOREST_TILE} trees to chop"),
        TileKind::Meadow => "Room for a tower".to_string(),
        TileKind::Road => "Longer slime path".to_string(),
        TileKind::Hearth => String::new(),
    }
}

fn spawn_card(
    commands: &mut Commands,
    parent: Entity,
    icons: &Icons,
    index: usize,
    kind: TileKind,
) {
    let card = commands
        .spawn((
            Node {
                width: Val::Px(CARD_WIDTH),
                height: Val::Px(CARD_HEIGHT),
                padding: UiRect::axes(Val::Px(10.0), Val::Px(8.0)),
                border: UiRect::all(Val::Px(3.0)),
                border_radius: BorderRadius::all(Val::Px(10.0)),
                column_gap: Val::Px(12.0),
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(palette::UI_PAPER),
            BorderColor::all(tile_color(kind)),
            Outline::new(Val::Px(3.0), Val::Px(2.0), Color::NONE),
            UiTransform::from_scale(Vec2::splat(0.01)),
            ChildOf(parent),
        ))
        .id();
    let icon = commands
        .spawn((
            ImageNode::new(icons.tile(kind)),
            Node {
                width: Val::Px(48.0),
                height: Val::Px(48.0),
                ..default()
            },
            UiTransform::IDENTITY,
            Pickable::IGNORE,
            ChildOf(card),
        ))
        .id();
    let text = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(2.0),
                ..default()
            },
            Pickable::IGNORE,
            ChildOf(card),
        ))
        .id();
    commands.spawn((
        Text::new(tile_name(kind)),
        font(19.0),
        TextColor(palette::UI_INK),
        Pickable::IGNORE,
        ChildOf(text),
    ));
    commands.spawn((
        Text::new(tile_blurb(kind)),
        font(12.0),
        TextColor(palette::UI_INK_SOFT),
        Pickable::IGNORE,
        ChildOf(text),
    ));
    // Its number key, in the corner.
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(7.0),
                top: Val::Px(6.0),
                padding: UiRect::axes(Val::Px(5.0), Val::Px(0.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BorderColor::all(palette::UI_INK_SOFT),
            Pickable::IGNORE,
            ChildOf(card),
        ))
        .with_child((
            Text::new((index + 1).to_string()),
            font(11.0),
            TextColor(palette::UI_INK_SOFT),
            Pickable::IGNORE,
        ));
    commands
        .entity(card)
        .insert(OfferCard {
            index,
            kind,
            hovered: false,
            pressed: false,
            picked: false,
            punch: 0.0,
            shift: 0.0,
            icon,
        })
        .observe(on_over)
        .observe(on_out)
        .observe(on_press)
        .observe(on_release)
        .observe(on_click);
}

/// Deals the cards for each new offer, slides the panel in and out, and keeps the hint current.
pub(super) fn update_offer_panel(
    mut commands: Commands,
    time: Res<Time>,
    state: Res<State<GameState>>,
    offer: Res<TileOffer>,
    icons: Res<Icons>,
    panel: Single<(&mut OfferPanel, &mut Node, &mut UiTransform, &ComputedNode)>,
    mut texts: Query<&mut Text>,
    mut covered: ResMut<CoveredRight>,
) {
    let (mut panel, mut node, mut transform, computed) = panel.into_inner();
    let shown = *state.get() == GameState::TileOffer;
    let display = if shown { Display::Flex } else { Display::None };
    if node.display != display {
        node.display = display;
    }
    // Asks the camera to keep the island clear of the panel.
    let width = computed.size().x * computed.inverse_scale_factor();
    let wanted = if shown { width + RIGHT } else { 0.0 };
    if covered.0 != wanted {
        covered.0 = wanted;
    }
    if !shown {
        return;
    }
    if panel.round != offer.round {
        panel.round = offer.round;
        panel.age = 0.0;
        commands.entity(panel.cards).despawn_children();
        for (index, &kind) in offer.cards.iter().enumerate() {
            spawn_card(&mut commands, panel.cards, &icons, index, kind);
        }
    }
    panel.age += time.delta_secs();

    let slide_in = 1.0 - EaseFunction::BackOut.sample_clamped(panel.age / PANEL_IN_TIME);
    let slide_out = offer.placed.map_or(0.0, |since| {
        EaseFunction::QuadraticIn.sample_clamped((since - PANEL_OUT_DELAY) / PANEL_OUT_TIME)
    });
    transform.set_if_neq(UiTransform::from_translation(Val2::new(
        Val::Px(PANEL_SLIDE * (slide_in + slide_out)),
        Val::Px(0.0),
    )));

    let hint = if offer.placed.is_some() {
        String::new()
    } else if offer.picked.is_some() {
        "Click a free spot on the island's edge. Right-click to put it back.".to_string()
    } else {
        "Pick one to add to the island.".to_string()
    };
    if let Ok(mut text) = texts.get_mut(panel.hint)
        && text.0 != hint
    {
        text.0 = hint;
    }
}

type CardParts<'a> = (
    &'a mut OfferCard,
    &'a mut UiTransform,
    &'a mut BorderColor,
    &'a mut Outline,
    &'a mut BackgroundColor,
);

/// Deals, lifts, pops and sends off the cards.
pub(super) fn animate_offer_cards(
    time: Res<Time>,
    offer: Res<TileOffer>,
    panel: Single<&OfferPanel>,
    mut cards: Query<CardParts>,
    mut icons: Query<&mut UiTransform, Without<OfferCard>>,
) {
    let dt = time.delta_secs();
    let t = time.elapsed_secs();
    let any_picked = offer.picked.is_some();
    for (mut card, mut transform, mut border, mut outline, mut background) in &mut cards {
        let picked = offer.picked == Some(card.index);
        if picked && !card.picked {
            card.punch = PUNCH_TIME;
        }
        card.picked = picked;
        card.punch = (card.punch - dt).max(0.0);

        let dealt = panel.age - DEAL_DELAY - card.index as f32 * DEAL_STAGGER;
        let deal = EaseFunction::BackOut.sample_clamped(dealt / DEAL_TIME);
        let mut scale = 0.2 + 0.8 * deal;
        let mut rotation = 0.08 * (1.0 - deal.min(1.0));
        let mut rise = 0.0;
        let wanted_shift = if picked {
            -18.0
        } else if card.hovered {
            -6.0
        } else {
            0.0
        };
        card.shift = card.shift.lerp(wanted_shift, 1.0 - (-EASE_RATE * dt).exp());
        if picked {
            scale *= 1.06;
        } else if any_picked {
            scale *= 0.95;
        } else if card.hovered {
            scale *= 1.04;
        }
        if card.pressed {
            scale *= 0.96;
        }
        // The pop when picked: a swell that wobbles back down.
        let k = card.punch / PUNCH_TIME;
        scale *= 1.0 + 0.22 * k * (k * 9.0).cos().abs();
        rotation += 0.05 * k * (t * 30.0).sin();

        if let Some(since) = offer.placed {
            let k = (since / LEAVE_TIME).min(1.0);
            if picked {
                // Hops up and away, like a finished order.
                rise = -40.0 * EaseFunction::QuadraticOut.sample_clamped(k);
                scale *= (1.0 + 0.25 * k - 1.2 * k * k).max(0.0);
            } else {
                scale *= 1.0 - EaseFunction::BackIn.sample_clamped(k).clamp(0.0, 1.0);
                rotation += 0.2 * k;
            }
        }
        let slide = 40.0 * (1.0 - deal.min(1.0));
        transform.set_if_neq(UiTransform {
            translation: Val2::new(Val::Px(card.shift + slide), Val::Px(rise)),
            scale: Vec2::splat(scale.max(0.01)),
            rotation: Rot2::radians(rotation),
        });

        let color = tile_color(card.kind);
        border.set_if_neq(BorderColor::all(if picked { palette::FLAG } else { color }));
        let glow = if picked {
            palette::FLAG.with_alpha(0.6 + 0.4 * (t * 5.0).sin().abs())
        } else {
            Color::NONE
        };
        if outline.color != glow {
            outline.color = glow;
        }
        background.set_if_neq(BackgroundColor(if picked {
            palette::UI_PAPER.mix(&palette::FLAG, 0.12)
        } else if card.hovered {
            palette::UI_PAPER.mix(&color, 0.08)
        } else {
            palette::UI_PAPER
        }));
        if let Ok(mut icon) = icons.get_mut(card.icon) {
            // The icon wiggles on hover and bobs gently once picked.
            let angle = if card.hovered && !picked {
                0.1 * (t * 14.0).sin()
            } else {
                0.0
            };
            let bob = if picked {
                -3.0 * (t * 4.0).sin().abs()
            } else {
                0.0
            };
            icon.set_if_neq(UiTransform {
                translation: Val2::new(Val::Px(0.0), Val::Px(bob)),
                rotation: Rot2::radians(angle),
                ..default()
            });
        }
    }
}

fn on_over(over: On<Pointer<Over>>, mut cards: Query<&mut OfferCard>) {
    if let Ok(mut card) = cards.get_mut(over.entity) {
        card.hovered = true;
    }
}

fn on_out(out: On<Pointer<Out>>, mut cards: Query<&mut OfferCard>) {
    if let Ok(mut card) = cards.get_mut(out.entity) {
        card.hovered = false;
        card.pressed = false;
    }
}

fn on_press(press: On<Pointer<Press>>, mut cards: Query<&mut OfferCard>) {
    if press.event.button == PointerButton::Primary
        && let Ok(mut card) = cards.get_mut(press.entity)
    {
        card.pressed = true;
    }
}

fn on_release(release: On<Pointer<Release>>, mut cards: Query<&mut OfferCard>) {
    if let Ok(mut card) = cards.get_mut(release.entity) {
        card.pressed = false;
    }
}

fn on_click(
    click: On<Pointer<Click>>,
    cards: Query<&OfferCard>,
    mut picks: MessageWriter<PickCard>,
) {
    if click.event.button == PointerButton::Primary
        && let Ok(card) = cards.get(click.entity)
    {
        picks.write(PickCard(Some(card.index)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_offered_tile_has_a_blurb() {
        for kind in [TileKind::Forest, TileKind::Meadow, TileKind::Road] {
            assert!(!tile_blurb(kind).is_empty());
        }
    }
}
