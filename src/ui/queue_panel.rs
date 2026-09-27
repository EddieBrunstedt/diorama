//! The order queue as a strip of cards along the bottom: drag a card to reorder, click it to remove.
//! Each card shows how long its order takes, and a line between the cards marks where the wave
//! starts: orders after it won't be done in time. The timeline under the cards is in
//! [`super::timeline`].
//!
//! Cards are positioned by hand (not by flexbox) so they can slide smoothly to their slots.

use super::icons::Icons;
use super::timeline::{TRACK_ROW_HEIGHT, spawn_track};
use super::{
    FlashOrder, OrderFinished, duration_text, font, format_seconds, order_color, order_title,
    paper, problem_color, problem_text, wood_label,
};
use crate::catapult::CatapultAim;
use crate::catapult::shot::Loader;
use crate::keeper::{Backpack, Keeper, OrderId, OrderKind, OrderQueue, Plan, Timeline};
use crate::palette;
use crate::tuning::MAX_ORDERS;
use bevy::picking::events::{Click, Drag, DragEnd, DragStart, Out, Over, Pointer, Press};
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use std::collections::HashMap;

const CARD_WIDTH: f32 = 64.0;
const CARD_HEIGHT: f32 = 84.0;
const CARD_GAP: f32 = 8.0;
const PITCH: f32 = CARD_WIDTH + CARD_GAP;
const PANEL_LEFT: f32 = 24.0;
const PANEL_BOTTOM: f32 = 20.0;
const PANEL_PADDING: f32 = 12.0;
const PANEL_BORDER: f32 = 2.0;
const HEADER_HEIGHT: f32 = 18.0;
const ROW_GAP: f32 = 8.0;
const PANEL_HEIGHT: f32 = 2.0 * (PANEL_PADDING + PANEL_BORDER)
    + HEADER_HEIGHT
    + ROW_GAP
    + CARD_HEIGHT
    + ROW_GAP
    + TRACK_ROW_HEIGHT;
/// Width of the strip of card slots (and of the timeline under it).
pub(super) const STRIP_WIDTH: f32 = MAX_ORDERS as f32 * PITCH - CARD_GAP;
/// Where the first card's left edge is on screen.
const CARDS_LEFT: f32 = PANEL_LEFT + PANEL_BORDER + PANEL_PADDING;

/// How quickly cards slide to their slots (per second, exponential).
const SLIDE_RATE: f32 = 18.0;
const APPEAR_TIME: f32 = 0.28;
const LEAVE_TIME: f32 = 0.3;
const FLASH_TIME: f32 = 0.5;
/// Pointer travel in pixels after which a press counts as a drag, not a click.
const DRAG_THRESHOLD: f32 = 6.0;
const DIVIDER_WIDTH: f32 = 4.0;

#[derive(Component)]
pub(super) struct CardArea;

#[derive(Component)]
pub(super) struct EmptyHint;

/// The line between the last card that's done in time and the first that isn't.
#[derive(Component)]
pub(super) struct WaveDivider {
    x: f32,
    /// Seconds since it appeared, for growing in. `None` while hidden.
    age: Option<f32>,
}

/// Says in the header how many orders won't be done before the wave.
#[derive(Component)]
pub(super) struct LateNote;

/// The hint at the header's right: how to use the cards, or the catapult while he's in it.
#[derive(Component)]
pub(super) struct HeaderHint;

const CARDS_HINT: &str = "drag to reorder, click to remove";
const CATAPULT_HINT: &str = "hold click to charge, let go to throw, right-click to get out";

#[derive(Component)]
pub(super) struct Tooltip {
    title: Entity,
    time: Entity,
    problem: Entity,
}

#[derive(Clone, Copy, PartialEq)]
enum Leave {
    /// Carried out: the card hops up and away.
    Done,
    /// Removed by the player: the card shrinks away.
    Removed,
}

