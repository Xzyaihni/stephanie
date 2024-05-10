use nalgebra::Vector3;

use crate::common::EntityInfo;


pub struct EnemyBuilder
{
    pos: Vector3<f32>
}

impl EnemyBuilder
{
    pub fn new(pos: Vector3<f32>) -> Self
    {
        Self{pos}
    }

    pub fn build(self) -> EntityInfo
    {
        EntityInfo{
            ..Default::default()
        }
    }
}
