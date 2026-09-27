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
/// Mud: darker and greener than the ground, drawn glossy, with a lip of drier mud around a patch,
/// black puddles in it, and droplets flung up by brutes wading through.
pub const MUD: Color = Color::srgb(0.12, 0.12, 0.07);
pub const MUD_LIP: Color = Color::srgb(0.2, 0.18, 0.11);
pub const MUD_PUDDLE: Color = Color::srgb(0.05, 0.055, 0.035);
pub const MUD_SPLASH: [Color; 2] = [Color::srgb(0.34, 0.31, 0.19), Color::srgb(0.26, 0.24, 0.14)];

pub const CONCRETE: Color = Color::srgb(0.5, 0.5, 0.48);
/// A band of hazard yellow along the top of walls: built by humans.
pub const HAZARD: Color = Color::srgb(0.95, 0.72, 0.1);
/// The Command Center: worn steel over concrete, with cyan lights.
pub const STEEL: Color = Color::srgb(0.33, 0.37, 0.4);
pub const BASE_LIGHT: Color = Color::srgb(0.45, 0.95, 1.0);
/// How the Command Center glows for a moment when it's hit.
pub const BASE_HIT: Color = Color::srgb(1.0, 0.3, 0.15);

/// Towers: dark gunmetal turrets, red fuel tanks on the flamer, sandbags around the mortar, and
/// the flamer's pilot light.
pub const GUNMETAL: Color = Color::srgb(0.2, 0.22, 0.24);
pub const FUEL: Color = Color::srgb(0.72, 0.2, 0.12);
pub const SANDBAG: Color = Color::srgb(0.56, 0.5, 0.36);
pub const PILOT_LIGHT: Color = Color::srgb(1.0, 0.55, 0.1);

/// Shots: a hot yellow muzzle flash and tracer, sparks where they hit, fire from yellow to
/// deep orange, grey smoke, and a mortar blast's ring and light.
pub const MUZZLE_FLASH: Color = Color::srgb(1.0, 0.92, 0.55);
pub const TRACER: Color = Color::srgb(1.0, 0.85, 0.4);
pub const SPARKS: [Color; 2] = [Color::srgb(1.0, 0.9, 0.5), Color::srgb(1.0, 0.7, 0.3)];
pub const FLAME: [Color; 4] = [
    Color::srgb(1.0, 0.92, 0.45),
    Color::srgb(1.0, 0.7, 0.2),
    Color::srgb(1.0, 0.45, 0.1),
    Color::srgb(0.9, 0.25, 0.08),
];
pub const SMOKE: [Color; 2] = [Color::srgb(0.2, 0.2, 0.2), Color::srgb(0.3, 0.29, 0.28)];
pub const BLAST_RING: Color = Color::srgb(1.0, 0.75, 0.4);
pub const BLAST_LIGHT: Color = Color::srgb(1.0, 0.6, 0.25);

/// The horde: purple chitin, lighter on the back so a crowd reads against the dark ground, with
/// acid-green eyes. Nests sit in a crust of dark creep around a glowing pit.
pub const CHITIN: Color = Color::srgb(0.62, 0.24, 0.5);
pub const CHITIN_DARK: Color = Color::srgb(0.3, 0.1, 0.26);
pub const ACID: Color = Color::srgb(0.75, 1.0, 0.2);
pub const CREEP: Color = Color::srgb(0.2, 0.1, 0.19);
pub const NEST_PIT: Color = Color::srgb(0.08, 0.03, 0.07);
/// Brutes: darker, heavier shell than a swarmer's, with pale bone scythes and spikes.
pub const BRUTE_SHELL: Color = Color::srgb(0.36, 0.13, 0.27);
pub const BRUTE_PLATE: Color = Color::srgb(0.52, 0.2, 0.38);
pub const BONE: Color = Color::srgb(0.86, 0.8, 0.66);
/// Bits of a swarmer that bursts on the Command Center.
pub const GIBS: [Color; 3] = [
    Color::srgb(0.62, 0.24, 0.5),
    Color::srgb(0.4, 0.14, 0.34),
    Color::srgb(0.75, 1.0, 0.2),
];

/// A brute bursting: darker shell, bone, and acid.
pub const BRUTE_GIBS: [Color; 4] = [
    Color::srgb(0.36, 0.13, 0.27),
    Color::srgb(0.52, 0.2, 0.38),
    Color::srgb(0.86, 0.8, 0.66),
    Color::srgb(0.75, 1.0, 0.2),
];

/// Dust kicked up by a landing: lighter than the ground, so it reads against it.
pub const DUST: [Color; 3] = [
    Color::srgb(0.62, 0.57, 0.5),
    Color::srgb(0.72, 0.67, 0.6),
    Color::srgb(0.52, 0.48, 0.43),
];
pub const RUBBLE: [Color; 2] = [Color::srgb(0.66, 0.65, 0.62), Color::srgb(0.5, 0.49, 0.47)];
/// Wall damage: dark cracks, the rough concrete inside, and bent rebar where chunks broke off.
pub const CRACK: Color = Color::srgb(0.13, 0.12, 0.12);
pub const BROKEN_CONCRETE: Color = Color::srgb(0.4, 0.39, 0.37);
pub const REBAR: Color = Color::srgb(0.45, 0.25, 0.15);

