use serde::{Serialize, Deserialize};

use crate::{
	client::DrawableEntity,
	common::{
		ChildEntity,
		ChildContainer,
		entity::{
			Entity,
			EntityProperties,
			transform::{Transform, OnTransformCallback, TransformContainer}
		},
		physics::PhysicsEntity
	}
};

use nalgebra::Vector3;


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

	fn set_rotation(&mut self, rotation: f32)
	{
		self.entity.set_rotation(rotation);
	}

	fn rotate(&mut self, radians: f32)
	{
		self.entity.rotate(radians);
	}
}

impl ChildContainer for Character
{
	fn children_ref(&self) -> &[ChildEntity]
	{
		self.entity.children_ref()
	}

	fn children_mut(&mut self) -> &mut Vec<ChildEntity>
	{
		self.entity.children_mut()
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

	fn velocity_add(&mut self, velocity: Vector3<f32>)
	{
		self.entity.velocity_add(velocity);
	}
}

impl DrawableEntity for Character
{
	fn texture(&self) -> &str
	{
		self.entity.texture()
	}
}