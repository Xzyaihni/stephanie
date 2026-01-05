use std::{
    fs::File,
    path::{Path, PathBuf}
};

use nalgebra::{vector, Vector2};

use serde::Deserialize;

use yanyaengine::Assets;

use crate::common::{
    with_error,
    some_or_value,
    generic_info::*,
    characters_info::*,
    ItemsInfo,
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
    face: Option<String>,
    lying_face_offset: Option<Vector2<i8>>,
    behavior: Option<EnemyBehavior>,
    body: Option<String>,
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
        let hand = raw.hand.and_then(|x|
        {
            let info = items_info.get_id(&x);

            if info.is_none()
            {
                eprintln!("item named `{x}` not found, using default hand");
            }

            info
        }).unwrap_or_else(|| items_info.id("zob hand"));

        let face = CharacterFace::load_at(raw.face.unwrap_or_else(|| raw.name.clone()).into(), |name|
        {
            load_texture(assets, textures_root, &name.to_string_lossy()).id
        });

        let body_part = |name|
        {
            let path = raw.body.as_ref().map(|body| PathBuf::from(body).join(name))
                .unwrap_or_else(|| PathBuf::from(raw.name.clone()).join(name));

            load_texture(assets, textures_root, &path.to_string_lossy())
        };

        let character = characters_info.push(CharacterInfo{
            hand,
            hairstyle: raw.hairstyle.map(|x| load_texture(assets, textures_root, &x)),
            face,
            lying_face_offset: raw.lying_face_offset.unwrap_or(vector![-6, 0]),
            normal: body_part("body"),
            crawling: body_part("crawling"),
            lying: body_part("lying")
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
