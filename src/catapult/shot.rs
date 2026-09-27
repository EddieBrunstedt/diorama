//! Working the catapult, as plain data: charging while the button is held, what a shot at some
//! charge does, where it can land, how long the boulder flies, and which slimes it hits.
//!
//! One shot at a time: after firing, the arm has to be wound back and a new boulder loaded
//! ([`Loader::Reloading`]) before it can be charged again. So shots are big and deliberate, and
//! clicking fast gets nothing.

use crate::tuning::{
    BOULDER_FLIGHT_PER_TILE, BOULDER_FLIGHT_SECONDS, CATAPULT_CHARGE_SECONDS, CATAPULT_DAMAGE_MAX,
    CATAPULT_DAMAGE_MIN, CATAPULT_MIN_CHARGE, CATAPULT_MIN_RANGE, CATAPULT_RANGE,
    CATAPULT_RELOAD_SECONDS, CATAPULT_SPLASH_MAX, CATAPULT_SPLASH_MIN,
};
use crate::voxel::TILE_SIZE;
use bevy::prelude::*;

/// Where the catapult is between shots.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Loader {
    /// Loaded and waiting.
    #[default]
    Ready,
    /// Being charged: seconds the button has been held.
    Charging { held: f32 },
    /// Winding back after a shot: seconds left.
    Reloading { left: f32 },
}

impl Loader {
    /// Starts charging, if it's loaded. Returns whether it started.
    pub fn press(&mut self) -> bool {
        if *self != Loader::Ready {
            return false;
        }
        *self = Loader::Charging { held: 0.0 };
        true
    }

    /// Advances time: the charge builds up, or the reload goes on.
    pub fn tick(&mut self, seconds: f32) {
        match self {
            Loader::Ready => {}
            Loader::Charging { held } => *held += seconds,
            Loader::Reloading { left } => {
                *left -= seconds;
                if *left <= 0.0 {
                    *self = Loader::Ready;
                }
            }
        }
    }

    /// Lets go. Returns the charge to fire with, and starts reloading, if it was charged enough;
    /// otherwise it just relaxes.
    pub fn release(&mut self) -> Option<f32> {
        let charge = self.charge();
        if !matches!(self, Loader::Charging { .. }) {
            return None;
        }
        if charge < CATAPULT_MIN_CHARGE {
            *self = Loader::Ready;
            return None;
        }
        *self = Loader::Reloading {
            left: CATAPULT_RELOAD_SECONDS,
        };
        Some(charge)
    }

    /// Stops charging without firing (e.g. the keeper got out).
    pub fn cancel(&mut self) {
        if matches!(self, Loader::Charging { .. }) {
            *self = Loader::Ready;
        }
    }

    /// How charged it is, from 0 to 1.
    pub fn charge(&self) -> f32 {
        match self {
            Loader::Charging { held } => (held / CATAPULT_CHARGE_SECONDS).min(1.0),
            _ => 0.0,
        }
    }

    /// How far along the reload is, from 0 (just fired) to 1 (loaded).
    pub fn loaded(&self) -> f32 {
        match self {
            Loader::Reloading { left } => 1.0 - (left / CATAPULT_RELOAD_SECONDS).clamp(0.0, 1.0),
            _ => 1.0,
        }
    }
}

/// What a shot does where it lands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shot {
    pub damage: u32,
    /// Splash radius in world units.
    pub radius: f32,
    /// Fired at full charge.
    pub full: bool,
}

/// The shot a release at `charge` (0 to 1) fires.
pub fn shot(charge: f32) -> Shot {
    let charge = charge.clamp(0.0, 1.0);
    let damage = (CATAPULT_DAMAGE_MIN as f32).lerp(CATAPULT_DAMAGE_MAX as f32, charge);
    Shot {
        damage: damage.round() as u32,
        radius: CATAPULT_SPLASH_MIN.lerp(CATAPULT_SPLASH_MAX, charge) * TILE_SIZE,
        full: charge >= 1.0,
    }
}

/// Where a shot aimed at `aim` from a catapult at `from` lands (world x/z): the aim, pulled into
/// the catapult's reach. Also says whether it had to be pulled in.
pub fn clamp_aim(from: Vec2, aim: Vec2) -> (Vec2, bool) {
    let (near, far) = (CATAPULT_MIN_RANGE * TILE_SIZE, CATAPULT_RANGE * TILE_SIZE);
    let offset = aim - from;
    let distance = offset.length();
    let direction = offset.normalize_or(Vec2::Y);
    let clamped = distance.clamp(near, far);
    (
        from + direction * clamped,
        (clamped - distance).abs() > 1e-4,
    )
}

