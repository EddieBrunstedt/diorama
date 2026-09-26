//! Heads-up bits: the wood and hearth counters, popups over the island, numbered markers over
//! order targets, and a label at the cursor saying what a click would do.

use super::icons::Icons;
use super::{
    Popup, font, format_seconds, order_color, order_title, paper, problem_color, wood_label,
};
use crate::camera::DioramaCamera;
use crate::dev::ScriptedCursor;
use crate::hearth::HearthHealth;
use crate::keeper::{
    Aim, AimPreview, Backpack, IncomingLog, OrderId, OrderKind, OrderQueue, Plan, Problem,
    Timeline, cursor_position, shown_wood,
};
use crate::palette;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use std::collections::HashMap;

const POPUP_LIFETIME: f32 = 1.2;
const POPUP_RISE: f32 = 46.0;
const POPUP_WIDTH: f32 = 260.0;
const PUNCH_TIME: f32 = 0.35;
/// The hearth counter shakes for this long when the hearth is hit.
const HURT_TIME: f32 = 0.5;
/// At this many hit points or fewer the hearth counter turns red.
const LOW_HEARTH: u32 = 3;
const MARKER_SIZE: f32 = 24.0;
/// Height above an order's target where its numbered marker floats.
const CHOP_MARKER_HEIGHT: f32 = 0.85;
const BUILD_MARKER_HEIGHT: f32 = 0.8;
const WALK_MARKER_HEIGHT: f32 = 0.3;

#[derive(Component)]
pub(super) struct WoodCounter {
    shown: Option<u32>,
    punch: f32,
    text: Entity,
}

#[derive(Component)]
pub(super) struct HearthCounter {
    shown: Option<u32>,
    hurt: f32,
    text: Entity,
}

#[derive(Component)]
pub(super) struct UiPopup {
    at: Vec3,
    age: f32,
    color: Color,
}

#[derive(Component)]
pub(super) struct OrderMarker {
    id: OrderId,
    height: f32,
    text: Entity,
}

#[derive(Component)]
pub(super) struct CursorLabel {
    title: Entity,
    time: Entity,
    warning: Entity,
}

pub(super) fn spawn_hud(mut commands: Commands, icons: Res<Icons>) {
    let mut counter = commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(24.0),
            top: Val::Px(20.0),
            padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
            border: UiRect::all(Val::Px(2.0)),
            border_radius: BorderRadius::all(Val::Px(14.0)),
            column_gap: Val::Px(10.0),
            align_items: AlignItems::Center,
            ..default()
        },
        paper(14.0),
        UiTransform::IDENTITY,
        Pickable::IGNORE,
    ));
    counter.with_child((
        ImageNode::new(icons.wood.clone()),
        Node {
            width: Val::Px(36.0),
            height: Val::Px(36.0),
            ..default()
        },
        Pickable::IGNORE,
    ));
    let mut text = Entity::PLACEHOLDER;
    counter.with_children(|counter| {
        text = counter
            .spawn((
                Text::default(),
                font(26.0),
                TextColor(palette::UI_INK),
                Pickable::IGNORE,
            ))
            .id();
    });
    counter.insert(WoodCounter {
        shown: None,
        punch: 0.0,
        text,
    });

    let mut hearth = commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(24.0),
            top: Val::Px(86.0),
            padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
            border: UiRect::all(Val::Px(2.0)),
            border_radius: BorderRadius::all(Val::Px(14.0)),
            column_gap: Val::Px(10.0),
            align_items: AlignItems::Center,
            ..default()
        },
        paper(14.0),
        UiTransform::IDENTITY,
        Pickable::IGNORE,
    ));
    hearth.with_child((
        ImageNode::new(icons.hearth.clone()),
        Node {
            width: Val::Px(36.0),
            height: Val::Px(36.0),
            ..default()
        },
        Pickable::IGNORE,
    ));
    let mut text = Entity::PLACEHOLDER;
    hearth.with_children(|hearth| {
        text = hearth
            .spawn((
                Text::default(),
                font(26.0),
                TextColor(palette::UI_INK),
                Pickable::IGNORE,
            ))
            .id();
    });
    hearth.insert(HearthCounter {
        shown: None,
        hurt: 0.0,
        text,
    });

    let mut label = commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            padding: UiRect::axes(Val::Px(10.0), Val::Px(5.0)),
            border: UiRect::all(Val::Px(2.0)),
            border_radius: BorderRadius::all(Val::Px(8.0)),
            flex_direction: FlexDirection::Column,
            display: Display::None,
            ..default()
        },
        paper(8.0),
        GlobalZIndex(20),
        Pickable::IGNORE,
    ));
    let (mut title, mut time, mut warning) = (
        Entity::PLACEHOLDER,
        Entity::PLACEHOLDER,
        Entity::PLACEHOLDER,
    );
    label.with_children(|label| {
        title = label
            .spawn((
                Text::default(),
                font(14.0),
                TextColor(palette::UI_INK),
                Pickable::IGNORE,
            ))
            .id();
        time = label
            .spawn((
                Text::default(),
                font(12.0),
                TextColor(palette::UI_INK_SOFT),
                Pickable::IGNORE,
            ))
            .id();
        warning = label
            .spawn((
                Text::default(),
                font(12.0),
                TextColor(palette::PROBLEM),
                Pickable::IGNORE,
            ))
            .id();
    });
    label.insert(CursorLabel {
        title,
        time,
        warning,
    });
}

