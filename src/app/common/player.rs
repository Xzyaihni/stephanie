use serde::{Serialize, Deserialize};

use yanyaengine::{
    Transform,
    OnTransformCallback,
    TransformContainer
};

use crate::{
    entity_forward,
	common::{
        ChildEntity,
		PlayerGet,
        CharacterProperties,
        EntityProperties,
        PhysicalProperties,
		character::Character,
		entity::{
			ValueAnimation,
			ChildConnection,
			ChildDeformation,
			SpringConnection,
			StretchDeformation
		},
	}
};


pub struct PlayerProperties
{
	pub character_properties: CharacterProperties,
	pub name: String
}

impl PlayerProperties
{
    pub fn physical(&self) -> &PhysicalProperties
    {
        self.character_properties.physical()
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
        let pon_scale = player_properties.physical().transform.scale * 0.4;

		let name = player_properties.name;

		let mut player = Self{
			character: Character::new(player_properties.character_properties),
			name
		};

        let pon = {
			let entity = Entity::new(EntityProperties{
				texture: "player/pon.png".to_owned(),
                physical: PhysicalProperties{
                    mass: 0.01,
                    friction: 0.8,
                    transform: Transform{
                        scale: pon_scale,
                        ..Default::default()
                    }
                }
			});

			ChildEntity::new(
				ChildConnection::Spring(
                    SpringConnection::new(0.1, 0.4)
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
		    pon.with_parent(&player).set_origin(Vector3::new(-0.15, 0.35, 0.0));

            pon
        };

		player.add_child(top_pon);

        let bottom_pon = {
            let mut pon = pon;
		    pon.with_parent(&player).set_origin(Vector3::new(-0.15, -0.35, 0.0));

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

entity_forward!{Player, character}
