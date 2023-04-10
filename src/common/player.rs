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

use nalgebra::{
	Unit,
	Vector3
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

impl OnTransformCallback for Player
{
	fn transform_callback(&mut self, transform: Transform)
	{
		self.character.transform_callback(transform);
	}

	fn position_callback(&mut self, position: Vector3<f32>)
	{
		self.character.position_callback(position);
	}

	fn scale_callback(&mut self, scale: Vector3<f32>)
	{
		self.character.scale_callback(scale);
	}

	fn rotation_callback(&mut self, rotation: f32)
	{
		self.character.rotation_callback(rotation);
	}

	fn rotation_axis_callback(&mut self, axis: Unit<Vector3<f32>>)
	{
		self.character.rotation_axis_callback(axis);
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

	fn physics_update(&mut self, dt: f32)
	{
		self.character.physics_update(dt);
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