#[derive(Component)]
pub(super) struct Card {
    id: OrderId,
    kind: OrderKind,
    /// Current left edge within the card area.
    x: f32,
    age: f32,
    leaving: Option<(Leave, f32)>,
    dragging: bool,
    /// Pointer travel since the last press, to tell clicks from drags.
    travel: f32,
    hovered: bool,
    flash: f32,
    /// Won't be done before the wave, as of the last frame.
    late: bool,
    parts: CardParts,
}

impl Card {
    /// The card's order, if the pointer is over the card.
    pub(super) fn hovered_order(&self) -> Option<OrderId> {
        (self.hovered && self.leaving.is_none()).then_some(self.id)
    }
}

#[derive(Clone, Copy)]
struct CardParts {
    number: Entity,
    time: Entity,
    icon: Entity,
    badge: Entity,
    bar: Entity,
    fill: Entity,
}

pub(super) fn spawn_panel(mut commands: Commands, icons: Res<Icons>) {
    let width = STRIP_WIDTH + 2.0 * (PANEL_PADDING + PANEL_BORDER);
    let panel = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(PANEL_LEFT),
                bottom: Val::Px(PANEL_BOTTOM),
                width: Val::Px(width),
                height: Val::Px(PANEL_HEIGHT),
                padding: UiRect::all(Val::Px(PANEL_PADDING)),
                border: UiRect::all(Val::Px(PANEL_BORDER)),
                border_radius: BorderRadius::all(Val::Px(16.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(ROW_GAP),
                ..default()
            },
            paper(16.0),
        ))
        .with_children(|panel| {
            panel
                .spawn((
                    Node {
                        height: Val::Px(HEADER_HEIGHT),
                        justify_content: JustifyContent::SpaceBetween,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    Pickable::IGNORE,
                ))
                .with_children(|header| {
                    header
                        .spawn((
                            Node {
                                column_gap: Val::Px(12.0),
                                align_items: AlignItems::Center,
                                ..default()
                            },
                            Pickable::IGNORE,
                        ))
                        .with_children(|title| {
                            title.spawn((
                                Text::new("ORDERS"),
                                font(14.0),
                                TextColor(palette::UI_INK),
                                Pickable::IGNORE,
                            ));
                            title.spawn((
                                LateNote,
                                Text::default(),
                                font(12.0),
                                TextColor(palette::PROBLEM),
                                Pickable::IGNORE,
                            ));
                        });
                    header.spawn((
                        HeaderHint,
                        Text::new(CARDS_HINT),
                        font(12.0),
                        TextColor(palette::UI_INK_SOFT),
                        Pickable::IGNORE,
                    ));
                });
            panel
                .spawn((
                    CardArea,
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(CARD_HEIGHT),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    Pickable::IGNORE,
                ))
                .with_children(|area| {
                    area.spawn((
                        EmptyHint,
                        Text::new(
                            "Click a tree to chop it, a meadow to build, a hurt tower to repair it.\nAnywhere else to walk. Shift-click to do it next.",
                        ),
                        font(14.0),
                        TextColor(palette::UI_INK_SOFT),
                        TextLayout::justify(Justify::Center),
                        Pickable::IGNORE,
                    ));
                    area.spawn((
                        WaveDivider { x: 0.0, age: None },
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(0.0),
                            top: Val::Px(-4.0),
                            width: Val::Px(DIVIDER_WIDTH),
                            height: Val::Px(CARD_HEIGHT + 8.0 + ROW_GAP),
                            border_radius: BorderRadius::all(Val::Px(DIVIDER_WIDTH / 2.0)),
                            display: Display::None,
                            ..default()
                        },
                        BackgroundColor(palette::PROBLEM),
                        UiTransform::IDENTITY,
                        ZIndex(4),
                        Pickable::IGNORE,
                    ));
                });
        })
        .id();
    spawn_track(&mut commands, panel, &icons);

    let mut tooltip = commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(PANEL_BOTTOM + PANEL_HEIGHT + 8.0),
            padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
            border: UiRect::all(Val::Px(2.0)),
            border_radius: BorderRadius::all(Val::Px(10.0)),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(4.0),
            display: Display::None,
            ..default()
        },
        paper(10.0),
        GlobalZIndex(10),
        Pickable::IGNORE,
    ));
    let mut title_entity = Entity::PLACEHOLDER;
    let mut time_entity = Entity::PLACEHOLDER;
    let mut problem_entity = Entity::PLACEHOLDER;
    tooltip.with_children(|tooltip| {
        title_entity = tooltip
            .spawn((
                Text::default(),
                font(15.0),
                TextColor(palette::UI_INK),
                Pickable::IGNORE,
            ))
            .id();
        time_entity = tooltip
            .spawn((
                Text::default(),
                font(13.0),
                TextColor(palette::UI_INK_SOFT),
                Pickable::IGNORE,
            ))
            .id();
        problem_entity = tooltip
            .spawn((
                Text::default(),
                font(13.0),
                TextColor(palette::PROBLEM),
                Pickable::IGNORE,
            ))
            .id();
    });
    tooltip.insert(Tooltip {
        title: title_entity,
        time: time_entity,
        problem: problem_entity,
    });
}

