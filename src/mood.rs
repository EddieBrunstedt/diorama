//! Lighting moods: how the island is lit in each phase, and the easing from one mood to the next.
//!
//! Each phase has its own [`Mood`]: a calm, warm afternoon while preparing, a stormy dusk during
//! the wave (lit by the hearth, tower lanterns and glowing slimes), a golden evening while a tile
//! is picked, and a cold gray one when the hearth has gone out. In the last seconds of the prepare
//! phase the storm starts gathering. Moods are plain data; [`CurrentMood`] eases toward the one
//! the game is in, and the plugins that own the lights, the sky and the camera apply it.

use crate::palette;
use crate::phase::{GameState, PrepareClock};
use bevy::prelude::*;

/// Seconds before the wave in which the storm starts gathering.
const GATHERING_SECONDS: f32 = 8.0;
/// How far the calm mood has turned toward the wave's by the time the wave starts.
const GATHERING_MAX: f32 = 0.3;

pub struct MoodPlugin;

impl Plugin for MoodPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CurrentMood>()
            .init_resource::<ForcedMood>()
            .add_systems(Update, follow_mood.in_set(MoodSystems));
    }
}

/// Updates [`CurrentMood`]. Systems applying the mood run after it.
#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct MoodSystems;

/// Which mood the game is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoodKind {
    /// Preparing: a warm, calm afternoon.
    Calm,
    /// The wave: a stormy dusk.
    Wave,
    /// Picking a tile: a golden evening after the storm.
    Offer,
    /// Game over: cold and gray.
    Lost,
}

impl MoodKind {
    pub fn for_state(state: GameState) -> Self {
        match state {
            GameState::Intro | GameState::Prepare => Self::Calm,
            GameState::Wave => Self::Wave,
            GameState::TileOffer => Self::Offer,
            GameState::GameOver => Self::Lost,
        }
    }

    /// Seconds to ease into this mood.
    pub fn ease_seconds(self) -> f32 {
        match self {
            Self::Calm => 2.5,
            Self::Wave => 2.5,
            Self::Offer => 3.0,
            Self::Lost => 4.0,
        }
    }

    pub fn mood(self) -> Mood {
        match self {
            Self::Calm => Mood::CALM,
            Self::Wave => Mood::WAVE,
            Self::Offer => Mood::OFFER,
            Self::Lost => Mood::LOST,
        }
    }
}

/// Everything about how the island is lit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mood {
    pub sun: Color,
    /// Lux.
    pub sun_illuminance: f32,
    /// Degrees above the horizon.
    pub sun_elevation: f32,
    /// Degrees around the island the sun shines from: 0 is south (+z), 90 is east (+x).
    pub sun_azimuth: f32,
    pub sky_top: Color,
    pub sky_horizon: Color,
    /// The distance fog the clouds and far-off things fade into.
    pub fog: Color,
    /// Light from the sky, filling the shadows.
    pub skylight: Color,
    pub skylight_brightness: f32,
    /// How much the clouds glow, so they don't go dull in the fog.
    pub cloud_glow: Color,
    /// Density of the haze the light shafts shine through.
    pub haze: f32,
    /// Exposure compensation, in stops.
    pub exposure: f32,
    /// White balance: negative is cooler, positive warmer.
    pub temperature: f32,
    pub saturation: f32,
    /// Darkening toward the screen's corners, 0 to 1.
    pub vignette: f32,
    pub bloom: f32,
    /// How bright the hearth's fire lights its surroundings, compared to normal.
    pub firelight: f32,
    /// Lanterns, glowing arrows and burning boulders, 0 (unlit) to 1.
    pub lamps: f32,
    /// How much slimes and their goo glow, 0 to 1.
    pub slime_glow: f32,
}

impl Mood {
    pub const CALM: Self = Self {
        sun: palette::SUNLIGHT,
        sun_illuminance: 10_000.0,
        sun_elevation: 45.0,
        sun_azimuth: -51.0,
        sky_top: palette::SKY_TOP,
        sky_horizon: palette::SKY_HORIZON,
        fog: palette::FOG,
        skylight: palette::SKYLIGHT,
        skylight_brightness: 480.0,
        cloud_glow: palette::CLOUD_GLOW,
        haze: 0.03,
        exposure: 0.0,
        temperature: 0.0,
        saturation: 1.12,
        vignette: 0.15,
        bloom: 0.18,
        firelight: 1.0,
        lamps: 0.0,
        slime_glow: 0.0,
    };

