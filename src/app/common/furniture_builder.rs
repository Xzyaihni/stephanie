use nalgebra::Vector3;

use yanyaengine::Transform;

use crate::common::{
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

        let mut loot = Loot::new(self.items_info, vec!["utility"], 1.0);
        loot.create_random(&mut inventory, 1..4);

        EntityInfo{
            lazy_transform: Some(LazyTransformInfo{
                transform: Transform{
                    position: self.pos,
                    scale: Vector3::repeat(0.08),
                    ..Default::default()
                },
                ..Default::default()
            }.into()),
            render: Some(RenderInfo{
                object: Some(RenderObject::Texture{
                    name: "furniture/crate.png".to_owned()
                }),
                shape: Some(BoundingShape::Circle),
                z_level: ZLevel::Low,
                ..Default::default()
            }),
            collider: Some(ColliderInfo{
                kind: ColliderType::Aabb,
                ..Default::default()
            }.into()),
            physical: Some(PhysicalProperties{
                mass: 200.0,
                friction: 0.5,
                floating: false
            }.into()),
            inventory: Some(inventory),
            ..Default::default()
        }
    }
}
