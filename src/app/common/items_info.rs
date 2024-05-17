use std::{
    fs::File,
    path::Path,
    collections::HashMap
};

use serde::{Serialize, Deserialize};

use yanyaengine::{Assets, TextureId};


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ItemId(usize);

#[derive(Deserialize)]
pub struct ItemInfoRaw
{
    name: String,
    texture: Option<String>
}

pub type ItemsInfoRaw = Vec<ItemInfoRaw>;

pub struct ItemInfo
{
    name: String,
    texture: Option<TextureId>
}

impl ItemInfo
{
    fn from_raw(
        assets: &Assets,
        textures_root: &Path,
        raw: ItemInfoRaw
    ) -> Self
    {
        let get_texture = |name|
        {
            let path = textures_root.join(name);
            let name = path.to_string_lossy();

            assets.texture_id(&name)
        };

        Self{
            name: raw.name,
            texture: raw.texture.map(get_texture)
        }
    }
}

pub struct ItemsInfo
{
    mapping: HashMap<String, ItemId>,
    items: Vec<ItemInfo>
}

impl ItemsInfo
{
    pub fn parse(
        assets: &Assets,
        textures_root: impl AsRef<Path>,
        info: impl AsRef<Path>
    ) -> Self
    {
        let info = File::open(info.as_ref()).unwrap();

        let items: ItemsInfoRaw = serde_json::from_reader(info).unwrap();

        let textures_root = textures_root.as_ref();
        let items: Vec<_> = items.into_iter().map(|info_raw|
        {
            ItemInfo::from_raw(assets, textures_root, info_raw)
        }).collect();

        let mapping = items.iter().enumerate().map(|(index, item)|
        {
            (item.name.clone(), ItemId(index))
        }).collect();

        Self{mapping, items}
    }

    pub fn id(&self, name: &str) -> ItemId
    {
        self.mapping[name]
    }

    pub fn get(&self, id: ItemId) -> &ItemInfo
    {
        &self.items[id.0]
    }
}