pub(super) fn update_wood_counter(
    time: Res<Time>,
    backpack: Res<Backpack>,
    incoming: Query<(), With<IncomingLog>>,
    counter: Single<(&mut WoodCounter, &mut UiTransform)>,
    mut texts: Query<(&mut Text, &mut TextColor)>,
) {
    let (mut counter, mut transform) = counter.into_inner();
    let shown = shown_wood(&backpack, incoming.iter().count());
    if counter.shown != Some(shown) {
        if counter.shown.is_some() {
            counter.punch = PUNCH_TIME;
        }
        counter.shown = Some(shown);
        if let Ok((mut text, mut color)) = texts.get_mut(counter.text) {
            text.0 = format!("{shown}/{}", backpack.capacity);
            color.0 = if shown >= backpack.capacity {
                palette::ORDER_CHOP
            } else {
                palette::UI_INK
            };
        }
    }
    counter.punch = (counter.punch - time.delta_secs()).max(0.0);
    let k = counter.punch / PUNCH_TIME;
    // A quick swell that wobbles back down.
    let scale = 1.0 + 0.35 * k * (k * 9.0).cos().abs();
    transform.set_if_neq(UiTransform::from_scale(Vec2::splat(scale)));
}

/// The hearth's hit points. Shakes and flashes red when a slime gets in.
pub(super) fn update_hearth_counter(
    time: Res<Time>,
    health: Res<HearthHealth>,
    counter: Single<(&mut HearthCounter, &mut UiTransform, &mut BorderColor)>,
    mut texts: Query<(&mut Text, &mut TextColor)>,
) {
    let (mut counter, mut transform, mut border) = counter.into_inner();
    if counter.shown != Some(health.hp) {
        if counter.shown.is_some_and(|shown| health.hp < shown) {
            counter.hurt = HURT_TIME;
        }
        counter.shown = Some(health.hp);
        if let Ok((mut text, mut color)) = texts.get_mut(counter.text) {
            text.0 = format!("{}/{}", health.hp, health.max);
            color.0 = if health.hp <= LOW_HEARTH {
                palette::PROBLEM
            } else {
                palette::UI_INK
            };
        }
    }
    counter.hurt = (counter.hurt - time.delta_secs()).max(0.0);
    let k = counter.hurt / HURT_TIME;
    let t = time.elapsed_secs();
    transform.set_if_neq(UiTransform {
        translation: Val2::new(Val::Px(6.0 * k * (t * 55.0).sin()), Val::Px(0.0)),
        scale: Vec2::splat(1.0 + 0.2 * k),
        rotation: Rot2::radians(0.06 * k * (t * 40.0).sin()),
    });
    border.set_if_neq(BorderColor::all(
        palette::UI_PAPER_DARK.mix(&palette::PROBLEM, k),
    ));
}

pub(super) fn spawn_popups(mut commands: Commands, mut popups: MessageReader<Popup>) {
    for popup in popups.read() {
        commands.spawn((
            UiPopup {
                at: popup.at,
                age: 0.0,
                color: popup.color,
            },
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(POPUP_WIDTH),
                justify_content: JustifyContent::Center,
                ..default()
            },
            UiTransform::from_scale(Vec2::splat(0.01)),
            GlobalZIndex(15),
            Pickable::IGNORE,
            children![(
                Text::new(popup.text.clone()),
                font(20.0),
                TextColor(popup.color),
                TextShadow {
                    offset: Vec2::new(1.5, 2.0),
                    color: palette::UI_INK.with_alpha(0.7),
                },
                TextLayout::justify(Justify::Center),
                Pickable::IGNORE,
            )],
        ));
    }
}

