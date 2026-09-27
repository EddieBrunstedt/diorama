//! Hit points over hurt towers: a little paper pill of pips floating above the tower, so it's
//! clear at a glance which towers need the keeper's hammer. It shakes when goo hits, turns red
//! when the tower is nearly or all the way broken, and goes away once it's repaired.

use crate::camera::DioramaCamera;
use crate::palette;
use crate::towers::Tower;
use bevy::prelude::*;
use std::collections::HashMap;

/// Height above the tower's base where the pips float.
const HEIGHT: f32 = 1.4;
const PIP_WIDTH: f32 = 7.0;
const PIP_HEIGHT: f32 = 9.0;
const PIP_GAP: f32 = 2.0;
const PADDING: f32 = 4.0;
const HURT_TIME: f32 = 0.4;
/// At this many hit points or fewer the pips turn red.
const LOW: u32 = 2;

#[derive(Component)]
pub(super) struct TowerBar {
    tower: Entity,
    pips: Vec<Entity>,
    shown_hp: u32,
    hurt: f32,
    age: f32,
}

pub(super) fn update_tower_bars(
    mut commands: Commands,
    time: Res<Time>,
    camera: Single<(&Camera, &GlobalTransform), With<DioramaCamera>>,
    towers: Query<(Entity, &Tower, &GlobalTransform)>,
    mut bars: Query<(
        &mut TowerBar,
        &mut Node,
        &mut UiTransform,
        &mut BorderColor,
        &ComputedNode,
    )>,
    mut pips: Query<&mut BackgroundColor>,
    mut known: Local<HashMap<Entity, Entity>>,
) {
    let (camera, camera_transform) = *camera;
    let dt = time.delta_secs();
    let t = time.elapsed_secs();
    known.retain(|tower, bar| {
        let keep = towers
            .get(*tower)
            .is_ok_and(|(_, tower, _)| tower.health.is_damaged());
        if !keep {
            commands.entity(*bar).despawn();
        }
        keep
    });
    for (entity, tower, _) in &towers {
        if !tower.health.is_damaged() || known.contains_key(&entity) {
            continue;
        }
        let width = tower.health.max as f32 * (PIP_WIDTH + PIP_GAP) - PIP_GAP + 2.0 * PADDING;
        let bar = commands
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(width + 4.0),
                    padding: UiRect::all(Val::Px(PADDING)),
                    border: UiRect::all(Val::Px(2.0)),
                    border_radius: BorderRadius::all(Val::Px(6.0)),
                    column_gap: Val::Px(PIP_GAP),
                    ..default()
                },
                BackgroundColor(palette::UI_PAPER.with_alpha(0.92)),
                BorderColor::all(palette::UI_PAPER_DARK),
                UiTransform::from_scale(Vec2::splat(0.01)),
                Pickable::IGNORE,
            ))
            .id();
        let pips = (0..tower.health.max)
            .map(|_| {
                commands
                    .spawn((
                        Node {
                            width: Val::Px(PIP_WIDTH),
                            height: Val::Px(PIP_HEIGHT),
                            border_radius: BorderRadius::all(Val::Px(2.0)),
                            ..default()
                        },
                        BackgroundColor(palette::UI_PAPER_DARK),
                        Pickable::IGNORE,
                        ChildOf(bar),
                    ))
                    .id()
            })
            .collect();
        commands.entity(bar).insert(TowerBar {
            tower: entity,
            pips,
            shown_hp: tower.health.max,
            hurt: 0.0,
            age: 0.0,
        });
        known.insert(entity, bar);
    }

    for (mut bar, mut node, mut transform, mut border, computed) in &mut bars {
        let Ok((_, tower, global)) = towers.get(bar.tower) else {
            continue;
        };
        let health = tower.health;
        if health.hp < bar.shown_hp {
            bar.hurt = HURT_TIME;
        }
        bar.shown_hp = health.hp;
        bar.age += dt;
        bar.hurt = (bar.hurt - dt).max(0.0);

        let at = global.translation() + Vec3::Y * HEIGHT;
        if let Ok(screen) = camera.world_to_viewport(camera_transform, at) {
            let size = computed.size() * computed.inverse_scale_factor();
            node.left = Val::Px(screen.x - size.x / 2.0);
            node.top = Val::Px(screen.y - size.y / 2.0);
        }
        let broken = health.is_broken();
        let k = bar.hurt / HURT_TIME;
        let pop = EaseFunction::BackOut.sample_clamped(bar.age / 0.25);
        let throb = if broken {
            1.0 + 0.08 * (t * 6.0).sin().abs()
        } else {
            1.0
        };
        *transform = UiTransform {
            translation: Val2::new(Val::Px(4.0 * k * (t * 55.0).sin()), Val::Px(0.0)),
            scale: Vec2::splat((pop * throb * (1.0 + 0.2 * k)).max(0.01)),
            rotation: Rot2::IDENTITY,
        };
        border.set_if_neq(BorderColor::all(if broken {
            palette::PROBLEM
        } else {
            palette::UI_PAPER_DARK.mix(&palette::PROBLEM, k)
        }));
        let fill = if health.hp <= LOW {
            palette::PROBLEM
        } else {
            palette::ORDER_BUILD
        };
        for (index, &pip) in bar.pips.iter().enumerate() {
            if let Ok(mut color) = pips.get_mut(pip) {
                color.set_if_neq(BackgroundColor(if (index as u32) < health.hp {
                    fill
                } else {
                    palette::UI_PAPER_DARK
                }));
            }
        }
    }
}
