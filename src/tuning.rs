//! All tuning numbers in one place: how the game plays, how the camera moves, and how the
//! feedback feels. The grey-box look (shape sizes, light strength) stays next to the code that
//! draws it, and colors live in [`crate::palette`].
//!
//! Distances are in world units, and one grid cell is one world unit.

/// The map: cells grouped into chunks.
pub mod grid {
    /// Cells along one side of a chunk.
    pub const CHUNK_SIZE: i32 = 16;
    /// The starting map is this many chunks along each side, centered on the origin.
    pub const STARTING_CHUNKS: i32 = 2;
}

/// The overview camera: steep, looking straight up the map (rows stay level on screen).
pub mod camera {
    /// Vertical field of view. Narrow, so there's little perspective distortion.
    pub const FOV_DEGREES: f32 = 30.0;
    /// How far the camera looks down (90 is straight down).
    pub const PITCH_DEGREES: f32 = 62.0;
    /// Room around the map when the camera frames all of it (1.0 is edge to edge).
    pub const FRAMING_MARGIN: f32 = 1.12;
    /// Closest and farthest zoom, as the distance from the camera to the ground point it looks at.
    pub const MIN_DISTANCE: f32 = 10.0;
    pub const MAX_DISTANCE: f32 = 90.0;
    /// How much one scroll notch zooms, as a fraction of the distance.
    pub const ZOOM_STEP: f32 = 0.14;
    /// How quickly the camera eases to a new zoom (per second, exponential).
    pub const ZOOM_EASE_RATE: f32 = 16.0;
    /// Keyboard pan speed, as a fraction of the camera distance per second, so it feels the same
    /// at every zoom.
    pub const PAN_SPEED: f32 = 1.0;
    /// How far past the map's edge the camera may look.
    pub const PAN_OVERSCROLL: f32 = 4.0;
}

/// Feedback. Everything here is about feel, none of it changes the rules.
pub mod juice {
    /// A structure drops in from this high, taking this long (seconds), accelerating all the way.
    pub const DROP_HEIGHT: f32 = 7.0;
    pub const DROP_SECONDS: f32 = 0.13;
    /// When a drag places several walls in one frame, each one lands this much after the previous
    /// one, so a fast drag ripples down the line.
    pub const LINE_STAGGER: f32 = 0.022;
    /// When many walls change at once (e.g. undoing a long line), the stagger shrinks so the
    /// whole ripple takes at most this long.
    pub const MAX_RIPPLE_SECONDS: f32 = 0.3;
    /// Squash on landing (negative is flatter), and screen shake: trauma per landing, and the
    /// most that landings alone can build up.
    pub const LAND_SQUASH: f32 = -0.45;
    pub const LAND_SHAKE: f32 = 0.22;
    pub const LAND_SHAKE_MAX: f32 = 0.38;
    /// Dust kicked up on landing.
    pub const DUST_COUNT: usize = 16;
    pub const DUST_SIZE: f32 = 0.2;
    pub const DUST_SPREAD: f32 = 3.2;
    pub const DUST_LIFT: f32 = 1.3;
    pub const DUST_LIFETIME: f32 = 0.6;
    /// A removed structure pops up and shrinks away, throwing out some rubble.
    pub const REMOVE_SECONDS: f32 = 0.16;
    pub const RUBBLE_COUNT: usize = 7;
    pub const RUBBLE_SIZE: f32 = 0.16;
    pub const RUBBLE_SPREAD: f32 = 1.8;
    pub const RUBBLE_LIFT: f32 = 3.2;
    pub const RUBBLE_LIFETIME: f32 = 0.55;
    /// A click that can't do anything shakes what's in the way (radians), and the hover highlight
    /// flashes for this long (seconds).
    pub const DENIED_WOBBLE: f32 = 0.18;
    pub const DENIED_FLASH_SECONDS: f32 = 0.25;
    /// The hover highlight pops (stretch amount) when it moves to another cell.
    pub const HOVER_POP: f32 = 0.35;

    /// Longest frame step the springs integrate at once, so a hitch can't make them explode.
    pub const MAX_SPRING_STEP: f32 = 1.0 / 30.0;
    /// Squash & stretch spring.
    pub const SQUASH_STIFFNESS: f32 = 320.0;
    pub const SQUASH_DAMPING: f32 = 13.0;
    /// Wobble: how fast it dies down, how fast it shakes, and how long it lasts at most.
    pub const WOBBLE_DECAY: f32 = 6.0;
    pub const WOBBLE_FREQUENCY: f32 = 32.0;
    pub const WOBBLE_LIFETIME: f32 = 0.7;
    /// Particles fall and bounce on the ground.
    pub const PARTICLE_GRAVITY: f32 = 12.0;
    /// Trauma lost per second, and the camera offset at full trauma, as a fraction of the camera
    /// distance (so a shake looks the same at every zoom).
    pub const SHAKE_DECAY: f32 = 1.8;
    pub const SHAKE_MAX_OFFSET: f32 = 0.018;
    /// How fast the game runs during a hit-stop, relative to normal: nearly frozen.
    pub const HIT_STOP_SPEED: f32 = 0.03;
    /// Longest hit-stop, however many hits pile up (real seconds).
    pub const MAX_HIT_STOP: f32 = 0.2;
}
