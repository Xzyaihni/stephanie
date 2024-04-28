use nalgebra::Vector3;

use yanyaengine::Transform;

use crate::common::{
    Enemy,
    EnemyProperties,
    enemy::EnemyBehavior,
    CharacterProperties,
    Anatomy,
    HumanAnatomy,
    EntityProperties,
    PhysicalProperties
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

    pub fn build(self) -> Enemy
    {
        let props = EnemyProperties{
            character_properties: CharacterProperties{
                entity_properties: EntityProperties{
                    texture: Some("enemy/body.png".to_owned()),
                    physical: PhysicalProperties{
                        transform: Transform{
                            position: self.pos,
                            scale: Vector3::repeat(0.1),
                            rotation: fastrand::f32() * (3.141596 * 2.0),
                            ..Default::default()
                        },
                        mass: 50.0,
                        friction: 0.5
                    }
                },
                anatomy: Anatomy::Human(HumanAnatomy::default())
            },
            behavior: EnemyBehavior::Melee
        };

        Enemy::new(props)
    }
}
