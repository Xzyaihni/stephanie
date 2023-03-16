use std::sync::Arc;

use parking_lot::RwLock;

use crate::common::{
	EntityType,
	EntityPasser,
	Transform,
	TransformContainer,
	entity::Entity,
	physics::PhysicsEntity
};


pub struct NetworkEntity<'a, E, T>
where
	E: ?Sized
{
	entity_passer: Arc<RwLock<E>>,
	entity_type: EntityType,
	entity: &'a mut T
}

impl<'a, E, T> NetworkEntity<'a, E, T>
where
	T: TransformContainer,
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

	fn callback(&mut self)
	{
		self.entity.callback();
		self.entity_passer.write().sync_entity(self.entity_type, self.entity.entity_clone());
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

	fn update(&mut self, dt: f32)
	{
		self.entity.update(dt);
		self.callback();
	}
}