use serde::{Serialize, Deserialize};

use crate::common::{
    ease_out,
    render_info::*
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
        Self{
            decay: 16.0,
            target: MixColor{color: [1.0; 3], amount: 0.0}
        }
    }

    pub fn update(
        &self,
        current: MixColor,
        dt: f32
    ) -> MixColor
    {
        let color = current.color.into_iter().zip(self.target.color)
            .map(|(current, target)| ease_out(current, target, self.decay, dt))
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();

        MixColor{
            color,
            amount: ease_out(current.amount, self.target.amount, self.decay, dt)
        }
    }
}
