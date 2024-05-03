use std::f32;

use serde::{Serialize, Deserialize};

use crate::{
    entity_forward,
    forward_damageable,
	common::{
        EntityAny,
        EntityAnyWrappable,
        ChildEntity,
        CharacterProperties,
        EntityProperties,
        PhysicalProperties,
		character::Character,
		entity::{
            child_entity::*,
			ValueAnimation,
			SpringConnection,
            EaseOutRotation,
            ConstantRotation,
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

        let character_properties = player_properties.character_properties;
        let character_properties = CharacterProperties{
            entity_properties: EntityProperties{
                texture: Some(character_properties.main_texture.clone()),
                ..character_properties.entity_properties
            },
            ..character_properties
        };

		let mut player = Self{
			character: Character::new(character_properties),
			name
		};

        let mut current_z = 1;
        let mut add_pon = |position|
        {
			let entity = Entity::new(EntityProperties{
				texture: Some("player/pon.png".to_owned()),
                physical: PhysicalProperties{
                    mass: 0.01,
                    friction: 0.8,
                    floating: true,
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
                        resistance: 0.0001,
                        momentum: 0.5
                    }.into()
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
				current_z
			);

            current_z += 1;

            player.add_child(position, pon);
        };

        add_pon(Vector3::new(-0.15, 0.35, 0.0));
        add_pon(Vector3::new(-0.15, -0.35, 0.0));

        let item_size = 0.2;
        let held_item = {
			let entity = Entity::new(EntityProperties{
				texture: Some("items/weapons/pistol.png".to_owned()),
                physical: PhysicalProperties{
                    mass: 0.5,
                    friction: 0.4,
                    floating: true,
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
                ChildRotation::Constant(
                    ConstantRotation{
                        speed: 5.0,
                        momentum: 0.5
                    }.into()
                ),
				ChildDeformation::Rigid,
				entity,
				-1
			)
        };

        player.add_child(Vector3::new(1.0, 0.0, 0.0), held_item);

		player
	}

    pub fn move_speed(&self) -> Option<f32>
    {
        self.character.move_speed()
    }

	pub fn speed(&self) -> Option<f32>
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

forward_damageable!{Player, character}
entity_forward!{Player, character}
