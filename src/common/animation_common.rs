use crate::common::{
    Lerpable,
    watcher::Lifetime
};

use serde::{Serialize, Deserialize};


#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ValueAnimation
{
    Linear,
    EaseIn(f32),
    EaseOut(f32)
}

impl ValueAnimation
{
    pub fn apply(&self, value: f32) -> f32
    {
        let value = value.clamp(0.0, 1.0);

        match self
        {
            Self::Linear => value,
            Self::EaseIn(strength) => value.powf(*strength),
            Self::EaseOut(strength) => 1.0 - (1.0 - value).powf(*strength)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimedInterpolation<T>
{
    lifetime: Lifetime,
    start: Option<T>
}

impl<T> From<f32> for TimedInterpolation<T>
{
    fn from(lifetime: f32) -> Self
    {
        Self::from(Lifetime::from(lifetime))
    }
}

impl<T> From<Lifetime> for TimedInterpolation<T>
{
    fn from(lifetime: Lifetime) -> Self
    {
        Self{lifetime, start: None}
    }
}

pub fn timed_interpolate<T: Lerpable + Clone>(
    current: &mut T,
    target: T,
    TimedInterpolation{lifetime, ref mut start}: &mut TimedInterpolation<T>,
    animation: ValueAnimation,
    dt: f32
)
{
    if start.is_none()
    {
        *start = Some(current.clone());
    }

    let remaining = 1.0 - lifetime.fraction();

    *current = start.as_ref().unwrap().lerp(&target, animation.apply(remaining));

    lifetime.current -= dt;
}