fn spawn_card(
    commands: &mut Commands,
    area: Entity,
    icons: &Icons,
    id: OrderId,
    kind: OrderKind,
    x: f32,
) -> Entity {
    let icon = icons.order(&kind);
    let card = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(x),
                top: Val::Px(0.0),
                width: Val::Px(CARD_WIDTH),
                height: Val::Px(CARD_HEIGHT),
                border: UiRect::all(Val::Px(3.0)),
                border_radius: BorderRadius::all(Val::Px(10.0)),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: Val::Px(2.0),
                ..default()
            },
            BackgroundColor(palette::UI_PAPER),
            BorderColor::all(order_color(&kind)),
            Outline::new(Val::Px(3.0), Val::Px(2.0), Color::NONE),
            UiTransform::from_scale(Vec2::splat(0.2)),
            ChildOf(area),
        ))
        .id();
    let number = commands
        .spawn((
            Text::default(),
            font(12.0),
            TextColor(palette::UI_INK_SOFT),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(5.0),
                top: Val::Px(2.0),
                ..default()
            },
            Pickable::IGNORE,
            ChildOf(card),
        ))
        .id();
    let time = commands
        .spawn((
            Text::default(),
            font(11.0),
            TextColor(palette::UI_INK_SOFT),
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(5.0),
                top: Val::Px(3.0),
                ..default()
            },
            Pickable::IGNORE,
            ChildOf(card),
        ))
        .id();
    let icon = commands
        .spawn((
            ImageNode::new(icon),
            Node {
                width: Val::Px(36.0),
                height: Val::Px(36.0),
                margin: UiRect::top(Val::Px(8.0)),
                ..default()
            },
            Pickable::IGNORE,
            ChildOf(card),
        ))
        .id();
    commands.spawn((
        Text::new(wood_label(&kind)),
        font(14.0),
        TextColor(palette::UI_INK),
        Node {
            height: Val::Px(18.0),
            ..default()
        },
        Pickable::IGNORE,
        ChildOf(card),
    ));
    let badge = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(-9.0),
                top: Val::Px(-9.0),
                width: Val::Px(20.0),
                height: Val::Px(20.0),
                border_radius: BorderRadius::MAX,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                display: Display::None,
                ..default()
            },
            BackgroundColor(palette::PROBLEM),
            Pickable::IGNORE,
            ChildOf(card),
        ))
        .with_child((
            Text::new("!"),
            font(14.0),
            TextColor(Color::WHITE),
            Pickable::IGNORE,
        ))
        .id();
    let bar = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(7.0),
                right: Val::Px(7.0),
                bottom: Val::Px(5.0),
                height: Val::Px(5.0),
                border_radius: BorderRadius::all(Val::Px(3.0)),
                display: Display::None,
                ..default()
            },
            BackgroundColor(palette::UI_PAPER_DARK),
            Pickable::IGNORE,
            ChildOf(card),
        ))
        .id();
    let fill = commands
        .spawn((
            Node {
                width: Val::Percent(0.0),
                height: Val::Percent(100.0),
                border_radius: BorderRadius::all(Val::Px(3.0)),
                ..default()
            },
            BackgroundColor(order_color(&kind)),
            Pickable::IGNORE,
            ChildOf(bar),
        ))
        .id();

    commands
        .entity(card)
        .insert(Card {
            id,
            kind,
            x,
            age: 0.0,
            leaving: None,
            dragging: false,
            travel: 0.0,
            hovered: false,
            flash: 0.0,
            late: false,
            parts: CardParts {
                number,
                time,
                icon,
                badge,
                bar,
                fill,
            },
        })
        .observe(on_press)
        .observe(on_drag_start)
        .observe(on_drag)
        .observe(on_drag_end)
        .observe(on_click)
        .observe(on_over)
        .observe(on_out);
    card
}

