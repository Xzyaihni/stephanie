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
    with_z,
    some_or_return,
    generic_info::*,
    render_info::*,
    collider::*,
    physics::*,
    lazy_transform::*,
    Parent,
    Entity,
    EntityInfo,
    AnyEntities,
    Transform,
    world::DirectionsGroup,
    entity::ClientEntities
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

    pub fn update_furniture(entities: &ClientEntities, entity: Entity)
    {
        if !entities.render_exists(entity) && !entities.in_flight().render_exists(entity)
        {
            let id = some_or_return!(entities.furniture(entity));
            let info = entities.infos().furnitures_info.get(*id);

            let ids = info.textures;

            let mut setter = entities.lazy_setter.borrow_mut();

            let render = RenderInfo{
                object: Some(RenderObjectKind::TextureRotating{ids, offset: info.collision}.into()),
                shadow_visible: true,
                z_level: ZLevel::Hips,
                ..Default::default()
            };

            setter.set_named_no_change(entity, Some(info.name.clone()));

            if info.collision.is_some()
            {
                let aspect = info.scale / info.scale.min();

                let scale = with_z(aspect, 1.0);

                entities.push(true, EntityInfo{
                    render: Some(render),
                    lazy_transform: Some(LazyTransformInfo{
                        transform: Transform{
                            scale,
                            ..Default::default()
                        },
                        ..Default::default()
                    }.into()),
                    parent: Some(Parent::new(entity, true)),
                    ..Default::default()
                });
            } else
            {
                setter.set_render_no_change(entity, Some(render));
            }

            setter.set_collider_no_change(entity, Some(ColliderInfo{
                kind: ColliderType::Rectangle,
                ..Default::default()
            }.into()));

            setter.set_physical_no_change(entity, Some(PhysicalProperties{
                inverse_mass: 100.0_f32.recip(),
                sleeping: true,
                ..Default::default()
            }.into()));
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
        let info = File::open(info.as_ref()).unwrap();

        let furnitures: FurnituresInfoRaw = serde_json::from_reader(info).unwrap();

        let textures_root = textures_root.as_ref();
        let furnitures: Vec<_> = furnitures.into_iter().map(|info_raw|
        {
            FurnitureInfo::from_raw(assets, textures_root, info_raw)
        }).collect();

        GenericInfo::new(furnitures)
    }
}
