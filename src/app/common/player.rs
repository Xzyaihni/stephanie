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
            ChildRotation,
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
        let transform = player_properties.physical().transform.clone();

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
                        scale: transform.scale * 0.4,
                        ..transform
                    }
                }
			});

			ChildEntity::new(
				ChildConnection::Spring(
                    SpringConnection{
                        limit: transform.scale.x * 0.1,
                        damping: 0.02,
                        strength: 0.9
                    }
                ),
                ChildRotation::Instant,
				ChildDeformation::Stretch(
					StretchDeformation{
                        animation: ValueAnimation::EaseOut(2.0),
                        limit: 0.4,
                        strength: 0.3
                    }
				),
				entity,
                Vector3::zeros(),
				1
			)
        };

        let mut add_pon = |position|
        {
            let child_pon = {
                let mut pon = pon.clone();

                let mut parented_pon = pon.with_parent(&player);

                parented_pon.set_origin(position);
                parented_pon.sync_position();

                pon
            };

            player.add_child(child_pon);
        };

        add_pon(Vector3::new(-0.15, 0.35, 0.0));
        add_pon(Vector3::new(-0.15, -0.35, 0.0));

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
