use std::f32;

use nalgebra::Vector3;

use yanyaengine::Transform;

use crate::common::{
    render_info::*,
    Anatomy,
    HumanAnatomy,
    Enemy,
    EnemyId,
    EnemiesInfo,
    PhysicalProperties,
    Collider,
    ColliderType,
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
        let info = self.enemies_info.get(self.id);

        EntityInfo{
            lazy_transform: Some(LazyTransformInfo{
                rotation: Rotation::EaseOut(
                    EaseOutRotation{
                        decay: 6.0,
                        momentum: 0.0
                    }.into()
                ),
                transform: Transform{
                    position: self.pos,
                    scale: Vector3::repeat(info.scale),
                    rotation: fastrand::f32() * (f32::consts::PI * 2.0),
                    ..Default::default()
                },
                ..Default::default()
            }.into()),
            render: Some(RenderInfo{
                shape: Some(BoundingShape::Circle),
                z_level: ZLevel::High,
                ..Default::default()
            }),
            collider: Some(Collider{
                kind: ColliderType::Circle,
                ..Default::default()
            }),
            physical: Some(PhysicalProperties{
                mass: 50.0,
                friction: 0.5,
                floating: false
            }.into()),
            anatomy: Some(Anatomy::Human(HumanAnatomy::default())),
            enemy: Some(Enemy::new(self.enemies_info, self.id)),
            ..Default::default()
        }
    }
}
