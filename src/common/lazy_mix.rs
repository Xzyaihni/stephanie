use serde::{Serialize, Deserialize};

use crate::common::{
    render_info::*,
    animation_common::*
};

pub use crate::common::animation_common::ValueAnimation;


pub struct LazyMixInfo
{
    pub lifetime: f32,
    pub animation: ValueAnimation,
    pub target: MixColor
}

impl Default for LazyMixInfo
{
    fn default() -> Self
    {
        Self{
            lifetime: 1.0,
            animation: ValueAnimation::Linear,
            target: MixColor::color([1.0; 4])
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LazyMix
{
    color_lifetime: TimedInterpolation<[f32; 4]>,
    color_animation: ValueAnimation,
    amount_lifetime: TimedInterpolation<f32>,
    amount_animation: ValueAnimation,
    pub target: MixColor
}

impl From<LazyMixInfo> for LazyMix
{
    fn from(info: LazyMixInfo) -> Self
    {
        Self{
            color_lifetime: info.lifetime.into(),
            color_animation: info.animation,
            amount_lifetime: info.lifetime.into(),
            amount_animation: info.animation,
            target: info.target
        }
    }
}

impl LazyMix
{
    pub fn update(
        &mut self,
        current: MixColor,
        dt: f32
    ) -> MixColor
    {
        let mut color = current.color;
        timed_interpolate(&mut color, self.target.color, &mut self.color_lifetime, self.color_animation, dt);

        let mut amount = current.amount;
        timed_interpolate(&mut amount, self.target.amount, &mut self.amount_lifetime, self.amount_animation, dt);

        MixColor{
            color,
            amount,
            only_alpha: current.only_alpha,
            keep_transparency: current.keep_transparency,
            palette: current.palette
        }
    }
}