    pub const WAVE: Self = Self {
        sun: palette::DUSK_SUN,
        sun_illuminance: 2_800.0,
        sun_elevation: 20.0,
        sun_azimuth: -72.0,
        sky_top: palette::DUSK_SKY_TOP,
        sky_horizon: palette::DUSK_HORIZON,
        fog: palette::DUSK_FOG,
        skylight: palette::DUSK_SKYLIGHT,
        skylight_brightness: 700.0,
        cloud_glow: palette::DUSK_CLOUD_GLOW,
        haze: 0.12,
        exposure: 0.35,
        temperature: -0.05,
        saturation: 1.1,
        vignette: 0.45,
        bloom: 0.3,
        firelight: 1.8,
        lamps: 1.0,
        slime_glow: 1.0,
    };

    pub const OFFER: Self = Self {
        sun: palette::GOLDEN_SUN,
        sun_illuminance: 7_500.0,
        sun_elevation: 30.0,
        sun_azimuth: -40.0,
        sky_top: palette::GOLDEN_SKY_TOP,
        sky_horizon: palette::GOLDEN_HORIZON,
        fog: palette::GOLDEN_FOG,
        skylight: palette::GOLDEN_SKYLIGHT,
        skylight_brightness: 560.0,
        cloud_glow: palette::GOLDEN_CLOUD_GLOW,
        haze: 0.06,
        exposure: 0.05,
        temperature: 0.0,
        saturation: 1.05,
        vignette: 0.2,
        bloom: 0.24,
        firelight: 1.2,
        lamps: 0.6,
        slime_glow: 0.0,
    };

    pub const LOST: Self = Self {
        sun: palette::LOST_SUN,
        sun_illuminance: 2_500.0,
        sun_elevation: 18.0,
        sun_azimuth: -80.0,
        sky_top: palette::LOST_SKY_TOP,
        sky_horizon: palette::LOST_HORIZON,
        fog: palette::LOST_FOG,
        skylight: palette::LOST_SKYLIGHT,
        skylight_brightness: 380.0,
        cloud_glow: palette::LOST_CLOUD_GLOW,
        haze: 0.1,
        exposure: 0.15,
        temperature: -0.1,
        saturation: 0.55,
        vignette: 0.6,
        bloom: 0.15,
        firelight: 1.0,
        lamps: 0.0,
        slime_glow: 0.6,
    };

    /// This mood blended `t` (0 to 1) of the way toward `other`.
    pub fn lerp(&self, other: &Self, t: f32) -> Self {
        let color =
            |a: Color, b: Color| -> Color { Oklaba::from(a).mix(&Oklaba::from(b), t).into() };
        let number = |a: f32, b: f32| a.lerp(b, t);
        Self {
            sun: color(self.sun, other.sun),
            sun_illuminance: number(self.sun_illuminance, other.sun_illuminance),
            sun_elevation: number(self.sun_elevation, other.sun_elevation),
            sun_azimuth: number(self.sun_azimuth, other.sun_azimuth),
            sky_top: color(self.sky_top, other.sky_top),
            sky_horizon: color(self.sky_horizon, other.sky_horizon),
            fog: color(self.fog, other.fog),
            skylight: color(self.skylight, other.skylight),
            skylight_brightness: number(self.skylight_brightness, other.skylight_brightness),
            cloud_glow: color(self.cloud_glow, other.cloud_glow),
            haze: number(self.haze, other.haze),
            exposure: number(self.exposure, other.exposure),
            temperature: number(self.temperature, other.temperature),
            saturation: number(self.saturation, other.saturation),
            vignette: number(self.vignette, other.vignette),
            bloom: number(self.bloom, other.bloom),
            firelight: number(self.firelight, other.firelight),
            lamps: number(self.lamps, other.lamps),
            slime_glow: number(self.slime_glow, other.slime_glow),
        }
    }

