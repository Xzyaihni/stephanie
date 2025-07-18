use std::{
    fs::File,
    path::Path
};

use serde::Deserialize;

use yanyaengine::{Assets, TextureId};

use crate::common::generic_info::*;


#[derive(Deserialize)]
struct FurnitureInfoRaw
{
    name: String,
    texture: Option<String>
}

type FurnituresInfoRaw = Vec<FurnitureInfoRaw>;

define_info_id!{FurnitureId}

pub struct FurnitureInfo
{
    pub name: String,
    pub texture: TextureId
}

impl GenericItem for FurnitureInfo
{
    fn name(&self) -> String
    {
        self.name.clone()
    }
}

impl FurnitureInfo
{
    fn from_raw(
        assets: &Assets,
        textures_root: &Path,
        raw: FurnitureInfoRaw
    ) -> Self
    {
        let texture = raw.texture.unwrap_or_else(|| raw.name.clone());
        let texture = load_texture(assets, textures_root, &texture);

        Self{
            name: raw.name,
            texture
        }
    }
}

pub type FurnituresInfo = GenericInfo<FurnitureId, FurnitureInfo>;

impl FurnituresInfo
{
    pub fn empty() -> Self
    {
        GenericInfo::new(Vec::new())
    }

    pub fn parse(
        assets: &Assets,
        textures_root: impl AsRef<Path>,
        info: impl AsRef<Path>
    ) -> Self
    {
        let info = File::open(info.as_ref()).unwrap();

        let furnitures: FurnituresInfoRaw = serde_json::from_reader(info).unwrap();

        let textures_root = textures_root.as_ref();
        let furnitures: Vec<_> = furnitures.into_iter().map(|info_raw|
        {
            FurnitureInfo::from_raw(assets, textures_root, info_raw)
        }).collect();

        GenericInfo::new(furnitures)
    }
}
