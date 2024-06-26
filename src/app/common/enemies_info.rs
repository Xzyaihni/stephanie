use std::{
    fs::File,
    path::Path
};

use serde::{Serialize, Deserialize};

use yanyaengine::{Assets, TextureId};

use crate::common::{
    ENTITY_SCALE,
    generic_info::*,
    enemy::EnemyBehavior
};


#[derive(Deserialize)]
struct EnemyInfoRaw
{
    name: String,
    behavior: EnemyBehavior,
    scale: Option<f32>,
    normal: String,
    lying: String
}

type EnemiesInfoRaw = Vec<EnemyInfoRaw>;

define_info_id!{EnemyId}

pub struct EnemyInfo
{
    pub name: String,
    pub behavior: EnemyBehavior,
    pub scale: f32,
    pub normal: TextureId,
    pub lying: TextureId
}

impl GenericItem for EnemyInfo
{
    fn name(&self) -> String
    {
        self.name.clone()
    }
}

impl EnemyInfo
{
    fn from_raw(
        assets: &Assets,
        textures_root: &Path,
        raw: EnemyInfoRaw
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
            behavior: raw.behavior,
            scale: raw.scale.unwrap_or(1.0) * ENTITY_SCALE,
            normal: get_texture(raw.normal),
            lying: get_texture(raw.lying)
        }
    }
}

pub type EnemiesInfo = GenericInfo<EnemyId, EnemyInfo>;

impl EnemiesInfo
{
    pub fn parse(
        assets: &Assets,
        textures_root: impl AsRef<Path>,
        info: impl AsRef<Path>
    ) -> Self
    {
        let info = File::open(info.as_ref()).unwrap();

        let enemies: EnemiesInfoRaw = serde_json::from_reader(info).unwrap();

        let textures_root = textures_root.as_ref();
        let enemies: Vec<_> = enemies.into_iter().map(|info_raw|
        {
            EnemyInfo::from_raw(assets, textures_root, info_raw)
        }).collect();

        GenericInfo::new(enemies)
    }
}
