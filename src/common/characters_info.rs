use std::path::PathBuf;

use nalgebra::Vector2;

use serde::Deserialize;

use yanyaengine::TextureId;

use crate::common::{
    lazy_transform::*,
    ItemId,
    generic_info::{define_info_id, Sprite}
};


define_info_id!{CharacterId}

pub const CHARACTER_DEFORMATION: Deformation = Deformation::Stretch(
    StretchDeformation{
        animation: ValueAnimation::EaseOut(1.1),
        limit: 1.3,
        onset: 0.5,
        strength: 0.2
    }
);

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct CharacterSprite<T=Sprite>
{
    #[serde(default)]
    pub offset: Vector2<i8>,
    pub sprite: T
}

impl<T> CharacterSprite<T>
{
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> CharacterSprite<U>
    {
        CharacterSprite{
            offset: self.offset,
            sprite: f(self.sprite)
        }
    }
}

pub type HairSprite<T> = CharacterSprite<T>;

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct CharacterSprites<T=CharacterSprite<Sprite>>
{
    pub base: T,
    pub crawling: T,
    pub lying: T
}

impl Default for CharacterSprites<&'static str>
{
    fn default() -> Self
    {
        Self{
            base: "normal",
            crawling: "crawling",
            lying: "lying"
        }
    }
}

impl<T> CharacterSprites<T>
{
    pub fn map<U>(self, mut f: impl FnMut(T) -> U) -> CharacterSprites<U>
    {
        CharacterSprites{
            base: f(self.base),
            crawling: f(self.crawling),
            lying: f(self.lying)
        }
    }
}

pub type BaseHair<T> = CharacterSprites<T>;

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub enum HairAccessory<T=Sprite>
{
    Pons{left: BaseHair<Vector2<i8>>, right: BaseHair<Vector2<i8>>, value: T}
}

impl<T> HairAccessory<T>
{
    pub fn map<U>(self, mut f: impl FnMut(T) -> U) -> HairAccessory<U>
    {
        match self
        {
            Self::Pons{left, right, value} => HairAccessory::Pons{left, right, value: f(value)}
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize)]
pub struct Hairstyle<T=Sprite>
{
    pub base: Option<BaseHair<HairSprite<T>>>,
    pub accessory: Option<HairAccessory<T>>
}

impl<T> Hairstyle<T>
{
    pub fn map<U>(self, mut f: impl FnMut(T) -> U) -> Hairstyle<U>
    {
        let base = self.base.map(|x|
        {
            BaseHair{
                base: x.base.map(&mut f),
                crawling: x.crawling.map(&mut f),
                lying: x.lying.map(&mut f)
            }
        });

        Hairstyle{
            base,
            accessory: self.accessory.map(|x| x.map(&mut f))
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
