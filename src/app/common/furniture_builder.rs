use nalgebra::Vector3;

use yanyaengine::Transform;

use crate::common::{
    ENTITY_SCALE,
    lazy_transform::*,
    collider::*,
    render_info::*,
    PhysicalProperties,
    EntityInfo,
    ItemsInfo,
    Loot,
    Inventory
};


pub struct FurnitureBuilder<'a>
{
    items_info: &'a ItemsInfo,
    pos: Vector3<f32>
}

impl<'a> FurnitureBuilder<'a>
{
    pub fn new(
        items_info: &'a ItemsInfo,
        pos: Vector3<f32>
    ) -> Self
    {
        Self{items_info, pos}
    }

    pub fn build(self) -> EntityInfo
    {
        let mut inventory = Inventory::new();

        let mut loot = Loot::new(self.items_info, vec!["trash", "utility"], 1.0);
        loot.create_random(&mut inventory, 1..4);

        EntityInfo{
            lazy_transform: Some(LazyTransformInfo{
                transform: Transform{
                    position: self.pos,
                    scale: Vector3::repeat(ENTITY_SCALE * 0.8),
                    ..Default::default()
                },
                ..Default::default()
            }.into()),
            named: Some("crate".to_owned()),
            render: Some(RenderInfo{
                object: Some(RenderObjectKind::Texture{
                    name: "furniture/crate.png".to_owned()
                }.into()),
                shape: Some(BoundingShape::Circle),
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
}
