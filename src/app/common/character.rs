use serde::{Serialize, Deserialize};

use yanyaengine::{Transform, OnTransformCallback, TransformContainer};

use crate::{
    entity_forward,
	common::{Anatomy, PhysicalProperties, entity::EntityProperties}
};


pub struct CharacterProperties
{
	pub entity_properties: EntityProperties,
	pub anatomy: Anatomy
}

impl CharacterProperties
{
    pub fn physical(&self) -> &PhysicalProperties
    {
        self.entity_properties.physical()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character
{
	entity: Entity,
    anatomy: Anatomy
}

impl Character
{
	pub fn new(properties: CharacterProperties) -> Self
	{
		let anatomy = properties.anatomy;

		Self{entity: Entity::new(properties.entity_properties), anatomy}
	}

	pub fn speed(&self) -> Option<f32>
	{
		self.anatomy.speed()
	}

    pub fn set_speed(&mut self, speed: f32)
    {
        self.anatomy.set_speed(speed);
    }
}

entity_forward!{Character, entity}
