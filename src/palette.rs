//! The game's color palette. Warm late-afternoon light, soft saturated greens, violet-gray stone.
//!
//! Keep colors here so the art direction stays consistent across modules.

use bevy::color::Color;

// Sky and atmosphere.
pub const SKY_TOP: Color = Color::srgb_u8(0x5C, 0x9E, 0xDA);
pub const SKY_HORIZON: Color = Color::srgb_u8(0xF8, 0xD8, 0xB6);
pub const FOG: Color = Color::srgb_u8(0xEE, 0xD4, 0xC0);
pub const SUNLIGHT: Color = Color::srgb_u8(0xFF, 0xE0, 0xBA);
pub const SKYLIGHT: Color = Color::srgb_u8(0xA8, 0xBF, 0xE6);
pub const CLOUD: Color = Color::srgb_u8(0xFF, 0xFA, 0xF4);
pub const CLOUD_SHADE: Color = Color::srgb_u8(0xDC, 0xD8, 0xEE);

// Ground.
pub const MEADOW_GRASS: Color = Color::srgb_u8(0x93, 0xC4, 0x5A);
pub const MEADOW_TUFT: Color = Color::srgb_u8(0xA2, 0xCE, 0x62);
pub const FOREST_GRASS: Color = Color::srgb_u8(0x62, 0x98, 0x4A);
pub const FOREST_MOSS: Color = Color::srgb_u8(0x55, 0x86, 0x44);
pub const DIRT: Color = Color::srgb_u8(0x8E, 0x5E, 0x3E);
pub const DIRT_DARK: Color = Color::srgb_u8(0x6C, 0x46, 0x31);
pub const STONE: Color = Color::srgb_u8(0x8E, 0x86, 0x94);
pub const STONE_DEEP: Color = Color::srgb_u8(0x55, 0x4C, 0x62);
pub const ROAD: Color = Color::srgb_u8(0xDC, 0xBA, 0x82);
pub const PEBBLE: Color = Color::srgb_u8(0xA8, 0xA0, 0x98);
pub const COBBLE: Color = Color::srgb_u8(0xBB, 0xAF, 0xA2);

// Flora.
pub const FLOWERS: [Color; 4] = [
    Color::srgb_u8(0xFF, 0xF4, 0xE0),
    Color::srgb_u8(0xFF, 0xD3, 0x4E),
    Color::srgb_u8(0xF5, 0x9B, 0xB5),
    Color::srgb_u8(0xB8, 0xA4, 0xF0),
];
pub const MUSHROOM_CAP: Color = Color::srgb_u8(0xD8, 0x47, 0x3A);
pub const MUSHROOM_STEM: Color = Color::srgb_u8(0xF1, 0xE6, 0xD2);
pub const BARK: Color = Color::srgb_u8(0x7A, 0x4E, 0x2D);
pub const LOG_END: Color = Color::srgb_u8(0xD8, 0xB2, 0x7A);
pub const PINE: Color = Color::srgb_u8(0x3C, 0x7A, 0x52);
pub const PINE_TIP: Color = Color::srgb_u8(0x5C, 0x9E, 0x5E);
pub const LEAVES: Color = Color::srgb_u8(0x74, 0xB0, 0x4C);
pub const LEAVES_AUTUMN: Color = Color::srgb_u8(0xE8, 0x9A, 0x3C);

// Hearth.
pub const HEARTH_STONE: Color = Color::srgb_u8(0xA0, 0x97, 0x8E);
pub const EMBER_BED: Color = Color::srgb(1.0, 0.22, 0.03);
pub const FLAME_OUTER: Color = Color::srgb(1.0, 0.3, 0.03);
pub const FLAME_CORE: Color = Color::srgb(1.0, 0.6, 0.08);
pub const FIRELIGHT: Color = Color::srgb(1.0, 0.62, 0.3);
