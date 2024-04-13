use std::sync::Arc;

use parking_lot::RwLock;

use nalgebra::{
	Unit,
	Vector3
};

use yanyaengine::{
	Transform,
	OnTransformCallback,
	TransformContainer
};

use crate::common::{
    ChildEntity,
	EntityType,
	EntityPasser,
    Physical,
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
		self.entity
	}

	pub fn inner_mut(&mut self) -> &mut T
	{
		self.entity
	}

    pub fn add_child(&mut self, position: Vector3<f32>, child: ChildEntity)
    {
    }

	pub fn sync(&mut self)
	{
		let transform = self.entity.transform_clone();

		self.entity_passer.write().sync_transform(self.entity_type, transform);
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
	fn physical_ref(&self) -> &Physical
    {
        self.entity.physical_ref()
    }

	fn physical_mut(&mut self) -> &mut Physical
    {
        self.entity.physical_mut()
    }

	fn physics_update(&mut self, dt: f32)
	{
		self.entity.physics_update(dt);

		self.transform_callback(self.transform_clone());
	}
}
