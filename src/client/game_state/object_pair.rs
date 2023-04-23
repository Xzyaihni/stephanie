use std::{
	sync::Arc
};

use parking_lot::RwLock;

use nalgebra::{
	Unit,
	Vector3
};

use crate::common::{
	PlayerGet,
	ChildContainer,
	Transform,
	OnTransformCallback,
	TransformContainer,
	physics::PhysicsEntity,
	player::Player,
	entity::{ChildEntity, Entity}
};

use crate::client::{
	GameObject,
	game_object_types::*,
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
	parent_object_id: usize,
	pub entity: T,
	models: Vec<Arc<RwLock<Model>>>
}

impl<T: PhysicsEntity + DrawableEntity + ChildContainer> ObjectPair<T>
{
	pub fn new(object_factory: &ObjectFactory, entity: T) -> Self
	{
		let mut objects = Vec::new();
		let mut children = entity.children_ref().iter();

		let parent_object_id = children.clone().position(|child| child.z_level() >= 0)
			.unwrap_or(0);

		let mut models = children.by_ref().take(parent_object_id).flat_map(|child|
		{
			let (model, object) = Self::child_object_create(object_factory, child);

			objects.push(object);

			model
		}).collect::<Vec<_>>();

		objects.push(Self::object_create(object_factory, &entity, None));

		models.extend(children.flat_map(|child|
		{
			let (model, object) = Self::child_object_create(object_factory, child);

			objects.push(object);

			model
		}));

		Self{objects, parent_object_id, entity, models}
	}

	fn object_create<E: DrawableEntity + TransformContainer>(
		object_factory: &ObjectFactory,
		entity: &E,
		unique_model: Option<Arc<RwLock<Model>>>
	) -> Object
	{
		let model = unique_model.unwrap_or_else(|| object_factory.default_model());

		object_factory.create(
			model,
			entity.transform_clone(),
			entity.texture()
		)
	}

	fn child_object_create(
		object_factory: &ObjectFactory,
		entity: &ChildEntity
	) -> (Option<Arc<RwLock<Model>>>, Object)
	{
		let unique_model = entity.unique_model();
		let mut object = Self::object_create(
			object_factory,
			entity.entity_ref(),
			unique_model.clone()
		);

		object.set_origin(entity.origin());

		(unique_model, object)
	}
}

impl PlayerGet for ObjectPair<Player>
{
	fn player(&self) -> Player
	{
		self.entity.clone()
	}
}

impl<T: PhysicsEntity + ChildContainer> GameObject for ObjectPair<T>
{
	fn update(&mut self, dt: f32)
	{
		self.physics_update(dt);

		let transform = self.transform_clone();
		self.entity.children_ref().iter()
			.filter(|child| child.unique_model().is_some())
			.zip(self.models.iter_mut())
			.for_each(|(child, model)|
			{
				child.modify_model(&mut model.write(), &transform, dt)
			});
	}

	fn draw(&self, allocator: AllocatorType, builder: BuilderType, layout: LayoutType)
	{
		self.objects.iter().for_each(|object| object.draw(allocator, builder, layout.clone()));
	}
}

impl<T: TransformContainer + ChildContainer> OnTransformCallback for ObjectPair<T>
{
	fn transform_callback(&mut self, transform: Transform)
	{
		self.objects[self.parent_object_id].set_transform(transform);

		self.objects.iter_mut().enumerate().filter(|(index, _)| *index != self.parent_object_id)
			.zip(self.entity.children_ref().iter()).for_each(|((_, object), child)|
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