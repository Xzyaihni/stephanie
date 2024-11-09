use std::f32;

use nalgebra::{Vector2, Vector3};

use yanyaengine::Transform;

use crate::common::{
    rotate_point_z_3d,
    collider::*,
    render_info::*,
    physics::*,
    lazy_transform::*,
    joint::*,
    Occluder,
    EntityInfo,
    Parent,
    SpawnerTile,
    entity::{AnyEntities, ServerEntities},
    world::{TILE_SIZE, Pos3, TileRotation}
};


pub fn create_spawner(
    entities: &mut ServerEntities,
    pos: Pos3<f32>,
    rotation: TileRotation,
    spawner: &SpawnerTile
)
{
    match spawner
    {
        SpawnerTile::Door{width} =>
        {
            let offset_inside = 0.15;
            let half_tile = TILE_SIZE / 2.0;

            let rotation = rotation.to_angle() - f32::consts::FRAC_PI_2;

            let mut position = Vector3::from(pos) + Vector3::repeat(half_tile);
            position += rotate_point_z_3d(
                Vector3::new(-(TILE_SIZE / 2.0 + TILE_SIZE * offset_inside), 0.0, 0.0),
                rotation
            );

            let hinge = entities.push(false, EntityInfo{
                transform: Some(Transform{
                    position,
                    scale: Vector3::repeat(TILE_SIZE),
                    rotation,
                    ..Default::default()
                }),
                saveable: Some(()),
                ..Default::default()
            });

            let texture = match width
            {
                1 => "furniture/metal_door.png",
                2 => "furniture/metal_door_wide.png",
                x => panic!("invalid door width: {x}")
            };

            entities.push(false, EntityInfo{
                lazy_transform: Some(LazyTransformInfo{
                    scaling: Scaling::Ignore,
                    transform: Transform{
                        position: rotate_point_z_3d(
                            Vector3::new((0.5 * *width as f32) + offset_inside / 2.0, 0.0, 0.0),
                            rotation
                        ),
                        scale: Vector2::new(1.0 * *width as f32 + offset_inside, 0.3).xyx(),
                        ..Default::default()
                    },
                    inherit_rotation: false,
                    ..Default::default()
                }.into()),
                render: Some(RenderInfo{
                    object: Some(RenderObjectKind::Texture{
                        name: texture.to_owned()
                    }.into()),
                    shadow_visible: true,
                    z_level: ZLevel::Door,
                    ..Default::default()
                }),
                collider: Some(ColliderInfo{
                    kind: ColliderType::Rectangle,
                    layer: ColliderLayer::Door,
                    ..Default::default()
                }.into()),
                physical: Some(PhysicalProperties{
                    inverse_mass: (10.0 * *width as f32).recip(),
                    restitution: 0.0,
                    floating: true,
                    move_z: false,
                    ..Default::default()
                }.into()),
                parent: Some(Parent::new(hinge, true)),
                saveable: Some(()),
                occluder: Some(Occluder::Door),
                joint: Some(Joint::Hinge(HingeJoint{
                    origin: Vector3::new(-0.5, 0.0, 0.0),
                    angle_limit: Some(HingeAngleLimit{
                        base: rotation,
                        distance: f32::consts::FRAC_PI_2 * 0.9
                    })
                })),
                ..Default::default()
            });
        }
    }
}
