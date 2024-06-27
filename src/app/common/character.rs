use serde::{Serialize, Deserialize};

use nalgebra::Vector3;

use yanyaengine::{Transform, TextureId};

use crate::common::{
    define_layers,
    render_info::*,
    CharacterId,
    CharactersInfo,
    Anatomy
};


#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpriteState
{
    Normal,
    Lying
}

fn true_fn() -> bool
{
    true
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Stateful<T>
{
    #[serde(skip, default="true_fn")]
    changed: bool,
    value: T
}

impl<T> From<T> for Stateful<T>
{
    fn from(value: T) -> Self
    {
        Self{
            changed: true,
            value
        }
    }
}

impl<T> Stateful<T>
{
    pub fn set_state(&mut self, value: T)
    where
        T: PartialEq
    {
        if self.value != value
        {
            self.value = value;
            self.changed = true;
        }
    }

    pub fn value(&self) -> &T
    {
        &self.value
    }

    pub fn dirty(&mut self)
    {
        self.changed = true;
    }

    pub fn changed(&mut self) -> bool
    {
        let state = self.changed;

        self.changed = false;

        state
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Faction
{
    Player,
    Zob
}

impl Faction
{
    pub fn aggressive(&self, other: &Self) -> bool
    {
        define_layers!{
            self, other,
            (Player, Player, false),
            (Zob, Zob, false),
            (Player, Zob, true)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character
{
    pub id: CharacterId,
    pub faction: Faction,
    sprite_state: Stateful<SpriteState>
}

impl Character
{
    pub fn new(
        id: CharacterId,
        faction: Faction
    ) -> Self
    {
        Self{
            id,
            faction,
            sprite_state: SpriteState::Normal.into()
        }
    }

    pub fn with_previous(&mut self, previous: Self)
    {
        self.sprite_state.set_state(*previous.sprite_state.value());
        self.sprite_state.dirty();
    }

    pub fn update_sprite_common(
        &mut self,
        characters_info: &CharactersInfo,
        transform: &mut Transform
    ) -> bool
    {
        if !self.sprite_state.changed()
        {
            return false;
        }

        let info = characters_info.get(self.id);
        match self.sprite_state.value()
        {
            SpriteState::Normal =>
            {
                transform.scale = Vector3::repeat(info.scale);
            },
            SpriteState::Lying =>
            {
                transform.scale = Vector3::repeat(info.scale * 1.3);
            }
        }

        true
    }

    pub fn update_sprite(
        &mut self,
        characters_info: &CharactersInfo,
        transform: &mut Transform,
        render: &mut ClientRenderInfo,
        set_sprite: impl FnOnce(&mut ClientRenderInfo, &Transform, TextureId)
    ) -> bool
    {
        if !self.update_sprite_common(characters_info, transform)
        {
            return false;
        }

        let info = characters_info.get(self.id);
        let texture = match self.sprite_state.value()
        {
            SpriteState::Normal =>
            {
                render.z_level = ZLevel::Head;

                info.normal
            },
            SpriteState::Lying =>
            {
                render.z_level = ZLevel::Feet;

                info.lying
            }
        };

        set_sprite(render, transform, texture);

        true
    }

    pub fn anatomy_changed(&mut self, anatomy: &Anatomy)
    {
        let can_move = anatomy.speed().is_some();

        let state = if can_move
        {
            SpriteState::Normal
        } else
        {
            SpriteState::Lying
        };

        self.set_sprite(state);
    }

    pub fn aggressive(&self, other: &Self) -> bool
    {
        self.faction.aggressive(&other.faction)
    }

    fn set_sprite(&mut self, state: SpriteState)
    {
        self.sprite_state.set_state(state);
    }
}