/// Adds cards for new orders and sends off cards whose orders are gone.
pub(super) fn sync_cards(
    mut commands: Commands,
    queue: Res<OrderQueue>,
    icons: Res<Icons>,
    area: Single<Entity, With<CardArea>>,
    mut hint: Single<&mut Node, With<EmptyHint>>,
    mut cards: Query<&mut Card>,
    mut known: Local<HashMap<OrderId, Entity>>,
    mut finished: MessageReader<OrderFinished>,
    mut flashes: MessageReader<FlashOrder>,
) {
    let finished: Vec<OrderId> = finished.read().map(|done| done.0).collect();
    for flash in flashes.read() {
        if let Some(mut card) = known.get(&flash.0).and_then(|&e| cards.get_mut(e).ok()) {
            card.flash = FLASH_TIME;
        }
    }
    known.retain(|id, entity| {
        if queue.index_of(*id).is_some() {
            return true;
        }
        if let Ok(mut card) = cards.get_mut(*entity) {
            let leave = if finished.contains(id) {
                Leave::Done
            } else {
                Leave::Removed
            };
            card.leaving = Some((leave, 0.0));
            card.dragging = false;
        }
        false
    });
    for (index, order) in queue.orders().iter().enumerate() {
        known.entry(order.id).or_insert_with(|| {
            // Slide in from a little to the right of its slot.
            let x = index as f32 * PITCH + 24.0;
            spawn_card(&mut commands, *area, &icons, order.id, order.kind, x)
        });
    }
    let display = if queue.is_empty() {
        Display::Flex
    } else {
        Display::None
    };
    if hint.display != display {
        hint.display = display;
    }
}

type CardVisuals<'a> = (
    Entity,
    &'a mut Card,
    &'a mut Node,
    &'a mut UiTransform,
    &'a mut BackgroundColor,
    &'a mut BorderColor,
    &'a mut Outline,
    &'a mut ZIndex,
);

