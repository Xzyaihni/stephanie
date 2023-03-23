use serde::{Serialize, Deserialize};

use crate::common::{
	entity::{
		Entity,
		EntityProperties,
		transform::{Transform, OnTransformCallback, TransformContainer}
	},
	physics::PhysicsEntity
};


#[derive(Debug, Clone, Default)]
pub struct CharacterProperties
{
	pub entity_properties: EntityProperties
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character
{
	entity: Entity
}

impl Character
{
	pub fn new(properties: CharacterProperties) -> Self
	{
		Self{entity: Entity::new(properties.entity_properties)}
	}
}

impl OnTransformCallback for Character
{
	fn callback(&mut self)
	{
		self.entity.callback();
	}
}

impl TransformContainer for Character
{
	fn transform_ref(&self) -> &Transform
	{
		self.entity.transform_ref()
	}

	fn transform_mut(&mut self) -> &mut Transform
	{
		self.entity.transform_mut()
	}
}

impl PhysicsEntity for Character
{
	fn entity_ref(&self) -> &Entity
	{
		self.entity.entity_ref()
	}

	fn entity_mut(&mut self) -> &mut Entity
	{
		self.entity.entity_mut()
	}

	fn update(&mut self, dt: f32)
	{
		self.entity.update(dt);
	}
}