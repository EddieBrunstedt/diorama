//! Small in-house animation helpers: tweens with easing, for things popping and dropping into place.

use bevy::prelude::*;

pub struct JuicePlugin;

impl Plugin for JuicePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, run_tweens);
    }
}

#[derive(Clone, Copy, Debug)]
pub enum TweenTarget {
    Translation { from: Vec3, to: Vec3 },
    Scale { from: Vec3, to: Vec3 },
}

/// Animates a transform property once, then removes itself. The entity stays hidden until the
/// tween starts, so spawn it with `Visibility::Hidden` and [`Tween::start_transform`].
#[derive(Component, Clone, Debug)]
pub struct Tween {
    pub target: TweenTarget,
    pub ease: EaseFunction,
    pub delay: f32,
    pub duration: f32,
    elapsed: f32,
}

impl Tween {
    pub fn new(target: TweenTarget, ease: EaseFunction, delay: f32, duration: f32) -> Self {
        Self {
            target,
            ease,
            delay,
            duration,
            elapsed: 0.0,
        }
    }

    /// Falls from `height` above `rest` and bounces into place.
    pub fn drop_in(rest: Vec3, height: f32, delay: f32) -> Self {
        let from = rest + Vec3::Y * height;
        Self::new(
            TweenTarget::Translation { from, to: rest },
            EaseFunction::BounceOut,
            delay,
            0.75,
        )
    }

    /// Grows from nothing to `scale` with a little overshoot.
    pub fn pop_in(scale: Vec3, delay: f32) -> Self {
        // Not exactly zero: a zero scale makes the transform non-invertible.
        let from = Vec3::splat(0.01);
        Self::new(
            TweenTarget::Scale { from, to: scale },
            EaseFunction::BackOut,
            delay,
            0.4,
        )
    }

    /// Seconds until the tween has finished.
    pub fn remaining(&self) -> f32 {
        (self.delay + self.duration - self.elapsed).max(0.0)
    }

    /// The transform the entity should be spawned with (the tween's first frame).
    pub fn start_transform(&self) -> Transform {
        match self.target {
            TweenTarget::Translation { from, .. } => Transform::from_translation(from),
            TweenTarget::Scale { from, .. } => Transform::from_scale(from),
        }
    }
}

fn run_tweens(
    mut commands: Commands,
    time: Res<Time>,
    mut tweens: Query<(Entity, &mut Tween, &mut Transform, &mut Visibility)>,
) {
    for (entity, mut tween, mut transform, mut visibility) in &mut tweens {
        tween.elapsed += time.delta_secs();
        let t = (tween.elapsed - tween.delay) / tween.duration;
        if t < 0.0 {
            continue;
        }
        visibility.set_if_neq(Visibility::Inherited);
        let k = tween.ease.sample_clamped(t);
        match tween.target {
            TweenTarget::Translation { from, to } => transform.translation = from.lerp(to, k),
            TweenTarget::Scale { from, to } => transform.scale = from.lerp(to, k),
        }
        if t >= 1.0 {
            commands.entity(entity).remove::<Tween>();
        }
    }
}
