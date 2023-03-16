use crate::common::{
	PlayerGet,
	entity::Entity,
	player::Player,
	Transform,
	TransformContainer,
	physics::PhysicsEntity
};

use crate::client::game::{
	ObjectFactory,
	object::Object
};


#[derive(Debug)]
pub struct ObjectPair<T>
{
	pub object: Object,
	pub entity: T
}

impl<T: TransformContainer> ObjectPair<T>
{
	pub fn new(object_factory: &ObjectFactory, entity: T) -> Self
	{
		let object = object_factory.create(2);

		Self{object, entity: entity}
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

	fn callback(&mut self)
	{
		self.entity.callback();
		self.object.set_transform(self.entity.transform_clone());
	}
}

impl PlayerGet for ObjectPair<Player>
{
	fn player(&self) -> Player
	{
		self.entity.clone()
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