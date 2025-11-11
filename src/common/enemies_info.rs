use std::{
    fs::File,
    path::{Path, PathBuf}
};

use serde::Deserialize;

use yanyaengine::Assets;

use crate::common::{
    with_error,
    some_or_value,
    generic_info::*,
    Hairstyle,
    ItemsInfo,
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
    pub character: CharacterId
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
        items_info: &ItemsInfo,
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

        let hand = raw.hand.and_then(|x|
        {
            let info = items_info.get_id(&x);

            if info.is_none()
            {
                eprintln!("item named `{x}` not found, using default hand");
            }

            info
        }).unwrap_or_else(|| items_info.id("hand"));

        let character = characters_info.push(CharacterInfo{
            hand,
            hairstyle: raw.hairstyle.map(|x| load_texture(assets, textures_root, &x)),
            normal: get_texture("body", raw.normal),
            crawling: get_texture("crawling", raw.crawling),
            lying: get_texture("lying", raw.lying)
        });

        Self{
            name: raw.name,
            anatomy: raw.anatomy,
            behavior: raw.behavior.unwrap_or(EnemyBehavior::Melee),
            character
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
        items_info: &ItemsInfo,
        textures_root: PathBuf,
        info: PathBuf
    ) -> Self
    {
        let info = some_or_value!(with_error(File::open(info)), Self::empty());

        let enemies: EnemiesInfoRaw = some_or_value!(with_error(serde_json::from_reader(info)), Self::empty());

        let enemies: Vec<_> = enemies.into_iter().map(|info_raw|
        {
            EnemyInfo::from_raw(assets, characters_info, items_info, &textures_root, info_raw)
        }).collect();

        GenericInfo::new(enemies)
    }
}