/// Slides, scales and styles the cards.
pub(super) fn animate_cards(
    mut commands: Commands,
    time: Res<Time>,
    queue: Res<OrderQueue>,
    plan: Res<Plan>,
    timeline: Res<Timeline>,
    keeper: Query<&Keeper>,
    catapult: Res<CatapultAim>,
    mut cards: Query<CardVisuals>,
    mut parts: Query<&mut Node, Without<Card>>,
    mut badges: Query<&mut BackgroundColor, Without<Card>>,
    mut texts: Query<(&mut Text, &mut TextColor)>,
    mut icons: Query<&mut ImageNode>,
) {
    let dt = time.delta_secs();
    let t = time.elapsed_secs();
    // The started card's bar shows the work done, or in the catapult, the charge or the reload.
    let progress = keeper.single().ok().and_then(|keeper| {
        keeper.task.work_progress().or_else(|| {
            catapult.manned.then(|| match catapult.loader {
                Loader::Charging { .. } => catapult.loader.charge(),
                loader => loader.loaded(),
            })
        })
    });
    let started = queue.started().map(|order| order.id);

    for (
        entity,
        mut card,
        mut node,
        mut transform,
        mut background,
        mut border,
        mut outline,
        mut z,
    ) in &mut cards
    {
        card.age += dt;
        card.flash = (card.flash - dt).max(0.0);
        let slot = queue
            .index_of(card.id)
            .and_then(|index| timeline.slots.get(index));
        let late = slot.is_some_and(|slot| slot.late);
        if late && !card.late && card.leaving.is_none() {
            // Just pushed past the wave: a shake says so.
            card.flash = FLASH_TIME;
        }
        card.late = late;
        let appear = EaseFunction::BackOut.sample_clamped(card.age / APPEAR_TIME);
        let mut scale = 0.2 + 0.8 * appear;
        let mut rise = 0.0;
        let mut rotation = 0.0;

        if let Some((leave, elapsed)) = &mut card.leaving {
            *elapsed += dt;
            let k = (*elapsed / LEAVE_TIME).min(1.0);
            if k >= 1.0 {
                commands.entity(entity).despawn();
                continue;
            }
            match leave {
                Leave::Done => {
                    rise = -40.0 * EaseFunction::QuadraticOut.sample_clamped(k);
                    scale *= 1.0 + 0.2 * k - k * k;
                }
                Leave::Removed => {
                    scale *= 1.0 - EaseFunction::BackIn.sample_clamped(k).clamp(0.0, 1.0);
                    rotation = 0.3 * k;
                }
            }
        } else if let Some(index) = queue.index_of(card.id) {
            let slot = index as f32 * PITCH;
            if !card.dragging {
                card.x = card.x.lerp(slot, 1.0 - (-SLIDE_RATE * dt).exp());
            }
            if card.dragging {
                scale *= 1.1;
                rise = -6.0;
            } else if card.hovered {
                scale *= 1.05;
            }
        }
        if card.flash > 0.0 {
            let k = card.flash / FLASH_TIME;
            rotation += (t * 40.0).sin() * 0.12 * k;
            scale *= 1.0 + 0.12 * k;
        }

        if node.left != Val::Px(card.x) {
            node.left = Val::Px(card.x);
        }
        let wanted = UiTransform {
            translation: Val2::new(Val::Px(0.0), Val::Px(rise)),
            scale: Vec2::splat(scale.max(0.01)),
            rotation: Rot2::radians(rotation),
        };
        transform.set_if_neq(wanted);
        z.set_if_neq(ZIndex(if card.dragging { 5 } else { 0 }));

        // Style from the plan.
        let planned = plan.get(card.id);
        let flagged = planned.and_then(|planned| planned.problem);
        let problem = flagged.is_some();
        let held = flagged.is_some_and(|problem| problem.holds());
        let is_started = started == Some(card.id);
        background.set_if_neq(BackgroundColor(match flagged {
            Some(problem) if problem.holds() => palette::UI_PAPER.mix(&palette::SLIME, 0.18),
            Some(_) => palette::UI_PROBLEM_PAPER,
            None => palette::UI_PAPER,
        }));
        border.set_if_neq(BorderColor::all(match flagged {
            Some(problem) => problem_color(problem),
            None if late => palette::PROBLEM,
            None => order_color(&card.kind),
        }));
        if let (Some(problem), Ok(mut badge)) = (flagged, badges.get_mut(card.parts.badge)) {
            badge.set_if_neq(BackgroundColor(problem_color(problem)));
        }
        // Late and waiting orders fade a little: they're still queued, just not happening now.
        if let Ok(mut icon) = icons.get_mut(card.parts.icon) {
            let alpha = if late || held { 0.55 } else { 1.0 };
            if icon.color.alpha() != alpha {
                icon.color = Color::WHITE.with_alpha(alpha);
            }
        }
        // Open-ended (the catapult) and after it: no time to show.
        let seconds = slot.map_or(Some(0.0), |slot| slot.seconds());
        let time = match seconds {
            Some(seconds) if !problem && seconds >= 0.05 => format_seconds(seconds),
            _ => String::new(),
        };
        if let Ok((mut text, mut color)) = texts.get_mut(card.parts.time) {
            if text.0 != time {
                text.0 = time;
            }
            color.set_if_neq(TextColor(if late {
                palette::PROBLEM
            } else {
                palette::UI_INK_SOFT
            }));
        }
        let glow = if is_started {
            palette::FLAG.with_alpha(0.6 + 0.4 * (t * 5.0).sin().abs())
        } else {
            Color::NONE
        };
        if outline.color != glow {
            outline.color = glow;
        }
        if let Some(index) = queue.index_of(card.id)
            && let Ok((mut text, _)) = texts.get_mut(card.parts.number)
        {
            let number = (index + 1).to_string();
            if text.0 != number {
                text.0 = number;
            }
        }
        set_display(&mut parts, card.parts.badge, problem);
        let work = progress.filter(|_| is_started);
        set_display(&mut parts, card.parts.bar, work.is_some());
        if let (Some(work), Ok(mut fill)) = (work, parts.get_mut(card.parts.fill)) {
            fill.width = Val::Percent(work * 100.0);
        }
    }
}

