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
    Inventory
};


pub fn create(
    loot: &Loot,
    pos: Vector3<f32>
) -> EntityInfo
{
    let name = "crate".to_owned();

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
        ..Default::default()
    }
}