/// How long a boulder flies to land `distance` away, and how high it arcs.
pub fn flight(distance: f32) -> (f32, f32) {
    let tiles = distance / TILE_SIZE;
    let seconds = BOULDER_FLIGHT_SECONDS + BOULDER_FLIGHT_PER_TILE * tiles;
    let height = 0.55 + 0.2 * tiles;
    (seconds, height)
}

/// Where a boulder is at `t` (0 to 1) of its flight: a tall arc from `from` to `to`.
pub fn boulder_position(from: Vec3, to: Vec3, height: f32, t: f32) -> Vec3 {
    from.lerp(to, t) + Vec3::Y * height * 4.0 * t * (1.0 - t)
}

/// The slimes a shot landing at `center` hits: every one within `radius`.
pub fn splash<T>(center: Vec2, radius: f32, slimes: impl IntoIterator<Item = (T, Vec2)>) -> Vec<T> {
    slimes
        .into_iter()
        .filter(|(_, position)| position.distance(center) <= radius)
        .map(|(id, _)| id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charges_while_held_and_fires_once() {
        let mut loader = Loader::default();
        assert!(loader.press());
        assert!(!loader.press(), "already charging");
        loader.tick(CATAPULT_CHARGE_SECONDS / 2.0);
        assert!((loader.charge() - 0.5).abs() < 1e-5);
        loader.tick(10.0);
        assert_eq!(loader.charge(), 1.0, "tops out");
        assert_eq!(loader.release(), Some(1.0));
        assert_eq!(loader.release(), None, "nothing left to fire");
        assert!(!loader.press(), "reloading");
        assert_eq!(loader.loaded(), 0.0);
        loader.tick(CATAPULT_RELOAD_SECONDS / 2.0);
        assert!((loader.loaded() - 0.5).abs() < 1e-5);
        loader.tick(CATAPULT_RELOAD_SECONDS);
        assert_eq!(loader, Loader::Ready);
        assert!(loader.press());
    }

    #[test]
    fn a_tap_fires_nothing() {
        let mut loader = Loader::default();
        loader.press();
        loader.tick(CATAPULT_CHARGE_SECONDS * CATAPULT_MIN_CHARGE * 0.5);
        assert_eq!(loader.release(), None);
        assert_eq!(loader, Loader::Ready, "no reload for a fizzle");
    }

    #[test]
    fn cancelling_keeps_the_boulder() {
        let mut loader = Loader::default();
        loader.press();
        loader.tick(1.0);
        loader.cancel();
        assert_eq!(loader, Loader::Ready);
    }

    #[test]
    fn fuller_charges_hit_harder_and_wider() {
        let weak = shot(0.0);
        let full = shot(1.0);
        assert_eq!(weak.damage, CATAPULT_DAMAGE_MIN);
        assert_eq!(full.damage, CATAPULT_DAMAGE_MAX);
        assert!(full.radius > weak.radius);
        assert!(full.full && !shot(0.99).full);
        assert!(shot(0.5).damage > weak.damage && shot(0.5).damage < full.damage);
    }

    #[test]
    fn aims_are_pulled_into_reach() {
        let from = Vec2::new(1.0, 1.0);
        let (at, clamped) = clamp_aim(from, from + Vec2::new(2.0, 0.0));
        assert_eq!((at, clamped), (from + Vec2::new(2.0, 0.0), false));
        let (at, clamped) = clamp_aim(from, from + Vec2::new(0.0, 99.0));
        assert!(clamped);
        assert!((at.distance(from) - CATAPULT_RANGE * TILE_SIZE).abs() < 1e-4);
        let (at, clamped) = clamp_aim(from, from + Vec2::new(0.1, 0.0));
        assert!(clamped, "too close");
        assert!((at.distance(from) - CATAPULT_MIN_RANGE * TILE_SIZE).abs() < 1e-4);
    }

    #[test]
    fn farther_shots_fly_longer_and_higher() {
        let (near_time, near_height) = flight(1.0);
        let (far_time, far_height) = flight(3.0);
        assert!(far_time > near_time && far_height > near_height);
        let (from, to) = (Vec3::ZERO, Vec3::new(2.0, 0.0, 0.0));
        assert_eq!(boulder_position(from, to, 1.0, 0.0), from);
        assert_eq!(boulder_position(from, to, 1.0, 1.0), to);
        assert!((boulder_position(from, to, 1.0, 0.5).y - 1.0).abs() < 1e-6);
    }

    #[test]
    fn splash_hits_everything_in_the_radius() {
        let slimes = [
            (0, Vec2::new(0.1, 0.0)),
            (1, Vec2::new(0.0, 0.45)),
            (2, Vec2::new(0.6, 0.0)),
        ];
        assert_eq!(splash(Vec2::ZERO, 0.5, slimes), vec![0, 1]);
    }
}
