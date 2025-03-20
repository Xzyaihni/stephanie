use serde::{Serialize, Deserialize};

use crate::common::{
    render_info::*,
    EaseOut
};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LazyMix
{
    pub decay: f32,
    pub target: MixColor
}

impl LazyMix
{
    pub fn ui() -> Self
    {
        Self::ui_color([1.0; 4])
    }

    pub fn ui_color(color: [f32; 4]) -> Self
    {
        Self{
            decay: 16.0,
            target: MixColor{color, amount: 0.0, keep_transparency: false}
        }
    }

    pub fn update(
        &self,
        current: MixColor,
        dt: f32
    ) -> MixColor
    {
        let color = current.color.ease_out(self.target.color, self.decay, dt);

        MixColor{
            color,
            amount: current.amount.ease_out(self.target.amount, self.decay, dt),
            keep_transparency: current.keep_transparency
        }
    }
}
