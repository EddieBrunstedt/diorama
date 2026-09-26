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

// The keeper: a little gnome in a red hat and a blue tunic.
pub const KEEPER_HAT: Color = Color::srgb_u8(0xD2, 0x46, 0x3C);
pub const KEEPER_TUNIC: Color = Color::srgb_u8(0x4A, 0x7C, 0xC0);
pub const KEEPER_BELT: Color = Color::srgb_u8(0x5A, 0x3A, 0x28);
pub const KEEPER_SKIN: Color = Color::srgb_u8(0xF2, 0xC0, 0x98);
pub const KEEPER_NOSE: Color = Color::srgb_u8(0xE8, 0x96, 0x80);
pub const KEEPER_BEARD: Color = Color::srgb_u8(0xF6, 0xF0, 0xE6);
pub const KEEPER_BOOTS: Color = Color::srgb_u8(0x5C, 0x40, 0x32);
pub const KEEPER_EYES: Color = Color::srgb_u8(0x2A, 0x22, 0x2E);
pub const BACKPACK: Color = Color::srgb_u8(0xA8, 0x74, 0x48);
pub const TOOL_METAL: Color = Color::srgb_u8(0xC8, 0xCE, 0xD8);
pub const TOOL_HANDLE: Color = Color::srgb_u8(0xB8, 0x84, 0x52);

// Towers.
pub const PLANK: Color = Color::srgb_u8(0xC0, 0x8A, 0x55);
pub const ROOF: Color = Color::srgb_u8(0x3E, 0x7E, 0xB0);
pub const ROOF_TOP: Color = Color::srgb_u8(0x5A, 0x9A, 0xC8);
pub const FLAG: Color = Color::srgb_u8(0xF4, 0xC8, 0x4A);
pub const BLUEPRINT: Color = Color::srgb_u8(0xC4, 0xE0, 0xFF);
pub const BLUEPRINT_PROBLEM: Color = Color::srgb_u8(0xFF, 0xB4, 0xA8);

// Enemies.
pub const SLIME: Color = Color::srgb_u8(0xA0, 0x84, 0xE8);
/// Flags builds that wait for the wave to end: a deeper slime lilac that reads on paper.
pub const WAVE_HOLD: Color = Color::srgb_u8(0x84, 0x64, 0xD4);

// The prepare phase and the bell.
pub const BRASS: Color = Color::srgb_u8(0xE6, 0xA8, 0x3E);
pub const COIN: Color = Color::srgb_u8(0xFF, 0xD0, 0x48);

// Orders and the route through them.
pub const ROUTE: Color = Color::srgb_u8(0xFF, 0xF6, 0xE4);
pub const ORDER_CHOP: Color = Color::srgb_u8(0xE8, 0x9A, 0x3C);
pub const ORDER_BUILD: Color = Color::srgb_u8(0x6C, 0xB8, 0x5C);
pub const ORDER_WALK: Color = Color::srgb_u8(0x7C, 0xA8, 0xDC);
pub const PROBLEM: Color = Color::srgb_u8(0xE0, 0x4A, 0x3E);

// UI.
pub const UI_PAPER: Color = Color::srgb_u8(0xFF, 0xF4, 0xE0);
pub const UI_PAPER_DARK: Color = Color::srgb_u8(0xE8, 0xD2, 0xB0);
pub const UI_INK: Color = Color::srgb_u8(0x4A, 0x36, 0x2C);
pub const UI_INK_SOFT: Color = Color::srgb_u8(0x8A, 0x70, 0x5E);
pub const UI_PROBLEM_PAPER: Color = Color::srgb_u8(0xFF, 0xD8, 0xCC);
