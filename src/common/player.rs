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
		character::{Character, CharacterProperties},
		physics::PhysicsEntity,
		entity::{
			ChildConnection,
			SpringConnection,
			ChildDeformation,
			StretchDeformation,
			OffsetStretchDeformation,
			EntityProperties,
			Entity
		},
	}
};

use nalgebra::{
	Unit,
	Vector3
};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerProperties
{
	pub character_properties: CharacterProperties,
	pub name: String
}

impl Default for PlayerProperties
{
	fn default() -> Self
	{
		let damp_factor = 0.001;

		let mut transform = Transform::new();
		transform.scale = Vector3::new(0.1, 0.1, 0.1);

		let texture = "player/hair.png".to_owned();

		let name = String::new();

		Self{
			character_properties: CharacterProperties{
				entity_properties: EntityProperties{
					damp_factor,
					transform,
					texture,
					..Default::default()
				},
				..Default::default()
			},
			name
		}
	}
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

		let create_pon = |texture: &str|
		{
			let mut transform = Transform::new();

			let pon_scale = 0.4;
			transform.scale = Vector3::new(pon_scale, pon_scale, 1.0);

			let entity = Entity::new(EntityProperties{
				texture: texture.to_owned(),
				transform,
				..Default::default()
			});

			ChildEntity::new(
				ChildConnection::Spring(SpringConnection::new(0.2, 0.1, 0.25)),
				ChildDeformation::Stretch(StretchDeformation::new(1.25, 0.2)),
				entity
			)
		};

		let mut player = Self{
			character: Character::new(player_properties.character_properties),
			name
		};

		let x_offset = -0.1;
		let y_offset = 0.25;

		let mut left_pon = create_pon("player/left_pon.png");
		left_pon.set_origin(&player, Vector3::new(x_offset, y_offset, 0.0));

		let mut right_pon = create_pon("player/right_pon.png");
		right_pon.set_origin(&player, Vector3::new(x_offset, -y_offset, 0.0));

		let mut back_hair =
		{
			let entity = Entity::new(EntityProperties{
				texture: "player/back_hair.png".to_owned(),
				..Default::default()
			});

			ChildEntity::new(
				ChildConnection::Rigid,
				ChildDeformation::OffsetStretch(OffsetStretchDeformation::new(1.0, 0.2, 0.001)),
				entity
			)
		};
		back_hair.set_origin(&player, Vector3::new(-0.7, 0.0, 0.0));

		player.add_under_child(back_hair);

		player.add_child(left_pon);
		player.add_child(right_pon);

		player
	}

	pub fn speed(&self) -> f32
	{
		self.character.speed()
	}

	pub fn name(&self) -> &str
	{
		&self.name
	}
}

impl PlayerGet for Player
{
	fn player(&self) -> Player
	{
		self.clone()
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
	fn under_children_ref(&self) -> &[ChildEntity]
	{
		self.character.under_children_ref()
	}

	fn under_children_mut(&mut self) -> &mut Vec<ChildEntity>
	{
		self.character.under_children_mut()
	}

	fn over_children_ref(&self) -> &[ChildEntity]
	{
		self.character.over_children_ref()
	}

	fn over_children_mut(&mut self) -> &mut Vec<ChildEntity>
	{
		self.character.over_children_mut()
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