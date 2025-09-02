use nalgebra::{Vector2, Vector3};

use yanyaengine::Transform;

use crate::common::{
    ENTITY_SCALE,
    with_z,
    rotate_point,
    some_or_return,
    render_info::*,
    physics::*,
    lazy_transform::*,
    collider::*,
    EntityInfo,
    Loot,
    FurnituresInfo,
    FurnitureInfo,
    FurnitureId,
    Inventory,
    AnyEntities,
    Entity,
    Parent,
    world::{TILE_SIZE, TileRotation},
    entity::ClientEntities
};


pub fn update_furniture(entities: &ClientEntities, entity: Entity)
{
    if !entities.named_exists(entity) && !entities.in_flight().named_exists(entity)
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

pub fn furniture_position(
    info: &FurnitureInfo,
    rotation: TileRotation
) -> Vector2<f32>
{
    let rotation = -rotation.to_angle();

    rotate_point(Vector2::new(0.0, -(TILE_SIZE - info.scale.y) / 2.0), rotation)
}

pub fn create(
    furnitures_info: &FurnituresInfo,
    loot: &Loot,
    id: FurnitureId,
    rotation: TileRotation,
    pos: Vector3<f32>
) -> EntityInfo
{
    let info = furnitures_info.get(id);

    let scale = info.collision.map(|_x|
    {
        let s = info.scale.min();

        Vector3::new(s, s, ENTITY_SCALE)
    }).unwrap_or_else(|| Vector3::new(info.scale.x, info.scale.y, ENTITY_SCALE));

    let position = pos + with_z(furniture_position(&info, rotation), 0.0);

    let rotation = -rotation.to_angle();

    let inventory = info.container.then(||
    {
        let mut inventory = Inventory::new();
        loot.create(&info.name).into_iter().for_each(|item| { inventory.push(item); });

        inventory
    });

    EntityInfo{
        transform: Some(Transform{
            position,
            scale,
            rotation,
            ..Default::default()
        }),
        inventory,
        furniture: Some(id),
        saveable: Some(()),
        ..Default::default()
    }
}
