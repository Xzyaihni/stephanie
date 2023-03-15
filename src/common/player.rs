use serde::{Serialize, Deserialize};

use crate::common::{
	Transform,
	TransformContainer,
	character::Character
};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player
{
	character: Character,
	name: String
}

impl Player
{
	pub fn new(name: String) -> Self
	{
		Self{character: Character::new(), name}
	}

	pub fn name(&self) -> &str
	{
		&self.name
	}
}

impl TransformContainer for Player
{
	fn transform_ref(&self) -> &Transform
	{
		self.character.transform_ref()
	}

	fn transform_mut(&mut self) -> &mut Transform
	{
		self.character.transform_mut()
	}

	fn callback(&mut self)
	{
		self.character.callback();
	}
}