    /// The direction sunlight travels in (from the sun toward the island).
    pub fn sun_direction(&self) -> Vec3 {
        let (elevation, azimuth) = (
            self.sun_elevation.to_radians(),
            self.sun_azimuth.to_radians(),
        );
        -Vec3::new(
            azimuth.sin() * elevation.cos(),
            elevation.sin(),
            azimuth.cos() * elevation.cos(),
        )
    }

    /// The mood `kind` calls for. While preparing, the storm gathers as the wave draws near.
    pub fn target(kind: MoodKind, seconds_to_wave: f32) -> Self {
        let mood = kind.mood();
        match kind {
            MoodKind::Calm => mood.lerp(&Mood::WAVE, gathering(seconds_to_wave)),
            _ => mood,
        }
    }
}

/// How far the calm mood has turned toward the wave's, this many seconds before the wave.
pub fn gathering(seconds_to_wave: f32) -> f32 {
    let t = (1.0 - seconds_to_wave / GATHERING_SECONDS).clamp(0.0, 1.0);
    GATHERING_MAX * EaseFunction::SineIn.sample_clamped(t)
}

/// The mood the island is lit in right now, easing toward the one the game is in.
#[derive(Resource, Clone, Debug)]
pub struct CurrentMood {
    pub mood: Mood,
    /// Where the current transition started.
    from: Mood,
    kind: MoodKind,
    /// How far the current transition is, 0 to 1.
    progress: f32,
}

impl Default for CurrentMood {
    fn default() -> Self {
        Self::new(MoodKind::Calm)
    }
}

impl CurrentMood {
    pub fn new(kind: MoodKind) -> Self {
        Self {
            mood: kind.mood(),
            from: kind.mood(),
            kind,
            progress: 1.0,
        }
    }

    /// Eases `seconds` further toward `target`, the mood for `kind`. Changing kind starts a new
    /// transition from wherever the light is now, so nothing jumps.
    pub fn advance(&mut self, kind: MoodKind, target: &Mood, seconds: f32) {
        if kind != self.kind {
            self.from = self.mood;
            self.kind = kind;
            self.progress = 0.0;
        }
        self.progress = (self.progress + seconds / kind.ease_seconds()).min(1.0);
        let eased = EaseFunction::SineInOut.sample_clamped(self.progress);
        self.mood = self.from.lerp(target, eased);
    }
}

/// Holds the island in one mood whatever the phase, e.g. to compare moods in screenshots.
#[derive(Resource, Default)]
pub struct ForcedMood(pub Option<MoodKind>);

