//! The build choice under the counters: what a click on a free meadow builds, a tower or the
//! catapult. Click a button or press its key (T or C). The catapult's button fades once there's a
//! catapult on the island (standing, going up or queued), since there's only room for one.

use super::icons::Icons;
use super::{building_name, font, paper};
use crate::keeper::{BuildChoice, Building, ChooseBuilding};
use crate::palette;
use bevy::picking::events::{Click, Out, Over, Pointer};
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;

const LEFT: f32 = 24.0;
const TOP: f32 = 152.0;
const BUTTON_WIDTH: f32 = 60.0;
const BUTTON_HEIGHT: f32 = 70.0;
const PUNCH_TIME: f32 = 0.3;

#[derive(Component)]
pub(super) struct BuildButton {
    building: Building,
    hovered: bool,
    punch: f32,
    icon: Entity,
    note: Entity,
}

pub(super) fn spawn_build_panel(mut commands: Commands, icons: Res<Icons>) {
    let panel = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(LEFT),
                top: Val::Px(TOP),
                padding: UiRect::new(Val::Px(10.0), Val::Px(10.0), Val::Px(6.0), Val::Px(10.0)),
                border: UiRect::all(Val::Px(2.0)),
                border_radius: BorderRadius::all(Val::Px(14.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                ..default()
            },
            paper(14.0),
            Pickable::IGNORE,
        ))
        .id();
    commands.spawn((
        Text::new("BUILD"),
        font(12.0),
        TextColor(palette::UI_INK_SOFT),
        Pickable::IGNORE,
        ChildOf(panel),
    ));
    let row = commands
        .spawn((
            Node {
                column_gap: Val::Px(8.0),
                ..default()
            },
            Pickable::IGNORE,
            ChildOf(panel),
        ))
        .id();
    for (building, key) in [(Building::Tower, "T"), (Building::Catapult, "C")] {
        spawn_button(&mut commands, row, &icons, building, key);
    }
}

fn spawn_button(
    commands: &mut Commands,
    row: Entity,
    icons: &Icons,
    building: Building,
    key: &str,
) {
    let button = commands
        .spawn((
            Node {
                width: Val::Px(BUTTON_WIDTH),
                height: Val::Px(BUTTON_HEIGHT),
                border: UiRect::all(Val::Px(3.0)),
                border_radius: BorderRadius::all(Val::Px(10.0)),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: Val::Px(1.0),
                ..default()
            },
            BackgroundColor(palette::UI_PAPER),
            BorderColor::all(palette::UI_PAPER_DARK),
            UiTransform::IDENTITY,
            ChildOf(row),
        ))
        .id();
    let icon = commands
        .spawn((
            ImageNode::new(icons.building(building)),
            Node {
                width: Val::Px(36.0),
                height: Val::Px(36.0),
                margin: UiRect::top(Val::Px(6.0)),
                ..default()
            },
            Pickable::IGNORE,
            ChildOf(button),
        ))
        .id();
    let note = commands
        .spawn((
            Text::new(format!("-{}", building.wood_cost())),
            font(13.0),
            TextColor(palette::UI_INK),
            Pickable::IGNORE,
            ChildOf(button),
        ))
        .id();
    // Its key, in the corner.
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(4.0),
                top: Val::Px(3.0),
                padding: UiRect::axes(Val::Px(4.0), Val::Px(0.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BorderColor::all(palette::UI_INK_SOFT),
            Pickable::IGNORE,
            ChildOf(button),
        ))
        .with_child((
            Text::new(key),
            font(10.0),
            TextColor(palette::UI_INK_SOFT),
            Pickable::IGNORE,
        ));
    commands
        .entity(button)
        .insert((
            BuildButton {
                building,
                hovered: false,
                punch: 0.0,
                icon,
                note,
            },
            Name::new(building_name(building)),
        ))
        .observe(on_over)
        .observe(on_out)
        .observe(on_click);
}

pub(super) fn update_build_panel(
    time: Res<Time>,
    choice: Res<BuildChoice>,
    mut buttons: Query<(
        &mut BuildButton,
        &mut BorderColor,
        &mut BackgroundColor,
        &mut UiTransform,
    )>,
    mut icons: Query<&mut ImageNode>,
    mut texts: Query<(&mut Text, &mut TextColor)>,
    mut chosen: Local<Option<Building>>,
) {
    let dt = time.delta_secs();
    let changed = *chosen != Some(choice.building);
    let first = chosen.is_none();
    *chosen = Some(choice.building);
    for (mut button, mut border, mut background, mut transform) in &mut buttons {
        let selected = button.building == choice.building;
        if changed && selected && !first {
            button.punch = PUNCH_TIME;
        }
        button.punch = (button.punch - dt).max(0.0);
        let taken = button.building == Building::Catapult && choice.catapult_taken;

        border.set_if_neq(BorderColor::all(if selected {
            palette::ORDER_BUILD
        } else {
            palette::UI_PAPER_DARK
        }));
        background.set_if_neq(BackgroundColor(if selected {
            palette::UI_PAPER
        } else {
            palette::UI_PAPER.mix(&palette::UI_PAPER_DARK, 0.35)
        }));
        let k = button.punch / PUNCH_TIME;
        let hover = if button.hovered { 1.06 } else { 1.0 };
        let scale = hover * (1.0 + 0.18 * k * (k * 9.0).cos().abs());
        transform.set_if_neq(UiTransform::from_scale(Vec2::splat(scale)));

        if let Ok(mut icon) = icons.get_mut(button.icon) {
            let alpha = match (taken, selected) {
                (true, _) => 0.35,
                (false, true) => 1.0,
                (false, false) => 0.75,
            };
            if icon.color.alpha() != alpha {
                icon.color = Color::WHITE.with_alpha(alpha);
            }
        }
        if let Ok((mut text, mut color)) = texts.get_mut(button.note) {
            let (note, note_color) = if taken {
                ("only 1".to_string(), palette::UI_INK_SOFT)
            } else {
                (format!("-{}", button.building.wood_cost()), palette::UI_INK)
            };
            if text.0 != note {
                text.0 = note;
            }
            color.set_if_neq(TextColor(note_color));
        }
    }
}

fn on_over(over: On<Pointer<Over>>, mut buttons: Query<&mut BuildButton>) {
    if let Ok(mut button) = buttons.get_mut(over.entity) {
        button.hovered = true;
    }
}

fn on_out(out: On<Pointer<Out>>, mut buttons: Query<&mut BuildButton>) {
    if let Ok(mut button) = buttons.get_mut(out.entity) {
        button.hovered = false;
    }
}

fn on_click(
    click: On<Pointer<Click>>,
    buttons: Query<&BuildButton>,
    mut choose: MessageWriter<ChooseBuilding>,
) {
    if click.event.button == PointerButton::Primary
        && let Ok(button) = buttons.get(click.entity)
    {
        choose.write(ChooseBuilding(button.building));
    }
}
