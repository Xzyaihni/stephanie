use nalgebra::Vector3;

use yanyaengine::Transform;

use crate::common::{
    ENTITY_SCALE,
    lazy_transform::*,
    collider::*,
    render_info::*,
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
    pos: Vector3<f32>
) -> EntityInfo
{
    let info = furnitures_info.get(id);

    let name = info.name.clone();

    let mut inventory = Inventory::new();
    loot.create(&name).for_each(|item| { inventory.push(item); });

    EntityInfo{
        lazy_transform: Some(LazyTransformInfo{
            transform: Transform{
                position: pos,
                scale: Vector3::repeat(ENTITY_SCALE * 0.8),
                ..Default::default()
            },
            ..Default::default()
        }.into()),
        named: Some(name),
        render: Some(RenderInfo{
            object: Some(RenderObjectKind::Texture{
                name: "furniture/crate.png".to_owned()
            }.into()),
            shadow_visible: true,
            z_level: ZLevel::Hips,
            ..Default::default()
        }),
        collider: Some(ColliderInfo{
            kind: ColliderType::Rectangle,
            ..Default::default()
        }.into()),
        physical: Some(PhysicalProperties{
            inverse_mass: 100.0_f32.recip(),
            ..Default::default()
        }.into()),
        inventory: Some(inventory),
        saveable: Some(()),
        ..Default::default()
    }
}
