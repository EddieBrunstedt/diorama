//! The timeline under the order cards: the prepare phase as a bar, the time already gone, each
//! order's time laid end to end from now, and the wave at the end. Orders reaching past it are late.
//!
//! Segments are placed at absolute times (time gone + time from now), so they stay put while the
//! keeper works and the "now" line sweeps through them.

use super::icons::Icons;
use super::order_color;
use super::queue_panel::{Card, STRIP_WIDTH};
use crate::keeper::{OrderId, OrderQueue, Plan, Timeline};
use crate::palette;
use crate::phase::{GameState, PrepareClock};
use bevy::prelude::*;
use std::collections::HashMap;

pub(super) const TRACK_ROW_HEIGHT: f32 = 22.0;
const BAR_TOP: f32 = 4.0;
const BAR_HEIGHT: f32 = 14.0;
/// Seconds past the wave the track still shows, so late orders have somewhere to be.
const OVERFLOW_SECONDS: f32 = 6.0;
const SLIDE_RATE: f32 = 16.0;
const SEGMENT_GAP: f32 = 2.0;
/// Segments at least this wide show their order's number.
const NUMBER_MIN_WIDTH: f32 = 15.0;
const MARKER_SIZE: f32 = 22.0;
const HURRY_SECONDS: f32 = 5.0;

#[derive(Component)]
pub(super) struct Track {
    bar: Entity,
    elapsed: Entity,
    overflow: Entity,
    now: Entity,
    wave: Entity,
}

#[derive(Component)]
pub(super) struct Segment {
    x: f32,
    width: f32,
    number: Entity,
}

/// Pixels per second of prepare time.
pub fn px_per_second(total: f32) -> f32 {
    STRIP_WIDTH / (total + OVERFLOW_SECONDS).max(1.0)
}

/// Where a segment goes on the track (left edge and width in pixels), clipped to the track.
/// `None` if it takes no time or lies wholly past the end.
pub fn segment_span(start: f32, seconds: f32, scale: f32) -> Option<(f32, f32)> {
    let left = start * scale;
    let right = ((start + seconds) * scale).min(STRIP_WIDTH);
    let width = right - left - SEGMENT_GAP;
    (width >= 1.0).then_some((left, width))
}

pub(super) fn spawn_track(commands: &mut Commands, panel: Entity, icons: &Icons) {
    let row = commands
        .spawn((
            Node {
                width: Val::Px(STRIP_WIDTH),
                height: Val::Px(TRACK_ROW_HEIGHT),
                ..default()
            },
            Pickable::IGNORE,
            ChildOf(panel),
        ))
        .id();
    let bar = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(BAR_TOP),
                height: Val::Px(BAR_HEIGHT),
                border_radius: BorderRadius::all(Val::Px(BAR_HEIGHT / 2.0)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(palette::UI_PAPER_DARK),
            Pickable::IGNORE,
            ChildOf(row),
        ))
        .id();
    let fill = |commands: &mut Commands, color: Color| {
        commands
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(0.0),
                    bottom: Val::Px(0.0),
                    ..default()
                },
                BackgroundColor(color),
                Pickable::IGNORE,
                ChildOf(bar),
            ))
            .id()
    };
    let elapsed = fill(commands, crate::voxel::shade(palette::UI_PAPER_DARK, 0.86));
    let overflow = fill(commands, palette::PROBLEM.with_alpha(0.25));
    let now = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(BAR_TOP - 3.0),
                width: Val::Px(3.0),
                height: Val::Px(BAR_HEIGHT + 6.0),
                border_radius: BorderRadius::all(Val::Px(1.5)),
                ..default()
            },
            BackgroundColor(palette::UI_INK),
            ZIndex(2),
            Pickable::IGNORE,
            ChildOf(row),
        ))
        .id();
    let wave = commands
        .spawn((
            ImageNode::new(icons.slime.clone()),
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(BAR_TOP + BAR_HEIGHT / 2.0 - MARKER_SIZE / 2.0),
                width: Val::Px(MARKER_SIZE),
                height: Val::Px(MARKER_SIZE),
                ..default()
            },
            UiTransform::IDENTITY,
            ZIndex(3),
            Pickable::IGNORE,
            ChildOf(row),
        ))
        .id();
    commands.entity(row).insert(Track {
        bar,
        elapsed,
        overflow,
        now,
        wave,
    });
}

fn spawn_segment(commands: &mut Commands, bar: Entity, color: Color, x: f32) -> Entity {
    let segment = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(x),
                top: Val::Px(2.0),
                bottom: Val::Px(2.0),
                width: Val::Px(0.0),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(color),
            Outline::new(Val::Px(2.0), Val::Px(0.0), Color::NONE),
            ZIndex(1),
            Pickable::IGNORE,
            ChildOf(bar),
        ))
        .id();
    let number = commands
        .spawn((
            Text::default(),
            super::font(9.0),
            TextColor(Color::WHITE),
            Pickable::IGNORE,
            ChildOf(segment),
        ))
        .id();
    commands.entity(segment).insert(Segment {
        x,
        width: 0.0,
        number,
    });
    segment
}

type SegmentParts<'a> = (
    &'a mut Segment,
    &'a mut Node,
    &'a mut BackgroundColor,
    &'a mut Outline,
);

