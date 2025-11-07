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
            only_alpha: current.only_alpha,
            keep_transparency: current.keep_transparency
        }
    }
}
