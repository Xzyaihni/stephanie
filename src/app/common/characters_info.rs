use serde::Deserialize;

use yanyaengine::{Assets, TextureId};

use crate::common::{
    ENTITY_SCALE,
    generic_info::define_info_id
};


define_info_id!{CharacterId}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Hairstyle<T=TextureId>
{
    None,
    Pons(T)
}

impl<T> Default for Hairstyle<T>
{
    fn default() -> Self
    {
        Self::None
    }
}

impl<T> Hairstyle<T>
{
    pub fn map<U>(self, mut f: impl FnMut(T) -> U) -> Hairstyle<U>
    {
        match self
        {
            Self::None => Hairstyle::None,
            Self::Pons(x) => Hairstyle::Pons(f(x))
        }
    }
}

pub struct CharacterInfo
{
    pub scale: f32,
    pub hairstyle: Hairstyle,
    pub normal: TextureId,
    pub crawling: TextureId,
    pub lying: TextureId,
    pub hand: TextureId
}

impl CharacterInfo
{
    pub fn player(assets: &Assets) -> Self
    {
        Self{
            scale: ENTITY_SCALE,
            hairstyle: Hairstyle::Pons(assets.texture_id("player/pon.png")),
            normal: assets.texture_id("player/hair.png"),
            crawling: assets.texture_id("player/crawling.png"),
            lying: assets.texture_id("player/lying.png"),
            hand: assets.texture_id("player/hand.png")
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
