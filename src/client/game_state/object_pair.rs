use std::{
	sync::Arc
};

use vulkano::memory::allocator::StandardMemoryAllocator;

use nalgebra::{
	Unit,
	Vector3
};

use crate::common::{
	PlayerGet,
	ChildContainer,
	entity::{ChildEntity, Entity},
	player::Player,
	Transform,
	OnTransformCallback,
	TransformContainer,
	physics::PhysicsEntity
};

use crate::client::{
	GameObject,
	BuilderType,
	LayoutType,
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
		entity.children_ref().iter().for_each(|child|
		{
			objects.push(Self::child_object_create(object_factory, &child))
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

	fn child_object_create(object_factory: &ObjectFactory, entity: &ChildEntity) -> Object
	{
		let mut object = Self::object_create(object_factory, entity.entity_ref());

		object.set_origin(entity.origin());

		object
	}
}

impl<T: PhysicsEntity + ChildContainer> GameObject for ObjectPair<T>
{
	fn update(&mut self, dt: f32)
	{
		self.physics_update(dt);
	}

	fn regenerate_buffers(&mut self, allocator: &StandardMemoryAllocator)
	{
		self.objects.iter_mut().for_each(|object| object.regenerate_buffers(allocator));
	}

	fn draw(&self, builder: BuilderType, layout: LayoutType)
	{
		self.objects.iter().for_each(|object| object.draw(builder, layout.clone()));
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
	fn transform_callback(&mut self, transform: Transform)
	{
		let mut objects = self.objects.iter_mut();

		objects.next().unwrap().set_transform(transform);

		objects.zip(self.entity.children_ref().iter())
			.for_each(|(object, child)|
			{
				object.set_transform(child.entity_ref().transform_clone())
			});
	}

	fn position_callback(&mut self, position: Vector3<f32>)
	{
		self.entity.position_callback(position);

		self.transform_callback(self.transform_clone());
	}

	fn scale_callback(&mut self, scale: Vector3<f32>)
	{
		self.entity.scale_callback(scale);

		self.transform_callback(self.transform_clone());
	}

	fn rotation_callback(&mut self, rotation: f32)
	{
		self.entity.rotation_callback(rotation);

		self.transform_callback(self.transform_clone());
	}

	fn rotation_axis_callback(&mut self, axis: Unit<Vector3<f32>>)
	{
		self.entity.rotation_axis_callback(axis);

		self.transform_callback(self.transform_clone());
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

	fn physics_update(&mut self, dt: f32)
	{
		self.entity.physics_update(dt);

		self.transform_callback(self.transform_clone());
	}

	fn velocity_add(&mut self, velocity: Vector3<f32>)
	{
		self.entity.velocity_add(velocity);
	}
}