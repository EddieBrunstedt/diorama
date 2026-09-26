//! The order queue as a strip of cards along the bottom: drag a card to reorder, click it to remove.
//!
//! Cards are positioned by hand (not by flexbox) so they can slide smoothly to their slots.

use super::icons::Icons;
use super::{
    FlashOrder, OrderFinished, font, order_color, order_title, paper, problem_text, wood_label,
};
use crate::keeper::{Backpack, Keeper, OrderId, OrderKind, OrderQueue, Plan};
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
const PANEL_HEIGHT: f32 =
    2.0 * (PANEL_PADDING + PANEL_BORDER) + HEADER_HEIGHT + ROW_GAP + CARD_HEIGHT;
/// Where the first card's left edge is on screen.
const CARDS_LEFT: f32 = PANEL_LEFT + PANEL_BORDER + PANEL_PADDING;

/// How quickly cards slide to their slots (per second, exponential).
const SLIDE_RATE: f32 = 18.0;
const APPEAR_TIME: f32 = 0.28;
const LEAVE_TIME: f32 = 0.3;
const FLASH_TIME: f32 = 0.5;
/// Pointer travel in pixels after which a press counts as a drag, not a click.
const DRAG_THRESHOLD: f32 = 6.0;

#[derive(Component)]
pub(super) struct CardArea;

#[derive(Component)]
pub(super) struct EmptyHint;

#[derive(Component)]
pub(super) struct Tooltip {
    title: Entity,
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
    parts: CardParts,
}

#[derive(Clone, Copy)]
struct CardParts {
    number: Entity,
    badge: Entity,
    bar: Entity,
    fill: Entity,
}

pub(super) fn spawn_panel(mut commands: Commands) {
    let width = MAX_ORDERS as f32 * PITCH - CARD_GAP + 2.0 * (PANEL_PADDING + PANEL_BORDER);
    commands
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
                    header.spawn((
                        Text::new("ORDERS"),
                        font(14.0),
                        TextColor(palette::UI_INK),
                        Pickable::IGNORE,
                    ));
                    header.spawn((
                        Text::new("drag to reorder, click to remove"),
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
                            "Click a tree to chop it, a meadow to build a tower, anywhere else to walk.\nShift-click to do it next.",
                        ),
                        font(14.0),
                        TextColor(palette::UI_INK_SOFT),
                        TextLayout::justify(Justify::Center),
                        Pickable::IGNORE,
                    ));
                });
        });

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
    let icon = match kind {
        OrderKind::Chop(_) => icons.chop.clone(),
        OrderKind::Build(_) => icons.build.clone(),
        OrderKind::Walk(_) => icons.walk.clone(),
    };
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
    commands.spawn((
        ImageNode::new(icon),
        Node {
            width: Val::Px(36.0),
            height: Val::Px(36.0),
            margin: UiRect::top(Val::Px(6.0)),
            ..default()
        },
        Pickable::IGNORE,
        ChildOf(card),
    ));
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
            parts: CardParts {
                number,
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
    keeper: Query<&Keeper>,
    mut cards: Query<CardVisuals>,
    mut parts: Query<&mut Node, Without<Card>>,
    mut texts: Query<&mut Text>,
) {
    let dt = time.delta_secs();
    let t = time.elapsed_secs();
    let progress = keeper
        .single()
        .ok()
        .and_then(|keeper| keeper.task.work_progress());
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
        let problem = planned.is_some_and(|planned| planned.problem.is_some());
        let is_started = started == Some(card.id);
        background.set_if_neq(BackgroundColor(if problem {
            palette::UI_PROBLEM_PAPER
        } else {
            palette::UI_PAPER
        }));
        border.set_if_neq(BorderColor::all(if problem {
            palette::PROBLEM
        } else {
            order_color(&card.kind)
        }));
        let glow = if is_started {
            palette::FLAG.with_alpha(0.6 + 0.4 * (t * 5.0).sin().abs())
        } else {
            Color::NONE
        };
        if outline.color != glow {
            outline.color = glow;
        }
        if let Some(index) = queue.index_of(card.id)
            && let Ok(mut text) = texts.get_mut(card.parts.number)
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
    backpack: Res<Backpack>,
    cards: Query<&Card>,
    tooltip: Single<(&Tooltip, &mut Node)>,
    mut texts: Query<&mut Text>,
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
    let problem = plan
        .get(card.id)
        .and_then(|planned| problem_text(planned, wood_before))
        .unwrap_or_default();
    for (entity, text) in [(tooltip.title, title), (tooltip.problem, problem)] {
        if let Ok(mut current) = texts.get_mut(entity)
            && current.0 != text
        {
            current.0 = text;
        }
    }
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
