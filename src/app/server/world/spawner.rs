use nalgebra::{Vector2, Vector3};

use yanyaengine::Transform;

use crate::common::{
    collider::*,
    render_info::*,
    physics::*,
    lazy_transform::*,
    EntityInfo,
    Parent,
    SpawnerTile,
    entity::{AnyEntities, ServerEntities},
    world::{TILE_SIZE, Pos3}
};


pub fn create_spawner(
    entities: &mut ServerEntities,
    pos: Pos3<f32>,
    spawner: &SpawnerTile
)
{
    match spawner
    {
        SpawnerTile::Door{width} =>
        {
            let half_tile = TILE_SIZE / 2.0;

            let mut position: Vector3<f32> = pos.into();
            position.y += half_tile;
            position.z += half_tile;

            let hinge = entities.push(false, EntityInfo{
                transform: Some(Transform{
                    position,
                    scale: Vector2::new(*width as f32 * TILE_SIZE, TILE_SIZE).xyy(),
                    ..Default::default()
                }),
                render: Some(RenderInfo{
                    object: Some(RenderObjectKind::Texture{
                        name: "placeholder.png".to_owned()
                    }.into()),
                    shape: Some(BoundingShape::Circle),
                    z_level: ZLevel::Door,
                    ..Default::default()
                }),
                saveable: Some(()),
                ..Default::default()
            });

            entities.push(false, EntityInfo{
                lazy_transform: Some(LazyTransformInfo{
                    rotation: Rotation::Ignore,
                    scaling: Scaling::Ignore,
                    transform: Transform{
                        position: Vector3::new(0.5, 0.0, 0.0),
                        scale: Vector2::new(1.0, 0.3).xyy(),
                        ..Default::default()
                    },
                    ..Default::default()
                }.into()),
                render: Some(RenderInfo{
                    object: Some(RenderObjectKind::Texture{
                        name: "furniture/metal_door_wide.png".to_owned()
                    }.into()),
                    shape: Some(BoundingShape::Circle),
                    z_level: ZLevel::Door,
                    ..Default::default()
                }),
                collider: Some(ColliderInfo{
                    kind: ColliderType::Rectangle,
                    layer: ColliderLayer::Door,
                    target_non_lazy: true,
                    ..Default::default()
                }.into()),
                physical: Some(PhysicalProperties{
                    mass: 40.0 * *width as f32,
                    floating: true,
                    fixed: PhysicalFixed{
                        position: true
                    },
                    ..Default::default()
                }.into()),
                parent: Some(Parent::new(hinge, true)),
                saveable: Some(()),
                ..Default::default()
            });
        }
    }
}
