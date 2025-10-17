use std::{
    fs::File,
    path::Path
};

use serde::Deserialize;

use yanyaengine::Assets;

use crate::common::{
    with_error,
    some_or_value,
    generic_info::*,
    render_info::ZLevel,
    world::DirectionsGroup
};


#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FurnitureInfoRaw
{
    name: String,
    z: Option<ZLevel>,
    container: Option<bool>,
    attached: Option<bool>,
    colliding: Option<bool>,
    symmetry: Option<Symmetry>,
    hitbox: Option<f32>,
    health: Option<f32>
}

type FurnituresInfoRaw = Vec<FurnitureInfoRaw>;

define_info_id!{FurnitureId}

pub struct FurnitureInfo
{
    pub name: String,
    pub z: ZLevel,
    pub container: bool,
    pub attached: bool,
    pub colliding: bool,
    pub textures: DirectionsGroup<Sprite>,
    pub hitbox: Option<f32>,
    pub health: f32
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
        let t = |suffix|
        {
            load_texture(assets, textures_root, &(raw.name.clone() + suffix))
        };

        let textures = match raw.symmetry.unwrap_or(Symmetry::All)
        {
            Symmetry::None => DirectionsGroup{
                left: t("_left"),
                right: t("_right"),
                up: t("_up"),
                down: t("_down")
            },
            Symmetry::Horizontal =>
            {
                let horizontal = t("_horizontal");

                DirectionsGroup{
                    left: horizontal,
                    right: horizontal,
                    up: t("_up"),
                    down: t("_down")
                }
            },
            Symmetry::Vertical =>
            {
                let vertical = t("_vertical");

                DirectionsGroup{
                    left: t("_left"),
                    right: t("_right"),
                    up: vertical,
                    down: vertical
                }
            },
            Symmetry::Both =>
            {
                let horizontal = t("_horizontal");
                let vertical = t("_vertical");

                DirectionsGroup{
                    left: horizontal,
                    right: horizontal,
                    up: vertical,
                    down: vertical
                }
            },
            Symmetry::All => DirectionsGroup::repeat(t(""))
        };

        Self{
            name: raw.name,
            z: raw.z.unwrap_or(ZLevel::Hips),
            container: raw.container.unwrap_or(false),
            attached: raw.attached.unwrap_or(false),
            colliding: raw.colliding.unwrap_or(true),
            textures,
            hitbox: raw.hitbox,
            health: raw.health.unwrap_or(5.0)
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
        let info = some_or_value!(with_error(File::open(info.as_ref())), Self::empty());

        let furnitures: FurnituresInfoRaw = some_or_value!(with_error(serde_json::from_reader(info)), Self::empty());

        let textures_root = textures_root.as_ref();
        let furnitures: Vec<_> = furnitures.into_iter().map(|info_raw|
        {
            FurnitureInfo::from_raw(assets, textures_root, info_raw)
        }).collect();

        GenericInfo::new(furnitures)
    }
}
