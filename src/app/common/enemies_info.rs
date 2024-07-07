use std::{
    fs::File,
    path::Path
};

use serde::Deserialize;

use yanyaengine::Assets;

use crate::common::{
    pick_by_commonness,
    normalize_path,
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
struct EnemyInfoRaw
{
    name: String,
    #[serde(default)]
    hairstyle: Hairstyle<String>,
    #[serde(default)]
    anatomy: HumanAnatomyInfo,
    behavior: EnemyBehavior,
    scale: Option<f32>,
    normal: String,
    crawling: String,
    lying: String,
    hand: String,
    commonness: f32
}

type EnemiesInfoRaw = Vec<EnemyInfoRaw>;

define_info_id!{EnemyId}

pub struct EnemyInfo
{
    pub name: String,
    pub anatomy: HumanAnatomyInfo,
    pub behavior: EnemyBehavior,
    pub character: CharacterId,
    pub scale: f32,
    pub commonness: f32
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

            let name = normalize_path(path);

            assets.texture_id(&name)
        };

        let scale = raw.scale.unwrap_or(1.0) * ENTITY_SCALE;

        let character = characters_info.push(CharacterInfo{
            scale,
            hairstyle: raw.hairstyle.map(get_texture),
            normal: get_texture(raw.normal),
            crawling: get_texture(raw.crawling),
            lying: get_texture(raw.lying),
            hand: get_texture(raw.hand)
        });

        Self{
            name: raw.name,
            anatomy: raw.anatomy,
            behavior: raw.behavior,
            character,
            scale,
            commonness: raw.commonness
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

    pub fn weighted_random(&self, commonness: f64) -> EnemyId
    {
        let ids = (0..self.items().len()).map(EnemyId::from);

        pick_by_commonness(commonness, ids, |id|
        {
            self.get(id).commonness as f64
        }).unwrap()
    }
}