fn set_display(nodes: &mut Query<&mut Node, Without<Card>>, entity: Entity, shown: bool) {
    let display = if shown { Display::Flex } else { Display::None };
    if let Ok(mut node) = nodes.get_mut(entity)
        && node.display != display
    {
        node.display = display;
    }
}

/// Explains the hovered card.
pub(super) fn update_tooltip(
    queue: Res<OrderQueue>,
    plan: Res<Plan>,
    timeline: Res<Timeline>,
    backpack: Res<Backpack>,
    cards: Query<&Card>,
    tooltip: Single<(&Tooltip, &mut Node)>,
    mut texts: Query<(&mut Text, &mut TextColor)>,
) {
    let (tooltip, mut node) = tooltip.into_inner();
    let hovered = cards
        .iter()
        .find(|card| card.hovered && !card.dragging && card.leaving.is_none());
    let Some((card, index)) = hovered.and_then(|card| Some((card, queue.index_of(card.id)?)))
    else {
        if node.display != Display::None {
            node.display = Display::None;
        }
        return;
    };
    node.display = Display::Flex;
    node.left = Val::Px(CARDS_LEFT + card.x);

    let mut title = order_title(&card.kind).to_string();
    if queue.started().is_some_and(|order| order.id == card.id) {
        title.push_str(" (doing now)");
    }
    let wood_before = index
        .checked_sub(1)
        .and_then(|previous| plan.orders.get(previous))
        .map_or(backpack.wood, |planned| planned.wood_after);
    let planned = plan.get(card.id);
    let slot = timeline.slots.get(index);
    let started = queue.started().is_some_and(|order| order.id == card.id);
    let time = match (planned, slot.and_then(|slot| slot.seconds())) {
        (Some(planned), _) if planned.problem.is_some() => String::new(),
        (Some(planned), _) if started && planned.order.kind.open_ended() => {
            "In until you call him out (right-click)".to_string()
        }
        (_, Some(seconds)) if started => format!("{} left", format_seconds(seconds)),
        (Some(planned), _) => duration_text(planned),
        _ => String::new(),
    };
    let problem = planned
        .and_then(|planned| problem_text(planned, wood_before))
        .or_else(|| {
            slot.filter(|slot| slot.late)
                .map(|_| "Won't be done before the wave".to_string())
        })
        .unwrap_or_default();
    let problem_color = planned
        .and_then(|planned| planned.problem)
        .map_or(palette::PROBLEM, problem_color);
    for (entity, text) in [
        (tooltip.title, title),
        (tooltip.time, time),
        (tooltip.problem, problem),
    ] {
        if let Ok((mut current, _)) = texts.get_mut(entity)
            && current.0 != text
        {
            current.0 = text;
        }
    }
    if let Ok((_, mut color)) = texts.get_mut(tooltip.problem) {
        color.set_if_neq(TextColor(problem_color));
    }
}

