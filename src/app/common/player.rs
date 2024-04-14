use std::f32;

use serde::{Serialize, Deserialize};

use yanyaengine::{
    Transform,
    OnTransformCallback,
    TransformContainer
};

use crate::{
    entity_forward,
	common::{
        EntityAny,
        EntityAnyWrappable,
        ChildEntity,
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
            EaseOutRotation,
			StretchDeformation
		}
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

        let mut add_pon = |position|
        {
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

			let pon = ChildEntity::new(
				ChildConnection::Spring(
                    SpringConnection{
                        limit: transform.scale.x * 0.1,
                        damping: 0.02,
                        strength: 0.9
                    }
                ),
                ChildRotation::EaseOut(
                    EaseOutRotation{
                        strength: 0.0001
                    }
                ),
				ChildDeformation::Stretch(
					StretchDeformation{
                        animation: ValueAnimation::EaseOut(2.0),
                        limit: 0.4,
                        onset: 0.3,
                        strength: 0.5
                    }
				),
				entity,
				1
			);

            player.add_child(position, pon);
        };

        add_pon(Vector3::new(-0.15, 0.35, 0.0));
        add_pon(Vector3::new(-0.15, -0.35, 0.0));

        let item_size = 0.2;
        let held_item = {
			let entity = Entity::new(EntityProperties{
				texture: "items/weapons/pistol.png".to_owned(),
                physical: PhysicalProperties{
                    mass: 0.5,
                    friction: 0.4,
                    transform: Transform{
                        scale: transform.scale.component_mul(&Vector3::new(
                            item_size,
                            item_size * 4.143,
                            item_size)),
                        rotation: f32::consts::FRAC_PI_2,
                        ..transform
                    }
                }
			});

			ChildEntity::new(
				ChildConnection::Spring(
                    SpringConnection{
                        limit: transform.scale.x * 0.1,
                        damping: 0.02,
                        strength: 6.0
                    }
                ),
                ChildRotation::Constant{speed: 5.0},
				ChildDeformation::Rigid,
				entity,
				-1
			)
        };

        player.add_child(Vector3::new(1.0, 0.0, 0.0), held_item);

		player
	}

	pub fn speed(&self) -> f32
	{
		self.character.speed()
	}

    pub fn set_speed(&mut self, speed: f32)
    {
        self.character.set_speed(speed);
    }

	pub fn name(&self) -> &str
	{
		&self.name
	}
}

impl EntityAnyWrappable for Player
{
    fn wrap_any(self) -> EntityAny
    {
        EntityAny::Player(self)
    }
}

entity_forward!{Player, character}
