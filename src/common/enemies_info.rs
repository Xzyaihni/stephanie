use std::{
    fs::File,
    path::{Path, PathBuf}
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
    anatomy::HumanAnatomyInfo,
    enemy::EnemyBehavior
};


#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EnemyInfoRaw
{
    name: String,
    #[serde(default)]
    hairstyle: Hairstyle<String>,
    #[serde(default)]
    anatomy: HumanAnatomyInfo,
    behavior: Option<EnemyBehavior>,
    scale: Option<f32>,
    normal: Option<String>,
    crawling: Option<String>,
    lying: Option<String>,
    hand: Option<String>
}

type EnemiesInfoRaw = Vec<EnemyInfoRaw>;

define_info_id!{EnemyId}

pub struct EnemyInfo
{
    pub name: String,
    pub anatomy: HumanAnatomyInfo,
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
        let get_texture = |default_name: &str, texture: Option<String>|
        {
            texture.map(|x| load_texture(assets, textures_root, &x))
                .unwrap_or_else(||
                {
                    let name = PathBuf::from(&raw.name).join(default_name);
                    let name = name.to_string_lossy().into_owned();

                    load_texture(assets, textures_root, &name)
                })
        };

        let scale = raw.scale.unwrap_or(1.0) * ENTITY_SCALE;

        let character = characters_info.push(CharacterInfo{
            scale,
            hairstyle: raw.hairstyle.map(|x| load_texture(assets, textures_root, &x)),
            normal: get_texture("body", raw.normal),
            crawling: get_texture("crawling", raw.crawling),
            lying: get_texture("lying", raw.lying),
            hand: get_texture("hand", raw.hand)
        });

        Self{
            name: raw.name,
            anatomy: raw.anatomy,
            behavior: raw.behavior.unwrap_or(EnemyBehavior::Melee),
            character,
            scale
        }
    }
}

pub type EnemiesInfo = GenericInfo<EnemyId, EnemyInfo>;

impl EnemiesInfo
{
    pub fn empty() -> Self
    {
        GenericInfo::new(Vec::new())
    }

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