/// Slides the wave line to between the last card done in time and the first one that isn't.
pub(super) fn update_wave_divider(
    time: Res<Time>,
    timeline: Res<Timeline>,
    divider: Single<(&mut WaveDivider, &mut Node, &mut UiTransform)>,
) {
    let (mut divider, mut node, mut transform) = divider.into_inner();
    let first_late = timeline.first_late();
    let display = if first_late.is_some() {
        Display::Flex
    } else {
        Display::None
    };
    if node.display != display {
        node.display = display;
    }
    let Some(index) = first_late else {
        divider.age = None;
        return;
    };
    let x = index as f32 * PITCH - (CARD_GAP + DIVIDER_WIDTH) / 2.0;
    let age = match divider.age {
        Some(age) => age + time.delta_secs(),
        None => {
            // Appear in place, growing from the middle.
            divider.x = x;
            0.0
        }
    };
    divider.age = Some(age);
    divider.x = divider
        .x
        .lerp(x, 1.0 - (-SLIDE_RATE * time.delta_secs()).exp());
    node.left = Val::Px(divider.x);
    let grow = EaseFunction::BackOut.sample_clamped(age / APPEAR_TIME);
    transform.set_if_neq(UiTransform::from_scale(Vec2::new(1.0, grow.max(0.01))));
}

pub(super) fn update_late_note(
    timeline: Res<Timeline>,
    mut note: Single<&mut Text, With<LateNote>>,
) {
    let text = match timeline.late_count() {
        0 => String::new(),
        1 => "1 order won't be done before the wave".to_string(),
        count => format!("{count} orders won't be done before the wave"),
    };
    if note.0 != text {
        note.0 = text;
    }
}

pub(super) fn update_header_hint(
    catapult: Res<CatapultAim>,
    hint: Single<(&mut Text, &mut TextColor), With<HeaderHint>>,
) {
    let (mut text, mut color) = hint.into_inner();
    let (wanted, wanted_color) = if catapult.manned {
        (CATAPULT_HINT, palette::ORDER_CATAPULT)
    } else {
        (CARDS_HINT, palette::UI_INK_SOFT)
    };
    if text.0 != wanted {
        text.0 = wanted.to_string();
    }
    color.set_if_neq(TextColor(wanted_color));
}

fn on_press(press: On<Pointer<Press>>, mut cards: Query<&mut Card>) {
    if let Ok(mut card) = cards.get_mut(press.entity) {
        card.travel = 0.0;
    }
}

fn on_drag_start(
    drag: On<Pointer<DragStart>>,
    mut cards: Query<&mut Card>,
    queue: Res<OrderQueue>,
) {
    if drag.event.button != PointerButton::Primary {
        return;
    }
    let started = queue.started().map(|order| order.id);
    if let Ok(mut card) = cards.get_mut(drag.entity)
        && card.leaving.is_none()
        && started != Some(card.id)
    {
        card.dragging = true;
    }
}

fn on_drag(drag: On<Pointer<Drag>>, mut cards: Query<&mut Card>, mut queue: ResMut<OrderQueue>) {
    let Ok(mut card) = cards.get_mut(drag.entity) else {
        return;
    };
    card.travel += drag.event.delta.length();
    if !card.dragging {
        return;
    }
    let first = queue.first_free_index() as f32 * PITCH;
    let last = queue.orders().len().saturating_sub(1) as f32 * PITCH;
    card.x = (card.x + drag.event.delta.x).clamp(first - PITCH * 0.3, last + PITCH * 0.3);
    let index = (card.x / PITCH).round().max(0.0) as usize;
    if queue.index_of(card.id) != Some(index) {
        queue.move_to(card.id, index);
    }
}

fn on_drag_end(drag: On<Pointer<DragEnd>>, mut cards: Query<&mut Card>) {
    if let Ok(mut card) = cards.get_mut(drag.entity) {
        card.dragging = false;
    }
}

fn on_click(click: On<Pointer<Click>>, cards: Query<&Card>, mut queue: ResMut<OrderQueue>) {
    if click.event.button != PointerButton::Primary {
        return;
    }
    if let Ok(card) = cards.get(click.entity)
        && card.travel < DRAG_THRESHOLD
        && card.leaving.is_none()
    {
        queue.remove(card.id);
    }
}

fn on_over(over: On<Pointer<Over>>, mut cards: Query<&mut Card>) {
    if let Ok(mut card) = cards.get_mut(over.entity) {
        card.hovered = true;
    }
}

fn on_out(out: On<Pointer<Out>>, mut cards: Query<&mut Card>) {
    if let Ok(mut card) = cards.get_mut(out.entity) {
        card.hovered = false;
    }
}
