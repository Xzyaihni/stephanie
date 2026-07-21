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
    lisp::*,
    ItemsInfo,
    ColorPalette,
    scripts_container::{ScriptsContainer, ScriptIndex},
    loot::{ServerEnemyScriptsInfo, ServerScriptSingleInfo},
    anatomy::HumanAnatomyInfo,
    enemy::EnemyBehavior
};


#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct HumanAnatomyInfoRaw
{
    health: Option<f32>,
    bone: Option<f32>,
    muscle: Option<f32>,
    skin: Option<f32>,
    speed: Option<f32>,
    strength: Option<f32>
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct EnemyInfoRaw
{
    name: String,
    inherit: Option<String>,
    hairstyle: Option<Hairstyle<String>>,
    anatomy: Option<HumanAnatomyInfoRaw>,
    face: Option<String>,
    palette: Option<ColorPalette>,
    lying_face_offset: Option<Vector2<i8>>,
    behavior: Option<EnemyBehavior>,
    body: Option<String>,
    hand: Option<String>,
    on_create: Option<String>,
    on_contents: Option<String>,
    on_equip: Option<String>
}

impl EnemyInfoRaw
{
    fn combine(&self, other: &Self) -> Self
    {
        let mut this = self.clone();

        this.name = other.name.clone();

        inherit_with_fields!{
            this,
            other,
            hairstyle,
            anatomy,
            face,
            lying_face_offset,
            behavior,
            body,
            hand,
            on_create,
            on_contents,
            on_equip
        };

        this
    }
}

type EnemiesInfoRaw = Vec<EnemyInfoRaw>;

define_info_id!{EnemyId}

pub struct EnemyInfo
{
    pub name: String,
    pub anatomy: HumanAnatomyInfo,
    pub behavior: EnemyBehavior,
    pub on_create: Option<ScriptIndex>,
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
        enemy_scripts: EnemyScripts,
        scripts: &mut ScriptsContainer,
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
            hairstyle: raw.hairstyle.unwrap_or_default().map(|x| load_texture(assets, textures_root, &x)),
            face,
            palette: raw.palette,
            lying_face_offset: raw.lying_face_offset.unwrap_or(vector![-6, 0]),
            normal: body_part("body"),
            crawling: body_part("crawling"),
            lying: body_part("lying")
        });

        let on_create = {
            let f = |code| ServerScriptSingleInfo{name: raw.name.clone(), code};

            enemy_scripts.server.push(ServerEnemyScriptsInfo{
                on_contents: raw.on_contents.map(f),
                on_equip: raw.on_equip.map(f)
            });

            raw.on_create.and_then(|code|
            {
                let memory = LispMemory::new(scripts.client_primitives.clone(), 128, 1 << 10);

                let config = LispConfig{
                    memory,
                    env_variables: vec!["caller-entity".to_owned(), "player-entity".to_owned()],
                    ..Default::default()
                };

                match Lisp::new_with_config(config, &[&code])
                {
                    Ok(mut lisp) =>
                    {
                        lisp.set_source_name(0, raw.name.clone());

                        Some(scripts.push(lisp))
                    },
                    Err(err) =>
                    {
                        eprintln!("error parsing on_use for enemy `{}`: {err}", &raw.name);

                        None
                    }
                }
            })
        };

        Self{
            name: raw.name,
            anatomy: raw.anatomy.map(|x|
            {
                let health = x.health.unwrap_or(1.0);

                HumanAnatomyInfo{
                    bone: x.bone.unwrap_or(1.0) * health,
                    muscle: x.muscle.unwrap_or(1.0) * health,
                    skin: x.skin.unwrap_or(1.0) * health,
                    base_speed: x.speed.unwrap_or(1.0),
                    base_strength: x.strength.unwrap_or(1.0)
                }
            }).unwrap_or_default(),
            behavior: raw.behavior.unwrap_or(EnemyBehavior::Melee),
            on_create,
            character
        }
    }
}

pub type EnemiesInfo = GenericInfo<EnemyId, EnemyInfo>;

pub struct EnemyScripts<'a>
{
    pub server: &'a mut Vec<ServerEnemyScriptsInfo<Option<ServerScriptSingleInfo>>>
}

impl EnemiesInfo
{
    pub fn empty() -> Self
    {
        GenericInfo::new(Vec::new())
    }

    pub fn parse(
        enemy_scripts: EnemyScripts,
        scripts: &mut ScriptsContainer,
        assets: &Assets,
        characters_info: &mut CharactersInfo,
        items_info: &ItemsInfo,
        textures_root: PathBuf,
        info: PathBuf
    ) -> Self
    {
        let info = some_or_value!(with_error(File::open(info)), Self::empty());

        let mut enemies: EnemiesInfoRaw = some_or_value!(with_error(serde_json::from_reader(info)), Self::empty());

        inherit_infos(
            &mut enemies,
            |this_info| this_info.inherit.as_ref(),
            |this_info| &this_info.name,
            |a, b| a.combine(b)
        );

        let enemies: Vec<_> = enemies.into_iter().map(|info_raw|
        {
            EnemyInfo::from_raw(
                EnemyScripts{
                    server: enemy_scripts.server
                },
                scripts,
                assets,
                characters_info,
                items_info,
                &textures_root,
                info_raw
            )
        }).collect();

        GenericInfo::new(enemies)
    }
}
