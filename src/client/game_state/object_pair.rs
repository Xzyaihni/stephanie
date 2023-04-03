use std::{
	sync::Arc
};

use vulkano::memory::allocator::FastMemoryAllocator;

use nalgebra::Vector3;

use crate::common::{
	PlayerGet,
	ChildContainer,
	entity::Entity,
	player::Player,
	Transform,
	OnTransformCallback,
	TransformContainer,
	physics::PhysicsEntity
};

use crate::client::{
	BuilderType,
	DrawableEntity,
	game::{
		ObjectFactory,
		object::{
			Object,
			model::Model
		}
	}
};


#[derive(Debug)]
pub struct ObjectPair<T>
{
	pub objects: Vec<Object>,
	pub entity: T
}

impl<T: PhysicsEntity + DrawableEntity + ChildContainer> ObjectPair<T>
{
	pub fn new(object_factory: &ObjectFactory, entity: T) -> Self
	{
		let mut objects = vec![Self::object_create(object_factory, &entity)];
		entity.children_ref().iter().for_each(|entity|
		{
			objects.push(Self::object_create(object_factory, entity))
		});

		Self{objects, entity}
	}

	fn object_create<E: DrawableEntity + TransformContainer>(
		object_factory: &ObjectFactory,
		entity: &E
	) -> Object
	{
		object_factory.create(
			Arc::new(Model::square(1.0)),
			entity.transform_clone(),
			entity.texture()
		)
	}

	pub fn regenerate_buffers(&mut self, allocator: &FastMemoryAllocator)
	{
		self.objects.iter_mut().for_each(|object| object.regenerate_buffers(allocator));
	}

	pub fn draw(&self, builder: BuilderType)
	{
		self.objects.iter().for_each(|object| object.draw(builder));
	}
}

impl PlayerGet for ObjectPair<Player>
{
	fn player(&self) -> Player
	{
		self.entity.clone()
	}
}

impl<T: TransformContainer + ChildContainer> OnTransformCallback for ObjectPair<T>
{
	fn callback(&mut self)
	{
		self.entity.callback();

		let mut objects = self.objects.iter_mut();

		objects.next().unwrap().set_transform(self.entity.transform_clone());

		objects.zip(self.entity.children_ref().iter())
			.for_each(|(object, child)| object.set_transform(child.transform_clone()));
	}
}

impl<T: TransformContainer + ChildContainer> TransformContainer for ObjectPair<T>
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

impl<T: PhysicsEntity + ChildContainer> PhysicsEntity for ObjectPair<T>
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

	fn velocity_add(&mut self, velocity: Vector3<f32>)
	{
		self.entity.velocity_add(velocity);
	}
}