/// Hover highlight: a free cell, a cell a click can't build on, and cells being cleared.
pub const HOVER_OK: Color = Color::srgb(0.45, 0.95, 1.0);
pub const HOVER_BLOCKED: Color = Color::srgb(1.0, 0.25, 0.2);
pub const HOVER_REMOVE: Color = Color::srgb(1.0, 0.6, 0.15);
/// A damaged structure a click would repair.
pub const HOVER_REPAIR: Color = Color::srgb(0.4, 1.0, 0.45);
/// Range rings: how far a tower reaches, and (for the mortar) how close it can't.
pub const RANGE_RING: Color = Color::srgba(0.45, 0.95, 1.0, 0.7);
pub const RANGE_MIN: Color = Color::srgba(1.0, 0.35, 0.25, 0.6);

/// The path preview: where the horde will go, in the horde's color, and red where it will go
/// through a wall.
pub const PATH_PREVIEW: Color = Color::srgb(0.95, 0.4, 0.85);
pub const PATH_BREACH: Color = Color::srgb(1.0, 0.25, 0.2);
/// The brutes' route, in amber so it doesn't mix up with the swarmers'.
pub const PATH_BRUTE: Color = Color::srgb(1.0, 0.62, 0.2);

/// The sun and the sky's fill light.
pub const SUN: Color = Color::srgb(1.0, 0.95, 0.88);
pub const SKYLIGHT: Color = Color::srgb(0.6, 0.7, 0.9);

/// Controls hint text.
pub const HINT_TEXT: Color = Color::srgba(0.85, 0.87, 0.9, 0.75);
/// The Command Center's health bar: filled, empty, and filled while it's low.
pub const HEALTH: Color = Color::srgb(0.45, 0.95, 1.0);
pub const HEALTH_EMPTY: Color = Color::srgba(1.0, 1.0, 1.0, 0.12);
pub const HEALTH_LOW: Color = Color::srgb(1.0, 0.3, 0.15);
/// A hovered wall's HP bar: the backing behind it.
pub const WALL_BAR_BACK: Color = Color::srgba(0.0, 0.0, 0.0, 0.6);
/// Money: the counter and the pips flying to it, and the counter's flash when something can't
/// be paid for.
pub const MONEY: Color = Color::srgb(1.0, 0.82, 0.3);
pub const BROKE: Color = Color::srgb(1.0, 0.3, 0.2);
/// The build panel: its buttons, the chosen one's border, and prices that can't be paid.
pub const PANEL: Color = Color::srgba(0.05, 0.06, 0.08, 0.8);
pub const PANEL_HOVER: Color = Color::srgba(0.12, 0.14, 0.18, 0.9);
pub const PANEL_BORDER: Color = Color::srgba(1.0, 1.0, 1.0, 0.12);
pub const PANEL_CHOSEN: Color = Color::srgb(0.45, 0.95, 1.0);
/// The wave loop on screen: a heavy wave's title, the Start button (and speed button) with its
/// hover, the "wave cleared" banner, and the game-over screen over a darkened map.
pub const HEAVY_WAVE: Color = Color::srgb(1.0, 0.62, 0.2);
pub const START_BUTTON: Color = Color::srgba(0.1, 0.3, 0.34, 0.9);
pub const START_BUTTON_HOVER: Color = Color::srgba(0.16, 0.42, 0.48, 0.95);
pub const BANNER_TEXT: Color = Color::srgb(0.9, 0.95, 1.0);
pub const GAME_OVER_TITLE: Color = Color::srgb(1.0, 0.35, 0.2);
pub const GAME_OVER_BACKDROP: Color = Color::srgba(0.02, 0.02, 0.03, 0.6);

/// Abilities. The sniper shot's tracer is the Command Center's light, white-hot; its reticle and
/// laser sight are cyan on a target and red with nothing to hit. The barrage's circle is amber
/// while aiming, and red where it's coming down.
pub const SNIPER_TRACER: Color = Color::srgb(0.8, 0.98, 1.0);
pub const AIM: Color = Color::srgb(0.45, 0.95, 1.0);
pub const AIM_NONE: Color = Color::srgb(1.0, 0.25, 0.2);
pub const LASER: Color = Color::srgba(0.45, 0.95, 1.0, 0.6);
pub const BARRAGE_AIM: Color = Color::srgb(1.0, 0.62, 0.2);
pub const BARRAGE_ZONE: Color = Color::srgb(1.0, 0.25, 0.15);
/// How see-through the fill inside an aiming circle or a barrage zone is.
pub const AIM_FILL_ALPHA: f32 = 0.14;
/// The ability bar: a ready slot's border, a charge pip (ready and reloading), and the reload
/// bar's track and fill (reloading, and full).
pub const ABILITY_READY: Color = Color::srgb(0.9, 0.95, 1.0);
pub const ABILITY_PIP: Color = Color::srgb(0.45, 0.95, 1.0);
pub const ABILITY_PIP_EMPTY: Color = Color::srgba(1.0, 1.0, 1.0, 0.15);
pub const ABILITY_TRACK: Color = Color::srgba(1.0, 1.0, 1.0, 0.1);
pub const ABILITY_RELOAD: Color = Color::srgba(0.45, 0.95, 1.0, 0.45);
pub const ABILITY_FULL: Color = Color::srgb(0.45, 0.95, 1.0);

/// New ground on offer after a wave: the chunks are drawn dimmed and cool until the cursor is on
/// one, framed in cyan (plain ground) or the horde's red (a chunk with a nest, a new front), and
/// named by a label.
pub const OFFER_TINT: Color = Color::srgb(0.42, 0.48, 0.58);
pub const OFFER_TINT_HOVER: Color = Color::srgb(0.9, 0.95, 1.0);
pub const OFFER_FRAME: Color = Color::srgb(0.45, 0.95, 1.0);
pub const OFFER_FRAME_NEST: Color = Color::srgb(1.0, 0.3, 0.35);
