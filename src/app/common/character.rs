use serde::{Serialize, Deserialize};

use yanyaengine::{Transform, OnTransformCallback, TransformContainer};

use crate::{
    entity_forward,
	common::{PhysicalProperties, entity::EntityProperties}
};


pub struct CharacterProperties
{
	pub entity_properties: EntityProperties,
	pub speed: f32
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
	speed: f32
}

impl Character
{
	pub fn new(properties: CharacterProperties) -> Self
	{
		let speed = properties.speed;

		Self{entity: Entity::new(properties.entity_properties), speed}
	}

	pub fn speed(&self) -> f32
	{
		self.speed
	}
}

entity_forward!{Character, entity}
