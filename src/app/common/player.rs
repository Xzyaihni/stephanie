use serde::{Serialize, Deserialize};

use yanyaengine::{
    Transform,
    OnTransformCallback,
    TransformContainer
};

use crate::{
	client::DrawableEntity,
	common::{
		PlayerGet,
		ChildEntity,
		ChildContainer,
		character::{Character, CharacterProperties},
		physics::PhysicsEntity,
		entity::{
			ValueAnimation,
			ChildConnection,
			ChildDeformation,
			SpringConnection,
			StretchDeformation,
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

		let transform = Transform{
		    scale: Vector3::repeat(0.1),
            ..Default::default()
        };

		let texture = "player/hair.png".to_owned();

		let name = String::new();

        // does clippy want me to update everything if i add another value to a struct?
        #[allow(clippy::needless_update)]
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

		let mut player = Self{
			character: Character::new(player_properties.character_properties),
			name
		};

        let pon = {
			let entity = Entity::new(EntityProperties{
				texture: "player/pon.png".to_owned(),
                transform: Transform{
                    scale: Vector3::repeat(0.4),
                    ..Default::default()
                },
				..Default::default()
			});

			ChildEntity::new(
				ChildConnection::Spring(
                    SpringConnection::new(0.1, 0.02, 0.2)
                ),
				ChildDeformation::Stretch(
					StretchDeformation::new(ValueAnimation::EaseOut(2.0), 0.4, 0.1)
				),
				entity,
				1
			)
        };

        let top_pon = {
            let mut pon = pon.clone();
		    pon.set_origin(&player, Vector3::new(-0.15, 0.35, 0.0));

            pon
        };

		player.add_child(top_pon);

        let bottom_pon = {
            let mut pon = pon;
		    pon.set_origin(&player, Vector3::new(-0.15, -0.35, 0.0));

            pon
        };

		player.add_child(bottom_pon);

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
	fn children_ref(&self) -> &[ChildEntity]
	{
		self.character.children_ref()
	}

	fn children_mut(&mut self) -> &mut Vec<ChildEntity>
	{
		self.character.children_mut()
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
