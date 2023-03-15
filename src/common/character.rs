use serde::{Serialize, Deserialize};

use crate::common::{
	entity::{
		Entity,
		transform::{Transform, TransformContainer}
	}
};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character
{
	entity: Entity
}

impl Character
{
	pub fn new() -> Self
	{
		Self{entity: Entity::new()}
	}
}

impl TransformContainer for Character
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
	}
}