pub(super) fn update_track(
    mut commands: Commands,
    time: Res<Time>,
    state: Res<State<GameState>>,
    clock: Res<PrepareClock>,
    plan: Res<Plan>,
    timeline: Res<Timeline>,
    queue: Res<OrderQueue>,
    cards: Query<&Card>,
    track: Single<&Track>,
    mut segments: Query<SegmentParts>,
    mut nodes: Query<(&mut Node, &mut BackgroundColor), Without<Segment>>,
    mut markers: Query<&mut UiTransform>,
    mut texts: Query<&mut Text>,
    mut known: Local<HashMap<OrderId, Entity>>,
) {
    let dt = time.delta_secs();
    let t = time.elapsed_secs();
    let scale = px_per_second(clock.total);
    let preparing = timeline.deadline.is_some();
    // During the wave there's no deadline: orders are laid out from the track's start.
    let gone = if preparing { clock.elapsed() } else { 0.0 };
    let now_x = gone * scale;
    let wave_x = clock.total * scale;

    let mut set_span = |entity: Entity, left: f32, width: f32, color: Option<Color>| {
        if let Ok((mut node, mut background)) = nodes.get_mut(entity) {
            node.left = Val::Px(left);
            node.width = Val::Px(width.max(0.0));
            if let Some(color) = color {
                background.set_if_neq(BackgroundColor(color));
            }
        }
    };
    set_span(
        track.elapsed,
        0.0,
        if preparing { now_x } else { 0.0 },
        None,
    );
    set_span(
        track.overflow,
        wave_x,
        if preparing { STRIP_WIDTH - wave_x } else { 0.0 },
        None,
    );
    set_span(track.now, now_x - 1.5, 3.0, None);
    set_span(
        track.bar,
        0.0,
        STRIP_WIDTH,
        Some(if *state.get() == GameState::Wave {
            palette::SLIME.mix(&palette::UI_PAPER_DARK, 0.55)
        } else {
            palette::UI_PAPER_DARK
        }),
    );
    for (entity, shown) in [(track.now, preparing), (track.wave, preparing)] {
        if let Ok((mut node, _)) = nodes.get_mut(entity) {
            let display = if shown { Display::Flex } else { Display::None };
            if node.display != display {
                node.display = display;
            }
        }
    }
    if let Ok((mut node, _)) = nodes.get_mut(track.wave) {
        node.left = Val::Px(wave_x - MARKER_SIZE / 2.0);
    }
    if let Ok(mut marker) = markers.get_mut(track.wave) {
        // The slime at the wave bobs, and bounces eagerly when it's nearly time.
        let hurry = preparing && clock.left <= HURRY_SECONDS;
        let (speed, amount) = if hurry { (14.0, 0.25) } else { (3.0, 0.06) };
        let bounce = (t * speed).sin().abs();
        marker.scale = Vec2::new(1.0 + amount * (1.0 - bounce), 1.0 + amount * bounce);
        marker.translation = Val2::new(Val::Px(0.0), Val::Px(-3.0 * bounce * amount / 0.06));
    }

    // One segment per planned order.
    known.retain(|id, entity| {
        let keep = queue.index_of(*id).is_some();
        if !keep {
            commands.entity(*entity).despawn();
        }
        keep
    });
    let hovered = cards.iter().find_map(Card::hovered_order);
    let started = queue.started().map(|order| order.id);
    for (index, (planned, slot)) in plan.orders.iter().zip(&timeline.slots).enumerate() {
        let id = planned.order.id;
        let color = if slot.late {
            palette::PROBLEM
        } else {
            order_color(&planned.order.kind)
        };
        let span = segment_span(gone + slot.start, slot.seconds(), scale);
        let entity = *known.entry(id).or_insert_with(|| {
            let x = span.map_or(STRIP_WIDTH, |(x, _)| x);
            spawn_segment(&mut commands, track.bar, color, x)
        });
        let Ok((mut segment, mut node, mut background, mut outline)) = segments.get_mut(entity)
        else {
            continue;
        };
        let (x, width) = span.unwrap_or((segment.x + segment.width, 0.0));
        let blend = 1.0 - (-SLIDE_RATE * dt).exp();
        segment.x = segment.x.lerp(x, blend);
        segment.width = segment.width.lerp(width, blend);
        // The order in progress is eaten by the "now" line as time passes; keep it snug.
        if started == Some(id) {
            segment.x = x;
        }
        node.left = Val::Px(segment.x);
        node.width = Val::Px(segment.width.max(0.0));
        let display = if segment.width >= 0.5 {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
        let pulse = if started == Some(id) {
            0.15 * (t * 5.0).sin().abs()
        } else {
            0.0
        };
        background.set_if_neq(BackgroundColor(color.mix(&Color::WHITE, pulse)));
        let highlight = if hovered == Some(id) {
            palette::UI_INK
        } else {
            Color::NONE
        };
        if outline.color != highlight {
            outline.color = highlight;
        }
        let label = if segment.width >= NUMBER_MIN_WIDTH {
            (index + 1).to_string()
        } else {
            String::new()
        };
        if let Ok(mut text) = texts.get_mut(segment.number)
            && text.0 != label
        {
            text.0 = label;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_whole_prepare_phase_and_some_overflow_fit_the_strip() {
        let scale = px_per_second(30.0);
        assert!((scale * (30.0 + OVERFLOW_SECONDS) - STRIP_WIDTH).abs() < 1e-3);
    }

    #[test]
    fn segments_are_clipped_to_the_track() {
        let scale = 10.0;
        assert_eq!(
            segment_span(1.0, 2.0, scale),
            Some((10.0, 20.0 - SEGMENT_GAP))
        );
        let (left, width) = segment_span(0.0, 1e4, scale).unwrap();
        assert_eq!(left + width + SEGMENT_GAP, STRIP_WIDTH);
        assert_eq!(segment_span(1e4, 2.0, scale), None, "wholly past the end");
        assert_eq!(segment_span(3.0, 0.0, scale), None, "takes no time");
    }
}
