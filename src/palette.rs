//! Grey-box colors: a dark planet surface, pale concrete, and clear signal colors for feedback.

use bevy::prelude::*;

/// The void around the map.
pub const BACKGROUND: Color = Color::srgb(0.035, 0.04, 0.05);
/// Bare ground, and the darker gaps between cells that make the grid readable.
pub const GROUND: Color = Color::srgb(0.22, 0.2, 0.19);
pub const GROUND_GAP: Color = Color::srgb(0.12, 0.11, 0.11);
/// The seam along chunk borders, a little lighter than the gaps between cells.
pub const CHUNK_SEAM: Color = Color::srgb(0.3, 0.27, 0.24);
/// Alien rock: darker and redder than anything built, so terrain and structures never mix up.
pub const ROCK: Color = Color::srgb(0.3, 0.19, 0.17);

pub const CONCRETE: Color = Color::srgb(0.5, 0.5, 0.48);
/// A band of hazard yellow along the top of walls: built by humans.
pub const HAZARD: Color = Color::srgb(0.95, 0.72, 0.1);

/// Dust kicked up by a landing: lighter than the ground, so it reads against it.
pub const DUST: [Color; 3] = [
    Color::srgb(0.62, 0.57, 0.5),
    Color::srgb(0.72, 0.67, 0.6),
    Color::srgb(0.52, 0.48, 0.43),
];
pub const RUBBLE: [Color; 2] = [Color::srgb(0.66, 0.65, 0.62), Color::srgb(0.5, 0.49, 0.47)];

/// Hover highlight: a free cell, a cell a click can't build on, and cells being cleared.
pub const HOVER_OK: Color = Color::srgb(0.45, 0.95, 1.0);
pub const HOVER_BLOCKED: Color = Color::srgb(1.0, 0.25, 0.2);
pub const HOVER_REMOVE: Color = Color::srgb(1.0, 0.6, 0.15);

/// The sun and the sky's fill light.
pub const SUN: Color = Color::srgb(1.0, 0.95, 0.88);
pub const SKYLIGHT: Color = Color::srgb(0.6, 0.7, 0.9);

/// Controls hint text.
pub const HINT_TEXT: Color = Color::srgba(0.85, 0.87, 0.9, 0.75);
