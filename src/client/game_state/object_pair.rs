use crate::common::{
	PlayerGet,
	player::Player,
	Transform,
	TransformContainer
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
		let object = object_factory.create(1);

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