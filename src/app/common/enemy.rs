use serde::{Serialize, Deserialize};

use yanyaengine::{
    Transform,
    OnTransformCallback,
    TransformContainer
};

use crate::{
    entity_forward,
	common::{
        EntityAny,
        EntityAnyWrappable,
        CharacterProperties,
        PhysicalProperties,
		character::Character,
	}
};


pub struct EnemyProperties
{
	pub character_properties: CharacterProperties
}

impl EnemyProperties
{
    pub fn physical(&self) -> &PhysicalProperties
    {
        self.character_properties.physical()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Enemy
{
	character: Character
}

impl Enemy
{
	pub fn new(enemy_properties: EnemyProperties) -> Self
	{
		Self{
			character: Character::new(enemy_properties.character_properties)
		}
	}

	pub fn speed(&self) -> Option<f32>
	{
		self.character.speed()
	}

    pub fn set_speed(&mut self, speed: f32)
    {
        self.character.set_speed(speed);
    }
}

impl EntityAnyWrappable for Enemy
{
    fn wrap_any(self) -> EntityAny
    {
        EntityAny::Enemy(self)
    }
}

entity_forward!{Enemy, character}
