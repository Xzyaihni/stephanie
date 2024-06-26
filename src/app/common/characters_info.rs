use yanyaengine::{Assets, TextureId};

use crate::common::{
    ENTITY_SCALE,
    generic_info::define_info_id
};


define_info_id!{CharacterId}

pub struct CharacterInfo
{
    pub scale: f32,
    pub normal: TextureId,
    pub lying: TextureId
}

impl CharacterInfo
{
    pub fn player(assets: &Assets) -> Self
    {
        Self{
            scale: ENTITY_SCALE,
            normal: assets.texture_id("player/hair.png"),
            lying: assets.texture_id("player/lying.png")
        }
    }
}

#[derive(Default)]
pub struct CharactersInfo
{
    items: Vec<CharacterInfo>
}

impl CharactersInfo
{
    pub fn new() -> Self
    {
        Self::default()
    }

    pub fn push(&mut self, item: CharacterInfo) -> CharacterId
    {
        let id = self.items.len();

        self.items.push(item);

        CharacterId(id)
    }

    pub fn get(&self, id: CharacterId) -> &CharacterInfo
    {
        &self.items[id.0]
    }
}