fn follow_mood(
    time: Res<Time>,
    state: Res<State<GameState>>,
    clock: Res<PrepareClock>,
    forced: Res<ForcedMood>,
    mut current: ResMut<CurrentMood>,
) {
    let kind = forced
        .0
        .unwrap_or_else(|| MoodKind::for_state(*state.get()));
    let seconds_to_wave = if *state.get() == GameState::Prepare && forced.0.is_none() {
        clock.left
    } else {
        f32::INFINITY
    };
    let target = Mood::target(kind, seconds_to_wave);
    current.advance(kind, &target, time.delta_secs());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: &Mood, b: &Mood) -> bool {
        let near = |x: f32, y: f32| (x - y).abs() < 1e-3;
        let near_color = |x: Color, y: Color| {
            let (x, y) = (x.to_linear(), y.to_linear());
            near(x.red, y.red) && near(x.green, y.green) && near(x.blue, y.blue)
        };
        near(a.sun_illuminance, b.sun_illuminance)
            && near(a.sun_elevation, b.sun_elevation)
            && near(a.exposure, b.exposure)
            && near(a.lamps, b.lamps)
            && near(a.vignette, b.vignette)
            && near_color(a.sun, b.sun)
            && near_color(a.sky_top, b.sky_top)
            && near_color(a.fog, b.fog)
    }

    #[test]
    fn each_phase_has_its_mood() {
        assert_eq!(MoodKind::for_state(GameState::Intro), MoodKind::Calm);
        assert_eq!(MoodKind::for_state(GameState::Prepare), MoodKind::Calm);
        assert_eq!(MoodKind::for_state(GameState::Wave), MoodKind::Wave);
        assert_eq!(MoodKind::for_state(GameState::TileOffer), MoodKind::Offer);
        assert_eq!(MoodKind::for_state(GameState::GameOver), MoodKind::Lost);
    }

    #[test]
    fn the_wave_is_darker_and_lit_by_lamps() {
        let (calm, wave) = (Mood::CALM, Mood::WAVE);
        assert!(wave.sun_illuminance < calm.sun_illuminance);
        assert!(wave.sun_elevation < calm.sun_elevation, "longer shadows");
        assert!(wave.vignette > calm.vignette);
        assert_eq!((calm.lamps, wave.lamps), (0.0, 1.0));
        assert_eq!((calm.slime_glow, wave.slime_glow), (0.0, 1.0));
        assert!(Mood::LOST.saturation < calm.saturation, "game over is gray");
    }

    #[test]
    fn blending_goes_from_one_mood_to_the_other() {
        let (calm, wave) = (Mood::CALM, Mood::WAVE);
        assert!(close(&calm.lerp(&wave, 0.0), &calm));
        assert!(close(&calm.lerp(&wave, 1.0), &wave));
        let half = calm.lerp(&wave, 0.5);
        assert!((half.lamps - 0.5).abs() < 1e-6);
        let expected = (calm.sun_illuminance + wave.sun_illuminance) / 2.0;
        assert!((half.sun_illuminance - expected).abs() < 1e-3);
    }

    #[test]
    fn the_sun_shines_down_from_where_it_stands() {
        let mut mood = Mood::CALM;
        (mood.sun_elevation, mood.sun_azimuth) = (30.0, 0.0);
        let direction = mood.sun_direction();
        assert!(direction.y < 0.0, "downward");
        assert!(direction.z < 0.0, "from the south, so toward the north");
        assert!(direction.x.abs() < 1e-6);
        assert!((direction.length() - 1.0).abs() < 1e-6);
        assert!((direction.y + 0.5).abs() < 1e-6, "30 degrees up");
    }

    #[test]
    fn the_storm_gathers_before_the_wave() {
        assert_eq!(gathering(30.0), 0.0);
        assert_eq!(gathering(GATHERING_SECONDS), 0.0);
        assert!(gathering(GATHERING_SECONDS / 2.0) > 0.0);
        assert!(
            gathering(GATHERING_SECONDS / 2.0) < GATHERING_MAX / 2.0,
            "slow at first"
        );
        assert!((gathering(0.0) - GATHERING_MAX).abs() < 1e-6);
        assert!(close(&Mood::target(MoodKind::Calm, 20.0), &Mood::CALM));
        let gathered = Mood::target(MoodKind::Calm, 0.0);
        assert!((gathered.lamps - GATHERING_MAX).abs() < 1e-6);
        assert!(close(&Mood::target(MoodKind::Wave, 0.0), &Mood::WAVE));
    }

    #[test]
    fn moods_ease_in_rather_than_snap() {
        let mut current = CurrentMood::new(MoodKind::Calm);
        let wave = MoodKind::Wave.mood();
        let seconds = MoodKind::Wave.ease_seconds();
        current.advance(MoodKind::Wave, &wave, 0.01 * seconds);
        let first = current.mood.lamps;
        assert!(first > 0.0 && first < 0.001, "starts slowly: {first}");
        current.advance(MoodKind::Wave, &wave, 0.49 * seconds);
        assert!(
            (current.mood.lamps - 0.5).abs() < 1e-3,
            "halfway at half time"
        );
        current.advance(MoodKind::Wave, &wave, 0.5 * seconds);
        assert!(close(&current.mood, &wave), "there after the ease time");
        current.advance(MoodKind::Wave, &wave, 1.0);
        assert!(close(&current.mood, &wave), "and stays");
    }

    #[test]
    fn changing_course_midway_starts_from_the_current_light() {
        let mut current = CurrentMood::new(MoodKind::Calm);
        let wave = MoodKind::Wave.mood();
        current.advance(MoodKind::Wave, &wave, 0.5 * MoodKind::Wave.ease_seconds());
        let midway = current.mood;
        let lost = MoodKind::Lost.mood();
        current.advance(MoodKind::Lost, &lost, 1e-4);
        assert!(
            close(&current.mood, &midway),
            "no jump when the target changes"
        );
        assert_eq!(current.kind, MoodKind::Lost);
        current.advance(MoodKind::Lost, &lost, MoodKind::Lost.ease_seconds());
        assert!(close(&current.mood, &lost));
    }
}
