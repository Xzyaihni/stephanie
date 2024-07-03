use std::{
    fs::File,
    path::Path
};

use serde::Deserialize;

use yanyaengine::Assets;

use crate::common::{
    ENTITY_SCALE,
    generic_info::*,
    Hairstyle,
    CharactersInfo,
    CharacterInfo,
    CharacterId,
    enemy::EnemyBehavior
};


#[derive(Deserialize)]
struct EnemyInfoRaw
{
    name: String,
    behavior: EnemyBehavior,
    scale: Option<f32>,
    normal: String,
    lying: String,
    hand: String
}

type EnemiesInfoRaw = Vec<EnemyInfoRaw>;

define_info_id!{EnemyId}

pub struct EnemyInfo
{
    pub name: String,
    pub behavior: EnemyBehavior,
    pub character: CharacterId,
    pub scale: f32
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
        characters_info: &mut CharactersInfo,
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

        let scale = raw.scale.unwrap_or(1.0) * ENTITY_SCALE;

        let character = characters_info.push(CharacterInfo{
            scale,
            hairstyle: Hairstyle::None,
            normal: get_texture(raw.normal),
            lying: get_texture(raw.lying),
            hand: get_texture(raw.hand)
        });

        Self{
            name: raw.name,
            behavior: raw.behavior,
            character,
            scale
        }
    }
}

pub type EnemiesInfo = GenericInfo<EnemyId, EnemyInfo>;

impl EnemiesInfo
{
    pub fn parse(
        assets: &Assets,
        characters_info: &mut CharactersInfo,
        textures_root: impl AsRef<Path>,
        info: impl AsRef<Path>
    ) -> Self
    {
        let info = File::open(info.as_ref()).unwrap();

        let enemies: EnemiesInfoRaw = serde_json::from_reader(info).unwrap();

        let textures_root = textures_root.as_ref();
        let enemies: Vec<_> = enemies.into_iter().map(|info_raw|
        {
            EnemyInfo::from_raw(assets, characters_info, textures_root, info_raw)
        }).collect();

        GenericInfo::new(enemies)
    }
}
