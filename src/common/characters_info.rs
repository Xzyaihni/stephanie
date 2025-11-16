use std::path::PathBuf;

use nalgebra::Vector2;

use serde::Deserialize;

use yanyaengine::{TextureId, Assets};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FacialExpression
{
    Normal,
    Hurt,
    Sick,
    Dead
}

#[derive(Debug, Clone, PartialEq)]
pub struct CharacterFace<T=TextureId>
{
    pub normal: T,
    pub dead: T,
    pub hurt: T,
    pub sick: T,
    pub eyes_normal: T,
    pub eyes_closed: T
}

impl Default for CharacterFace<&'static str>
{
    fn default() -> Self
    {
        Self{
            normal: "face_normal",
            dead: "face_dead",
            hurt: "face_hurt",
            sick: "face_sick",
            eyes_normal: "eyes_normal",
            eyes_closed: "eyes_closed"
        }
    }
}

impl<T> CharacterFace<T>
{
    pub fn map<U>(self, mut f: impl FnMut(T) -> U) -> CharacterFace<U>
    {
        CharacterFace{
            normal: f(self.normal),
            dead: f(self.dead),
            hurt: f(self.hurt),
            sick: f(self.sick),
            eyes_normal: f(self.eyes_normal),
            eyes_closed: f(self.eyes_closed)
        }
    }
}

impl CharacterFace
{
    pub fn load_at(parent: PathBuf, mut f: impl FnMut(PathBuf) -> TextureId) -> Self
    {
        CharacterFace::<&'static str>::default().map(|name|
        {
            f(parent.join(name))
        })
    }
}

pub struct CharacterInfo
{
    pub hand: ItemId,
    pub hairstyle: Hairstyle,
    pub face: CharacterFace,
    pub lying_face_offset: Vector2<i8>,
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

        let s = |name: &str|
        {
            f(assets.texture_id(name))
        };

        Self{
            hand: items_info.id("hand"),
            hairstyle: Hairstyle::Pons(s("player/pon.png")),
            face: CharacterFace::load_at("player".into(), |mut path| { path.set_extension("png"); s(&path.to_string_lossy()).id }),
            lying_face_offset: Vector2::zeros(),
            normal: s("player/body.png"),
            crawling: s("player/crawling.png"),
            lying: s("player/lying.png")
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
