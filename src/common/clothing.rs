use std::path::Path;

use serde::Deserialize;

use yanyaengine::Assets;

use crate::common::{
    Sprite,
    generic_info::load_texture,
    characters_info::CharacterSprites
};


#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum EquipSlot
{
    Head
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClothingInfoRaw
{
    slot: EquipSlot,
    armor_multiply: f32,
    armor_normal: f32
}

#[derive(Debug, Clone)]
pub struct ClothingInfo
{
    pub sprites: CharacterSprites<Sprite>,
    pub slot: EquipSlot,
    pub armor_multiply: f32,
    pub armor_normal: f32
}

impl ClothingInfo
{
    pub fn from_raw(
        assets: &Assets,
        textures_root: &Path,
        raw: ClothingInfoRaw
    ) -> Self
    {
        let sprites = CharacterSprites::<&'static str>::default().map(|state|
        {
            load_texture(assets, textures_root, state)
        });

        Self{
            sprites,
            slot: raw.slot,
            armor_multiply: raw.armor_multiply,
            armor_normal: raw.armor_normal
        }
    }
}
