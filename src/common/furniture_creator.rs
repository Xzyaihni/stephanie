use nalgebra::Vector3;

use yanyaengine::Transform;

use crate::common::{
    ENTITY_SCALE,
    lazy_transform::*,
    collider::*,
    render_info::*,
    rotate_point_z_3d,
    world::{TILE_SIZE, TileRotation},
    PhysicalProperties,
    EntityInfo,
    Loot,
    FurnituresInfo,
    FurnitureId,
    Inventory
};


pub fn create(
    furnitures_info: &FurnituresInfo,
    loot: &Loot,
    id: FurnitureId,
    rotation: TileRotation,
    pos: Vector3<f32>
) -> EntityInfo
{
    let info = furnitures_info.get(id);

    let name = info.name.clone();

    let shift = {
        let rotation = -rotation.to_angle();

        rotate_point_z_3d(Vector3::new(0.0, -(TILE_SIZE - info.scale.y) / 2.0, 0.0), rotation)
    };

    let inventory = info.container.then(||
    {
        let mut inventory = Inventory::new();
        loot.create(&name).into_iter().for_each(|item| { inventory.push(item); });

        inventory
    });

    let rotation = match rotation
    {
        TileRotation::Left => TileRotation::Right,
        x => x
    };

    EntityInfo{
        lazy_transform: Some(LazyTransformInfo{
            transform: Transform{
                position: pos + shift,
                scale: Vector3::new(info.scale.x, info.scale.y, ENTITY_SCALE),
                rotation: -rotation.to_angle(),
                ..Default::default()
            },
            ..Default::default()
        }.into()),
        render: Some(RenderInfo{
            object: Some(RenderObjectKind::TextureId{
                id: info.texture
            }.into()),
            shadow_visible: true,
            z_level: ZLevel::Hips,
            ..Default::default()
        }),
        named: Some(name),
        collider: Some(ColliderInfo{
            kind: ColliderType::Rectangle,
            ..Default::default()
        }.into()),
        physical: Some(PhysicalProperties{
            inverse_mass: 100.0_f32.recip(),
            sleeping: true,
            ..Default::default()
        }.into()),
        inventory,
        saveable: Some(()),
        ..Default::default()
    }
}
