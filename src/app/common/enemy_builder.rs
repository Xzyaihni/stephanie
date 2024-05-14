use nalgebra::Vector3;

use yanyaengine::Transform;

use crate::common::{
    Anatomy,
    HumanAnatomy,
    ServerEnemy,
    EnemyId,
    EnemiesInfo,
    PhysicalProperties,
    RenderInfo,
    EntityInfo,
    lazy_transform::*
};


pub struct EnemyBuilder<'a>
{
    enemies_info: &'a EnemiesInfo,
    pos: Vector3<f32>,
    id: EnemyId
}

impl<'a> EnemyBuilder<'a>
{
    pub fn new(
        enemies_info: &'a EnemiesInfo,
        id: EnemyId,
        pos: Vector3<f32>
    ) -> Self
    {
        Self{enemies_info, pos, id}
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
            render: Some(RenderInfo{texture: None, z_level: -1}),
            physical: Some(PhysicalProperties{
                mass: 50.0,
                friction: 0.5,
                floating: false
            }.into()),
            anatomy: Some(Anatomy::Human(HumanAnatomy::default())),
            enemy: Some(ServerEnemy::new(self.enemies_info, self.id)),
            ..Default::default()
        }
    }
}
