use nalgebra::Vector3;

use yanyaengine::Transform;

use crate::common::{PhysicalProperties, RenderInfo, EntityInfo};


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
            transform: Some(Transform{
                position: self.pos,
                scale: Vector3::repeat(0.1),
                rotation: fastrand::f32() * (3.141596 * 2.0),
                ..Default::default()
            }),
            render: Some(RenderInfo{texture: "enemy/body.png".to_owned()}),
            physical: Some(PhysicalProperties{
                mass: 50.0,
                friction: 0.5,
                floating: false
            }.into()),
            ..Default::default()
        }
    }
}
