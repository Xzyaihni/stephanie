use std::{
	sync::Arc
};

use vulkano::memory::allocator::FastMemoryAllocator;

use crate::common::{
	PlayerGet,
	entity::Entity,
	player::Player,
	Transform,
	OnTransformCallback,
	TransformContainer,
	physics::PhysicsEntity
};

use crate::client::game::{
	ObjectFactory,
	object::{
		Object,
		model::Model
	}
};


#[derive(Debug)]
pub struct ObjectPair<T>
{
	pub object: Object,
	pub entity: T
}

impl<T: PhysicsEntity> ObjectPair<T>
{
	pub fn new(object_factory: &ObjectFactory, entity: T) -> Self
	{
		let object = object_factory.create(
			Arc::new(Model::square(1.0)),
			entity.transform_clone(),
			0
		);

		Self{object, entity}
	}

	pub fn regenerate_buffers(&mut self, allocator: &FastMemoryAllocator)
	{
		self.object.regenerate_buffers(allocator);
	}
}

impl PlayerGet for ObjectPair<Player>
{
	fn player(&self) -> Player
	{
		self.entity.clone()
	}
}

impl<T: TransformContainer> OnTransformCallback for ObjectPair<T>
{
	fn callback(&mut self)
	{
		self.entity.callback();
		self.object.set_transform(self.entity.transform_clone());
	}
}

impl<T: TransformContainer> TransformContainer for ObjectPair<T>
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

impl<T: PhysicsEntity> PhysicsEntity for ObjectPair<T>
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