use nalgebra::Vector3;

use yanyaengine::Transform;

use crate::common::{
    ENTITY_SCALE,
    lazy_transform::*,
    rotate_point_z_3d,
    world::{TILE_SIZE, TileRotation},
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

    let rotation = -rotation.to_angle();

    let shift = rotate_point_z_3d(Vector3::new(0.0, -(TILE_SIZE - info.scale.y) / 2.0, 0.0), rotation);

    let inventory = info.container.then(||
    {
        let mut inventory = Inventory::new();
        loot.create(&info.name).into_iter().for_each(|item| { inventory.push(item); });

        inventory
    });

    let scale = info.collision.map(|_x|
    {
        let s = info.scale.min();

        Vector3::new(s, s, ENTITY_SCALE)
    }).unwrap_or_else(|| Vector3::new(info.scale.x, info.scale.y, ENTITY_SCALE));

    EntityInfo{
        lazy_transform: Some(LazyTransformInfo{
            transform: Transform{
                position: pos + shift,
                scale,
                rotation,
                ..Default::default()
            },
            ..Default::default()
        }.into()),
        inventory,
        furniture: Some(id),
        saveable: Some(()),
        ..Default::default()
    }
}