pub(super) fn animate_popups(
    mut commands: Commands,
    time: Res<Time>,
    camera: Single<(&Camera, &GlobalTransform), With<DioramaCamera>>,
    mut popups: Query<(Entity, &mut UiPopup, &mut Node, &mut UiTransform, &Children)>,
    mut texts: Query<(&mut TextColor, &mut TextShadow)>,
) {
    let (camera, camera_transform) = *camera;
    for (entity, mut popup, mut node, mut transform, children) in &mut popups {
        popup.age += time.delta_secs();
        let life = popup.age / POPUP_LIFETIME;
        if life >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let Ok(screen) = camera.world_to_viewport(camera_transform, popup.at) else {
            continue;
        };
        let rise = POPUP_RISE * EaseFunction::QuadraticOut.sample_clamped(life);
        node.left = Val::Px(screen.x - POPUP_WIDTH / 2.0);
        node.top = Val::Px(screen.y - 28.0 - rise);
        let pop = EaseFunction::BackOut.sample_clamped(popup.age / 0.2);
        transform.scale = Vec2::splat(pop.max(0.01));
        let alpha = ((1.0 - life) / 0.35).min(1.0);
        for &child in children {
            if let Ok((mut color, mut shadow)) = texts.get_mut(child) {
                color.0 = popup.color.with_alpha(alpha);
                shadow.color = palette::UI_INK.with_alpha(0.7 * alpha);
            }
        }
    }
}

fn marker_height(kind: &OrderKind) -> f32 {
    match kind {
        OrderKind::Chop(_) => CHOP_MARKER_HEIGHT,
        OrderKind::Build(_) => BUILD_MARKER_HEIGHT,
        OrderKind::Walk(_) => WALK_MARKER_HEIGHT,
    }
}

/// Numbered bubbles floating over each order's target, matching the card numbers.
pub(super) fn update_markers(
    mut commands: Commands,
    time: Res<Time>,
    plan: Res<Plan>,
    queue: Res<OrderQueue>,
    camera: Single<(&Camera, &GlobalTransform), With<DioramaCamera>>,
    timeline: Res<Timeline>,
    mut markers: Query<(
        &OrderMarker,
        &mut Node,
        &mut BackgroundColor,
        &mut BorderColor,
        &mut UiTransform,
    )>,
    mut texts: Query<&mut Text>,
    mut known: Local<HashMap<OrderId, Entity>>,
) {
    let (camera, camera_transform) = *camera;
    let planned: HashMap<OrderId, (usize, &crate::keeper::PlannedOrder)> = plan
        .orders
        .iter()
        .enumerate()
        .filter(|(_, planned)| planned.target.is_some())
        .map(|(index, planned)| (planned.order.id, (index, planned)))
        .collect();
    known.retain(|id, entity| {
        let keep = planned.contains_key(id);
        if !keep {
            commands.entity(*entity).despawn();
        }
        keep
    });
    for (&id, (_, planned)) in &planned {
        if known.contains_key(&id) {
            continue;
        }
        let mut text = Entity::PLACEHOLDER;
        let mut marker = commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(MARKER_SIZE),
                height: Val::Px(MARKER_SIZE),
                border: UiRect::all(Val::Px(2.0)),
                border_radius: BorderRadius::MAX,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(order_color(&planned.order.kind)),
            BorderColor::all(palette::UI_PAPER),
            UiTransform::from_scale(Vec2::splat(0.01)),
            Pickable::IGNORE,
        ));
        marker.with_children(|marker| {
            text = marker
                .spawn((
                    Text::default(),
                    font(13.0),
                    TextColor(Color::WHITE),
                    Pickable::IGNORE,
                ))
                .id();
        });
        marker.insert(OrderMarker {
            id,
            height: marker_height(&planned.order.kind),
            text,
        });
        known.insert(id, marker.id());
    }

    let t = time.elapsed_secs();
    let started = queue.started().map(|order| order.id);
    for (marker, mut node, mut background, mut border, mut transform) in &mut markers {
        let Some(&(index, planned)) = planned.get(&marker.id) else {
            continue;
        };
        let target = planned.target.unwrap_or_default();
        let bob = 0.03 * (t * 2.5 + index as f32).sin();
        let at = Vec3::new(target.x, marker.height + bob, target.y);
        if let Ok(screen) = camera.world_to_viewport(camera_transform, at) {
            node.left = Val::Px(screen.x - MARKER_SIZE / 2.0);
            node.top = Val::Px(screen.y - MARKER_SIZE / 2.0);
        }
        let color = planned
            .problem
            .map_or(order_color(&planned.order.kind), problem_color);
        background.set_if_neq(BackgroundColor(color));
        // A red ring on orders that won't be done before the wave.
        let late = timeline.slots.get(index).is_some_and(|slot| slot.late);
        border.set_if_neq(BorderColor::all(if late {
            palette::PROBLEM
        } else {
            palette::UI_PAPER
        }));
        // Pop in, and pulse on the order being carried out.
        let pulse = if started == Some(marker.id) {
            1.0 + 0.12 * (t * 6.0).sin().abs()
        } else {
            1.0
        };
        let grow = (transform.scale.x + time.delta_secs() * 6.0).min(1.0);
        let scale = if transform.scale.x < 1.0 { grow } else { pulse };
        transform.scale = Vec2::splat(scale);
        let label = if planned.problem.is_some() {
            "!".to_string()
        } else {
            (index + 1).to_string()
        };
        if let Ok(mut text) = texts.get_mut(marker.text)
            && text.0 != label
        {
            text.0 = label;
        }
    }
}

