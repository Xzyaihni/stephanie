use serde::{Serialize, Deserialize};

use crate::{
	client::DrawableEntity,
	common::{
		PlayerGet,
		ChildEntity,
		ChildContainer,
		Transform,
		OnTransformCallback,
		TransformContainer,
		entity::Entity,
		character::{Character, CharacterProperties},
		physics::PhysicsEntity
	}
};

use nalgebra::Vector3;


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

impl OnTransformCallback for Player
{
	fn callback(&mut self)
	{
		self.character.callback();
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

	fn set_rotation(&mut self, rotation: f32)
	{
		self.character.set_rotation(rotation);
	}

	fn rotate(&mut self, radians: f32)
	{
		self.character.rotate(radians);
	}
}

impl ChildContainer for Player
{
	fn children_ref(&self) -> &[ChildEntity]
	{
		self.character.children_ref()
	}

	fn children_mut(&mut self) -> &mut Vec<ChildEntity>
	{
		self.character.children_mut()
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

	fn velocity_add(&mut self, velocity: Vector3<f32>)
	{
		self.character.velocity_add(velocity);
	}
}

impl DrawableEntity for Player
{
	fn texture(&self) -> &str
	{
		self.character.texture()
	}
}