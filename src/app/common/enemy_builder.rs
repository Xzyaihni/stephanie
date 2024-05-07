use nalgebra::Vector3;

use crate::common::Entity;


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

    pub fn build(self) -> Entity
    {
        /*let props = EnemyProperties{
            character_properties: CharacterProperties{
                entity_properties: EntityProperties{
                    physical: PhysicalProperties{
                        transform: Transform{
                            position: self.pos,
                            scale: Vector3::repeat(0.1),
                            rotation: fastrand::f32() * (3.141596 * 2.0),
                            ..Default::default()
                        },
                        mass: 50.0,
                        friction: 0.5,
                        floating: false
                    }
                },
                anatomy: Anatomy::Human(HumanAnatomy::default())
            },
            behavior: EnemyBehavior::Melee
        };

        Enemy::new(props)*/
        todo!();
    }
}
