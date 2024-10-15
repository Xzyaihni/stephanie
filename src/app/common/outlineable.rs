use serde::{Serialize, Deserialize};

use crate::common::EaseOut;


#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Outlineable
{
    current: f32,
    target: f32
}

impl Default for Outlineable
{
    fn default() -> Self
    {
        Self{current: 0.0, target: 0.0}
    }
}

impl Outlineable
{
    pub fn enable(&mut self)
    {
        self.target = 1.0;
    }

    pub fn disable(&mut self)
    {
        self.target = 0.0;
    }

    pub fn current(&self) -> Option<f32>
    {
        (self.current > 0.0).then_some(self.current)
    }

    pub fn update(&mut self, dt: f32)
    {
        self.current = self.current.ease_out(self.target, 10.0, dt);
    }
}
