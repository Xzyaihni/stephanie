use std::cell::RefCell;

use yanyaengine::Transform;

use nalgebra::Vector3;

use crate::{
    debug_config::*,
    common::{
        some_or_return,
        collider::*,
        render_info::*,
        watcher::*,
        ENTITY_SCALE,
        Entity,
        SpatialGrid,
        Joint,
        EntityInfo,
        AnyEntities,
        world::World,
        entity::{
            for_each_component,
            ClientEntities
        }
    }
};

pub use resolver::ContactResolver;

mod resolver;


pub fn update(
    entities: &mut ClientEntities,
    world: &World,
    space: &mut SpatialGrid
) -> Vec<Contact>
{
    macro_rules! maybe_colliding_info
    {
        ($result_variable:expr, $entity:expr) =>
        {
            let mut collider = entities.collider_mut_no_change($entity).unwrap();
            {
                let mut transform = some_or_return!(entities.transform($entity)).clone();

                let kind = collider.kind;
                if kind == ColliderType::Aabb
                {
                    transform.rotation = 0.0;
                }

                if let Some(scale) = collider.scale
                {
                    transform.scale = scale;
                }

                $result_variable = CollidingInfo{
                    entity: Some($entity),
                    transform,
                    collider: &mut collider
                };
            }
        }
    }

    for_each_component!(entities, collider, |entity, collider: &RefCell<Collider>|
    {
        if DebugConfig::is_enabled(DebugTool::CollisionBounds)
        {
            if let Some(transform) = entities.transform(entity)
            {
                let collider = collider.borrow_mut();
                let (bounds, mix, sprite) = match &collider.kind
                {
                    ColliderType::RayZ => (Some(Vector3::repeat(ENTITY_SCALE * 0.06)), None, "solid.png"),
                    ColliderType::Tile(_)
                    | ColliderType::Aabb
                    | ColliderType::Rectangle => (None, Some(MixColor::color([0.0, 0.0, 0.0, 0.4])), "solid.png"),
                    ColliderType::Circle => (None, None, "circle_transparent.png")
                };

                let scale = bounds.unwrap_or_else(|| collider.scale.unwrap_or(transform.scale));
                entities.push(true, EntityInfo{
                    transform: Some(Transform{
                        scale,
                        position: transform.position,
                        rotation: transform.rotation,
                        ..Default::default()
                    }),
                    render: Some(RenderInfo{
                        object: Some(RenderObjectKind::Texture{
                            name: sprite.to_owned()
                        }.into()),
                        mix,
                        z_level: ZLevel::highest(),
                        ..Default::default()
                    }),
                    watchers: Some(Watchers::simple_one_frame()),
                    ..Default::default()
                });
            }
        }

        collider.borrow_mut().reset_frame();
    });

    let mut contacts = Vec::new();

    crate::frame_time_this!{
        collision_system_collision,
        space.possible_pairs(|entity: Entity, other_entity: Entity|
        {
            let mut this;
            maybe_colliding_info!{this, entity};

            let other;
            maybe_colliding_info!{other, other_entity};

            this.collide(other, |contact| contacts.push(contact));
        })
    };

    crate::frame_time_this!{
        collision_system_world,
        for_each_component!(entities, collider, |entity, _collider|
        {
            let mut this;
            maybe_colliding_info!{this, entity};

            if !this.collider.layer.collides(&ColliderLayer::World)
            {
                return;
            }

            if DebugConfig::is_enabled(DebugTool::CollisionWorldBounds)
            {
                entities.push(true, EntityInfo{
                    transform: Some(Transform{
                        position: this.transform.position,
                        scale: this.bounds() * 2.0,
                        ..Default::default()
                    }),
                    render: Some(RenderInfo{
                        object: Some(RenderObjectKind::Texture{
                            name: "solid.png".to_owned()
                        }.into()),
                        z_level: ZLevel::highest(),
                        ..Default::default()
                    }),
                    watchers: Some(Watchers::simple_one_frame()),
                    ..Default::default()
                });
            }

            this.collide_with_world(world, &mut contacts);

            let mut physical = some_or_return!(entities.physical_mut_no_change(entity));
            let next_position = physical.next_position_mut();

            if this.collide_with_world_z(world, *next_position)
            {
                next_position.z = this.transform.position.z;
                physical.remove_velocity_axis(2);
            }
        })
    };

    for_each_component!(entities, joint, |entity, joint: &RefCell<Joint>|
    {
        let parent = some_or_return!(entities.parent(entity));
        let transform = some_or_return!(entities.transform(entity));

        let parent_position = some_or_return!(entities.transform(parent.entity())).position;

        joint.borrow().add_contacts(&transform, entity, parent_position, &mut contacts);
    });

    contacts
}
