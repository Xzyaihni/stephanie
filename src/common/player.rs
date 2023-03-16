use serde::{Serialize, Deserialize};

use crate::common::{
	PlayerGet,
	Transform,
	TransformContainer,
	entity::Entity,
	character::{Character, CharacterProperties},
	physics::PhysicsEntity
};


#[derive(Debug, Clone, Default)]
pub struct PlayerProperties
{
	pub character_properties: CharacterProperties,
	pub name: String
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player
{
	character: Character,
	name: String
}

impl Player
{
	pub fn new(player_properties: PlayerProperties) -> Self
	{
		let name = player_properties.name;

		Self{character: Character::new(player_properties.character_properties), name}
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

impl PlayerGet for Player
{
	fn player(&self) -> Player
	{
		self.clone()
	}
}

impl PhysicsEntity for Player
{
	fn entity_ref(&self) -> &Entity
	{
		self.character.entity_ref()
	}

	fn entity_mut(&mut self) -> &mut Entity
	{
		self.character.entity_mut()
	}

	fn update(&mut self, dt: f32)
	{
		self.character.update(dt);
	}
}