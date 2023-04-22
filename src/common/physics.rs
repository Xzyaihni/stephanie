use nalgebra::Vector3;

use crate::common::{
	Transform,
	TransformContainer,
	entity::Entity
};


pub trait PhysicsEntity: TransformContainer
{
	fn entity_ref(&self) -> &Entity;
	fn entity_mut(&mut self) -> &mut Entity;

	fn physics_update(&mut self, dt: f32);

	fn entity_clone(&self) -> Entity
	{
		self.entity_ref().clone()
	}

	fn set_entity(&mut self, transform: Transform, velocity: Vector3<f32>)
	{
		let entity = self.entity_mut();

		*entity.transform_mut() = transform;
		entity.velocity = velocity;

		self.transform_callback(self.transform_clone());
	}

	fn velocity_add(&mut self, velocity: Vector3<f32>)
	{
		self.entity_mut().velocity += velocity;
	}

	fn damp_velocity(velocity: &mut Vector3<f32>, factor: f32, dt: f32) -> Vector3<f32>
	{
		let damp = factor.powf(dt);

		*velocity *= damp;

		*velocity * (damp - 1.0) / factor.ln()
	}
}