use serde::Deserialize;

use yanyaengine::Assets;

use crate::common::{
    ItemId,
    ItemsInfo,
    generic_info::{define_info_id, Sprite}
};


define_info_id!{CharacterId}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub enum Hairstyle<T=Sprite>
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
    pub hand: ItemId,
    pub hairstyle: Hairstyle,
    pub normal: Sprite,
    pub crawling: Sprite,
    pub lying: Sprite
}

impl CharacterInfo
{
    pub fn player(
        assets: &Assets,
        items_info: &ItemsInfo
    ) -> Self
    {
        let f = |texture|
        {
            Sprite::new(assets, texture)
        };

        Self{
            hand: items_info.id("hand"),
            hairstyle: Hairstyle::Pons(f(assets.texture_id("player/pon.png"))),
            normal: f(assets.texture_id("player/body.png")),
            crawling: f(assets.texture_id("player/crawling.png")),
            lying: f(assets.texture_id("player/lying.png"))
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
