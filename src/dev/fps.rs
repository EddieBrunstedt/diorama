//! Frame time readout in the top-right corner. Temporary, for the development phase.
//!
//! A small table shows the average, best and worst frame, in both FPS and milliseconds. Below
//! it, a graph of the last few seconds: each bar is the slowest frame in its slice of time, so a
//! single hitch shows up instead of being averaged away. Taller is worse.
//!
//! Frames are counted in real time, so `DIORAMA_SPEED` doesn't change the numbers.

use bevy::prelude::*;
use std::collections::VecDeque;

/// How often the text refreshes, slow enough to read.
const TEXT_INTERVAL: f32 = 0.5;
/// Time covered by each graph bar.
const BAR_INTERVAL: f32 = 0.1;
/// 60 bars of 0.1 s show the last 6 seconds. Best and worst cover the same stretch.
const BARS: usize = 60;
const BAR_WIDTH: f32 = 2.0;
const GRAPH_HEIGHT: f32 = 40.0;
/// Frame time at the top of the graph (30 fps); slower frames are cut off.
const GRAPH_MAX_MS: f32 = 1000.0 / 30.0;
/// Bars up to this are green (120 fps or better).
const GOOD_MS: f32 = 1000.0 / 120.0;
/// Bars up to this are orange (60 fps or better), slower ones red.
const OK_MS: f32 = 1000.0 / 60.0;

pub struct FpsPlugin;

impl Plugin for FpsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FrameStats>()
            .add_systems(Startup, spawn_readout)
            .add_systems(Update, update_readout);
    }
}

/// Frames over a stretch of time. Times are in seconds.
#[derive(Clone, Copy)]
struct Frames {
    count: u32,
    total: f32,
    best: f32,
    worst: f32,
}

impl Default for Frames {
    fn default() -> Self {
        Self {
            count: 0,
            total: 0.0,
            best: f32::INFINITY,
            worst: 0.0,
        }
    }
}

impl Frames {
    fn add(&mut self, frame: f32) {
        self.count += 1;
        self.total += frame;
        self.best = self.best.min(frame);
        self.worst = self.worst.max(frame);
    }

    fn merge(&mut self, other: &Frames) {
        self.count += other.count;
        self.total += other.total;
        self.best = self.best.min(other.best);
        self.worst = self.worst.max(other.worst);
    }

    fn average(&self) -> f32 {
        self.total / self.count as f32
    }
}

#[derive(Resource, Default)]
struct FrameStats {
    /// Since the text last refreshed.
    text: Frames,
    /// Since the last bar.
    bar: Frames,
    /// One entry per bar, oldest first.
    history: VecDeque<Frames>,
}

#[derive(Component)]
struct ReadoutText;

/// A graph bar; 0 is the oldest.
#[derive(Component)]
struct Bar(usize);

fn spawn_readout(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(4.0),
                right: Val::Px(6.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::FlexEnd,
                ..default()
            },
            GlobalZIndex(i32::MAX),
            Pickable::IGNORE,
        ))
        .with_children(|readout| {
            readout.spawn((
                ReadoutText,
                Text::default(),
                TextFont::from_font_size(14.0),
                Pickable::IGNORE,
            ));
            readout
                .spawn((
                    Node {
                        width: Val::Px(BARS as f32 * BAR_WIDTH),
                        height: Val::Px(GRAPH_HEIGHT),
                        align_items: AlignItems::FlexEnd,
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.3)),
                    Pickable::IGNORE,
                ))
                .with_children(|graph| {
                    for index in 0..BARS {
                        graph.spawn((
                            Bar(index),
                            Node {
                                width: Val::Px(BAR_WIDTH),
                                height: Val::Px(0.0),
                                ..default()
                            },
                            BackgroundColor(Color::NONE),
                            Pickable::IGNORE,
                        ));
                    }
                });
        });
}

fn update_readout(
    time: Res<Time<Real>>,
    mut stats: ResMut<FrameStats>,
    mut text: Single<&mut Text, With<ReadoutText>>,
    mut bars: Query<(&Bar, &mut Node, &mut BackgroundColor)>,
) {
    let frame = time.delta_secs();
    // The very first update has no previous frame to measure from.
    if frame <= 0.0 {
        return;
    }
    let stats = &mut *stats;
    stats.text.add(frame);
    stats.bar.add(frame);

    if stats.bar.total >= BAR_INTERVAL {
        stats.history.push_back(stats.bar);
        if stats.history.len() > BARS {
            stats.history.pop_front();
        }
        stats.bar = Frames::default();

        // New bars come in on the right.
        let offset = BARS - stats.history.len();
        for (bar, mut node, mut color) in &mut bars {
            let Some(frames) = bar.0.checked_sub(offset).map(|index| stats.history[index]) else {
                continue;
            };
            let ms = frames.worst * 1000.0;
            node.height = Val::Px(GRAPH_HEIGHT * (ms / GRAPH_MAX_MS).min(1.0));
            color.0 = if ms <= GOOD_MS {
                Color::srgb(0.3, 0.85, 0.3)
            } else if ms <= OK_MS {
                Color::srgb(1.0, 0.6, 0.1)
            } else {
                Color::srgb(0.95, 0.2, 0.2)
            };
        }
    }

    if stats.text.total >= TEXT_INTERVAL {
        let mut recent = stats.bar;
        for frames in &stats.history {
            recent.merge(frames);
        }
        let row =
            |label: &str, secs: f32| format!("{label:<6}{:>5.0}{:>8.2}", 1.0 / secs, secs * 1000.0);
        text.0 = [
            format!("{:<6}{:>5}{:>8}", "", "fps", "ms"),
            row("avg", stats.text.average()),
            row("best", recent.best),
            row("worst", recent.worst),
        ]
        .join("\n");
        stats.text = Frames::default();
    }
}
