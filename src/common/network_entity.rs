use std::sync::Arc;

use parking_lot::RwLock;

use nalgebra::{
	Unit,
	Vector3
};

use crate::common::{
	EntityType,
	EntityPasser,
	Transform,
	OnTransformCallback,
	TransformContainer,
	entity::Entity,
	physics::PhysicsEntity
};


pub struct NetworkEntity<'a, E, T>
where
	T: PhysicsEntity,
	E: EntityPasser + ?Sized
{
	entity_passer: Arc<RwLock<E>>,
	entity_type: EntityType,
	entity: &'a mut T
}

impl<'a, E, T> NetworkEntity<'a, E, T>
where
	T: PhysicsEntity,
	E: EntityPasser + ?Sized
{
	pub fn new(
		entity_passer: Arc<RwLock<E>>,
		entity_type: EntityType,
		entity: &'a mut T
	) -> Self
	{
		Self{entity_passer, entity_type, entity}
	}

	pub fn inner(&self) -> &T
	{
		&self.entity
	}

	pub fn sync(&mut self)
	{
		let transform = self.entity.transform_clone();
		let velocity = self.entity.entity_ref().velocity;

		self.entity_passer.write().sync_entity(self.entity_type, transform, velocity);
	}
}

impl<'a, E, T> Drop for NetworkEntity<'a, E, T>
where
	T: PhysicsEntity,
	E: EntityPasser + ?Sized
{
	fn drop(&mut self)
	{
		self.sync();
	}
}

impl<'a, E, T> OnTransformCallback for NetworkEntity<'a, E, T>
where
	T: PhysicsEntity,
	E: EntityPasser
{
	fn transform_callback(&mut self, transform: Transform)
	{
		self.entity.transform_callback(transform);

		self.sync();
	}

	fn position_callback(&mut self, position: Vector3<f32>)
	{
		self.entity.position_callback(position);

		self.sync();
	}

	fn scale_callback(&mut self, scale: Vector3<f32>)
	{
		self.entity.scale_callback(scale);

		self.sync();
	}

	fn rotation_callback(&mut self, rotation: f32)
	{
		self.entity.rotation_callback(rotation);

		self.sync();
	}

	fn rotation_axis_callback(&mut self, axis: Unit<Vector3<f32>>)
	{
		self.entity.rotation_axis_callback(axis);

		self.sync();
	}
}

impl<'a, E, T> TransformContainer for NetworkEntity<'a, E, T>
where
	T: PhysicsEntity,
	E: EntityPasser
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

impl<'a, E, T> PhysicsEntity for NetworkEntity<'a, E, T>
where
	T: PhysicsEntity,
	E: EntityPasser
{
	fn entity_ref(&self) -> &Entity
	{
		self.entity.entity_ref()
	}

	fn entity_mut(&mut self) -> &mut Entity
	{
		self.entity.entity_mut()
	}

	fn physics_update(&mut self, dt: f32)
	{
		self.entity.physics_update(dt);

		self.transform_callback(self.transform_clone());
	}

	fn velocity_add(&mut self, velocity: Vector3<f32>)
	{
		self.entity.velocity_add(velocity);

		self.transform_callback(self.transform_clone());
	}
}