/// A label next to the cursor: what a click would do, and whether it would run into trouble.
pub(super) fn update_cursor_label(
    window: Single<&Window, With<PrimaryWindow>>,
    scripted: Option<Res<ScriptedCursor>>,
    keys: Res<ButtonInput<KeyCode>>,
    aim: Res<Aim>,
    queue: Res<OrderQueue>,
    plan: Res<Plan>,
    timeline: Res<Timeline>,
    preview: Res<AimPreview>,
    backpack: Res<Backpack>,
    label: Single<(&CursorLabel, &mut Node, &ComputedNode)>,
    mut texts: Query<(&mut Text, &mut TextColor)>,
) {
    let (label, mut node, computed) = label.into_inner();
    let cursor = cursor_position(&window, scripted.as_deref());
    let (Some(cursor), Some(target)) = (cursor, aim.target) else {
        if node.display != Display::None {
            node.display = Display::None;
        }
        return;
    };
    node.display = Display::Flex;
    // Beside the cursor, flipping to its left near the window's right edge.
    let width = computed.size().x * computed.inverse_scale_factor();
    let left = if cursor.x + 18.0 + width > window.width() - 8.0 {
        cursor.x - 12.0 - width
    } else {
        cursor.x + 18.0
    };
    node.left = Val::Px(left);
    node.top = Val::Px(cursor.y + 16.0);

    let next = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let preview = preview.get(&timeline);
    let time = preview
        .filter(|(planned, _)| planned.problem.is_none())
        .map(|(_, slot)| format!("Takes {}", format_seconds(slot.seconds())))
        .unwrap_or_default();
    let mut warning_color = palette::PROBLEM;
    let (title, warning) = if let Some(existing) = queue.find_same_target(&target.kind) {
        let number = queue.index_of(existing).map_or(0, |index| index + 1);
        (format!("Already order {number}"), String::new())
    } else {
        let mut title = order_title(&target.kind).to_string();
        let wood = wood_label(&target.kind);
        if !wood.is_empty() {
            title = format!("{title}  {wood} wood");
        }
        if next {
            title = format!("Next: {title}");
        }
        // Wood at the point where the order would go in the queue.
        let insert_at = queue.insert_index(next);
        let wood_then = insert_at
            .checked_sub(1)
            .and_then(|previous| plan.orders.get(previous))
            .map_or(backpack.wood, |planned| planned.wood_after);
        let then = Backpack {
            wood: wood_then,
            capacity: backpack.capacity,
        };
        let warning = if queue.is_full() {
            "Too many orders".to_string()
        } else {
            let problem = preview
                .and_then(|(planned, _)| planned.problem)
                .or_else(|| then.problem(&target.kind));
            if let Some(problem) = problem {
                warning_color = problem_color(problem);
            }
            match problem {
                Some(Problem::NotEnoughWood) => {
                    format!("Not enough wood yet (will have {wood_then})")
                }
                Some(Problem::BackpackFull) => "Backpack will be full".to_string(),
                Some(problem) => problem.message().to_string(),
                None => match preview {
                    Some((_, slot)) if slot.late => "Won't be done before the wave".into(),
                    _ => String::new(),
                },
            }
        };
        (title, warning)
    };
    for (entity, text) in [
        (label.title, title),
        (label.time, time),
        (label.warning, warning),
    ] {
        if let Ok((mut current, _)) = texts.get_mut(entity)
            && current.0 != text
        {
            current.0 = text;
        }
    }
    if let Ok((_, mut color)) = texts.get_mut(label.warning) {
        color.set_if_neq(TextColor(warning_color));
    }
}
