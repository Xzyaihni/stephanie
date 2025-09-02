use std::{
    fs::File,
    path::Path
};

use serde::Deserialize;

use nalgebra::Vector2;

use yanyaengine::{Assets, TextureId};

use crate::common::{
    ENTITY_SCALE,
    ENTITY_PIXEL_SCALE,
    with_error,
    some_or_value,
    generic_info::*,
    world::DirectionsGroup
};


#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FurnitureInfoRaw
{
    name: String,
    container: Option<bool>,
    symmetry: Option<Symmetry>,
    collision: Option<f32>
}

type FurnituresInfoRaw = Vec<FurnitureInfoRaw>;

define_info_id!{FurnitureId}

pub struct FurnitureInfo
{
    pub name: String,
    pub scale: Vector2<f32>,
    pub container: bool,
    pub textures: DirectionsGroup<TextureId>,
    pub collision: Option<f32>
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

        let scale = assets.texture(textures.up).lock().size() / ENTITY_PIXEL_SCALE as f32 * ENTITY_SCALE;

        Self{
            name: raw.name,
            scale,
            container: raw.container.unwrap_or(false),
            textures,
            collision: raw.collision
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
