use serde::{Serialize, Deserialize};

use crate::{
    basic_entity_forward,
    client::DrawableEntity,
	common::{Anatomy, PhysicalProperties, entity::EntityProperties}
};


pub struct CharacterProperties
{
	pub entity_properties: EntityProperties,
	pub anatomy: Anatomy
}

impl CharacterProperties
{
    pub fn physical(&self) -> &PhysicalProperties
    {
        self.entity_properties.physical()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character
{
	entity: Entity,
    anatomy: Anatomy,
    could_move: bool
}

impl Character
{
	pub fn new(properties: CharacterProperties) -> Self
	{
		let anatomy = properties.anatomy;

		Self{entity: Entity::new(properties.entity_properties), anatomy, could_move: true}
	}

    pub fn move_speed(&self) -> Option<f32>
    {
        self.speed().map(|speed| speed / self.physical_ref().mass)
    }

	pub fn speed(&self) -> Option<f32>
	{
		self.anatomy.speed()
	}

    pub fn set_speed(&mut self, speed: f32)
    {
        self.anatomy.set_speed(speed);
    }

    fn this_needs_redraw(&mut self) -> bool
    {
        let current_move = self.speed().is_some();
        let move_changed = current_move != self.could_move;

        self.could_move = current_move;

        move_changed
    }
}

#[macro_export]
macro_rules! forward_damageable
{
    ($name:ident, $child_name:ident) =>
    {
        use $crate::common::{Damageable, Damage};

        impl Damageable for $name
        {
            fn damage(&mut self, damage: Damage)
            {
                self.$child_name.damage(damage);
            }
        }
    }
}

forward_damageable!{Character, anatomy}
basic_entity_forward!{Character, entity}

impl DrawableEntity for Character
{
	fn texture(&self) -> Option<&str>
	{
        self.entity.texture()
	}

    fn needs_redraw(&mut self) -> bool
    {
        self.this_needs_redraw() || self.entity.needs_redraw()
    }
}
