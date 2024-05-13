use nalgebra::Vector3;

use yanyaengine::Transform;

use crate::common::{
    Anatomy,
    HumanAnatomy,
    EnemyProperties,
    EnemyBehavior,
    PhysicalProperties,
    RenderInfo,
    EntityInfo,
    lazy_transform::*
};


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
            transform: Some(Default::default()),
            lazy_transform: Some(LazyTransformInfo{
                connection: Connection::Rigid,
                deformation: Deformation::Rigid,
                rotation: Rotation::EaseOut(
                    EaseOutRotation{
                        resistance: 0.01,
                        momentum: 0.0
                    }.into()
                ),
                origin_rotation: 0.0,
                origin: Vector3::zeros(),
                transform: Transform{
                    position: self.pos,
                    scale: Vector3::repeat(0.1),
                    rotation: fastrand::f32() * (3.141596 * 2.0),
                    ..Default::default()
                }
            }.into()),
            render: Some(RenderInfo{texture: "enemy/body.png".to_owned(), z_level: -1}),
            physical: Some(PhysicalProperties{
                mass: 50.0,
                friction: 0.5,
                floating: false
            }.into()),
            anatomy: Some(Anatomy::Human(HumanAnatomy::default())),
            enemy: Some(EnemyProperties{
                behavior: EnemyBehavior::Melee
            }.into()),
            ..Default::default()
        }
    }
}
