//! Labels over the chunks offered after a wave: each chunk's name, and for one with a nest, a
//! warning and what claiming it pays (and, once there are as many live nests as there can be,
//! that the oldest goes quiet). The label of the chunk under the cursor lights up.

use crate::camera::MainCamera;
use crate::chunks::{ChunkOffer, OfferHover, chunk_area, templates::Claim};
use crate::grid::Grid;
use crate::palette;
use crate::tuning::chunks::LIVE_NESTS;
use bevy::prelude::*;

const NAME_FONT_SIZE: f32 = 16.0;
const NOTE_FONT_SIZE: f32 = 13.0;

pub struct OfferHudPlugin;

impl Plugin for OfferHudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (spawn_labels, place_labels).chain());
    }
}

/// A label over an offered chunk.
#[derive(Component)]
struct OfferLabel(Claim);

/// Keeps a label for each offered chunk while one has to be claimed.
fn spawn_labels(
    mut commands: Commands,
    grid: Res<Grid>,
    offer: Res<ChunkOffer>,
    labels: Query<(Entity, &OfferLabel)>,
) {
    if !offer.is_changed() && !grid.is_changed() {
        return;
    }
    let quiets = grid.live_nests().len() >= LIVE_NESTS;
    let (wanted, bounty) = match &offer.0 {
        Some(offer) if offer.pending(&grid) => (offer.choices.clone(), offer.bounty),
        _ => (Vec::new(), 0),
    };
    let shown: Vec<Claim> = labels.iter().map(|(_, label)| label.0).collect();
    if shown == wanted {
        return;
    }
    for (entity, _) in &labels {
        commands.entity(entity).despawn();
    }
    for claim in wanted {
        let nest = claim.nest().is_some();
        let color = if nest {
            palette::OFFER_FRAME_NEST
        } else {
            palette::OFFER_FRAME
        };
        commands
            .spawn((
                OfferLabel(claim),
                Node {
                    position_type: PositionType::Absolute,
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(3.0)),
                    ..default()
                },
                BackgroundColor(palette::PANEL),
                BorderColor::all(color.with_alpha(0.5)),
                Visibility::Hidden,
                Pickable::IGNORE,
            ))
            .with_children(|label| {
                label.spawn((
                    Text::new(claim.template().name),
                    TextFont::from_font_size(NAME_FONT_SIZE),
                    TextColor(palette::BANNER_TEXT),
                    Pickable::IGNORE,
                ));
                let note = if nest {
                    format!("New nest  +{bounty}")
                } else {
                    "No nest".to_string()
                };
                label.spawn((
                    Text::new(note),
                    TextFont::from_font_size(NOTE_FONT_SIZE),
                    TextColor(if nest { color } else { palette::HINT_TEXT }),
                    Pickable::IGNORE,
                ));
                if nest && quiets {
                    label.spawn((
                        Text::new("Oldest nest goes quiet"),
                        TextFont::from_font_size(NOTE_FONT_SIZE),
                        TextColor(palette::HINT_TEXT),
                        Pickable::IGNORE,
                    ));
                }
            });
    }
}

/// Keeps each label over the middle of its chunk, lit up while the cursor is on the chunk.
fn place_labels(
    hover: Res<OfferHover>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut labels: Query<(
        &OfferLabel,
        &mut Node,
        &mut Visibility,
        &mut BackgroundColor,
        &ComputedNode,
    )>,
) {
    let (camera, transform) = *camera;
    for (label, mut node, mut visibility, mut background, computed) in &mut labels {
        let middle = chunk_area(label.0.slot).center();
        let Ok(at) = camera.world_to_viewport(transform, Vec3::new(middle.x, 0.0, middle.y)) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        let size = computed.size() * computed.inverse_scale_factor();
        node.left = Val::Px(at.x - size.x / 2.0);
        node.top = Val::Px(at.y - size.y / 2.0);
        // Hidden until it's been laid out once, so it doesn't flash in the corner.
        if size.x > 0.0 {
            visibility.set_if_neq(Visibility::Inherited);
        }
        let hovered = hover.0 == Some(label.0);
        background.set_if_neq(BackgroundColor(if hovered {
            palette::PANEL_HOVER
        } else {
            palette::PANEL
        }));
    }
}
