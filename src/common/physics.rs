use nalgebra::Vector3;

use crate::common::{
	TransformContainer,
	entity::Entity
};


pub trait PhysicsEntity: TransformContainer
{
	fn entity_ref(&self) -> &Entity;
	fn entity_mut(&mut self) -> &mut Entity;

	fn update(&mut self, dt: f32);

	fn entity_clone(&self) -> Entity
	{
		self.entity_ref().clone()
	}

	fn set_entity(&mut self, entity: Entity)
	{
		*self.entity_mut() = entity;
		self.callback();
	}

	fn velocity_add(&mut self, velocity: Vector3<f32>)
	{
		self.entity_mut().velocity += velocity;
		self.callback();
	}
}