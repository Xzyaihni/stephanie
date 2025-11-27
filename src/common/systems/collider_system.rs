use std::{
    borrow::Borrow,
    cell::RefCell
};

use nalgebra::Vector3;

use yanyaengine::Transform;

use crate::{
    debug_config::*,
    client::CommonTextures,
    common::{
        line_info,
        some_or_return,
        collider::*,
        render_info::*,
        watcher::*,
        ENTITY_SCALE,
        Entity,
        SpatialGrid,
        Joint,
        Loot,
        EntityInfo,
        AnyEntities,
        Parent,
        systems::damaging_system,
        physics::SLEEPING_VELOCITY,
        anatomy::FALL_VELOCITY,
        world::World,
        entity::{
            for_each_component,
            iterate_components_many_with,
            ClientEntities
        }
    }
};

pub use resolver::ContactResolver;

mod resolver;


pub fn debug_collision_bounds<T: Borrow<Collider>>(
    entities: &ClientEntities,
    colliding_info: &CollidingInfo<T>
)
{
    let mut transform = colliding_info.transform.clone();

    let (bounds, mix, sprite) = match &colliding_info.collider.borrow().kind
    {
        ColliderType::RayZ => (Some(Vector3::repeat(ENTITY_SCALE * 0.06)), None, "solid.png"),
        ColliderType::Tile(_)
        | ColliderType::Aabb
        | ColliderType::Rectangle => (None, Some(MixColor::color([0.0, 0.0, 0.0, 0.4])), "solid.png"),
        ColliderType::Circle => (None, None, "circle_transparent.png")
    };

    if let Some(OverrideTransform{
        transform: override_transform,
        override_position
    }) = &colliding_info.collider.borrow().override_transform
    {
        let position = if *override_position
        {
            override_transform.position
        } else
        {
            override_transform.position + transform.position
        };

        transform = override_transform.clone();
        transform.position = position;
    }

    if let Some(scale) = bounds
    {
        transform.scale = scale;
    }

    let entity = entities.push(true, EntityInfo{
        transform: Some(transform),
        render: Some(RenderInfo{
            object: Some(RenderObjectKind::Texture{
                name: sprite.into()
            }.into()),
            mix,
            above_world: true,
            ..Default::default()
        }),
        ..Default::default()
    });

    entities.add_watcher(entity, Watcher::simple_one_frame());
}

pub fn update_sleeping(
    entities: &ClientEntities,
    space: &SpatialGrid
)
{
    for_each_component!(entities, collider, |entity, collider: &RefCell<Collider>|
    {
        let mut collider = collider.borrow_mut();

        let inside_simulated = {
            let other_transform = some_or_return!(entities.transform(entity));

            space.inside_simulated(other_transform.position, other_transform.scale.x.hypot(other_transform.scale.y))
        };

        if !collider.sleeping && !inside_simulated
        {
            if entities.physical(entity).map(|x| x.velocity().magnitude() > SLEEPING_VELOCITY).unwrap_or(false)
            {
                return;
            }
        }

        collider.sleeping = !inside_simulated;
    });
}

pub fn update(
    entities: &mut ClientEntities,
    world: &World,
    space: &SpatialGrid,
    textures: &CommonTextures,
    loot: &Loot
) -> Vec<Contact>
{
    macro_rules! maybe_colliding_info
    {
        ($result_variable:expr, $entity:expr) =>
        {
            let mut collider = entities.collider_mut_no_change($entity).unwrap();

            {
                $result_variable = maybe_colliding_info!(with $entity, collider);
            }
        };
        (with $entity:expr, $collider:expr) =>
        {
            {
                some_or_return!(CollidingInfo::new_with(
                    Some($entity),
                    || entities.transform($entity).map(|x| x.position),
                    || entities.transform($entity).map(|x| x.clone()),
                    &mut *$collider
                ))
            }
        };
    }

    let mut contacts = Vec::new();

    crate::frame_time_this!{
        [update, update_pre, collider_system_update] -> world,
        for_each_component!(entities, collider, |entity, collider: &RefCell<Collider>|
        {
            let mut collider = collider.borrow_mut();

            collider.reset_frame();

            if !collider.sleeping && DebugConfig::is_enabled(DebugTool::CollisionBounds)
            {
                let this = maybe_colliding_info!{with entity, collider};
                debug_collision_bounds(entities, &this);
            }

            if collider.sleeping || !collider.layer.collides(&ColliderLayer::World)
            {
                return;
            }

            let mut this = maybe_colliding_info!{with entity, collider};

            let mut physical = some_or_return!(entities.physical_mut_no_change(entity));

            if physical.move_z
            {
                crate::frame_time_this!{
                    [update, update_pre, collider_system_update, world] -> z_time,
                    {
                        let next_position = physical.next_position_mut();
                        if this.collide_with_world_z(world, *next_position) && !this.collider.ghost
                        {
                            next_position.z = this.transform.position.z;

                            if let Some(mut transform) = entities.transform_mut(entity)
                            {
                                transform.position.z = this.transform.position.z;
                            }

                            let hit_velocity = physical.remove_velocity_axis(2);

                            if hit_velocity < -FALL_VELOCITY
                            {
                                let damage = ((-hit_velocity - FALL_VELOCITY) * 5.0 + 1.0).powi(2) - 1.0;
                                damaging_system::fall_damage(entities, textures, loot, entity, damage);
                            }
                        }
                    }
                };
            }

            crate::frame_time_this!{
                [update, update_pre, collider_system_update, world] -> flat_time,
                this.collide_with_world(world, &mut contacts)
            };
        })
    };

    if DebugConfig::is_enabled(DebugTool::PrintContactsCount)
    {
        eprintln!("after world: {} contacts", contacts.len());
    }

    crate::frame_time_this!{
        [update, update_pre, collider_system_update] -> collision,
        space.possible_pairs(|entity: Entity, other_entity: Entity|
        {
            let mut this;
            maybe_colliding_info!{this, entity};

            let other;
            maybe_colliding_info!{other, other_entity};

            {
                let this = &this.transform;
                let other = &other.transform;

                let this_scale = this.scale.x.hypot(this.scale.y);
                let other_scale = other.scale.x.hypot(other.scale.y);

                if (this_scale + other_scale) * 0.5 < this.position.xy().metric_distance(&other.position.xy())
                {
                    return;
                }
            }

            let before_collision_contacts = contacts.len();
            this.collide(other, |contact| contacts.push(contact));

            if DebugConfig::is_enabled(DebugTool::PrintContactsCount)
            {
                if before_collision_contacts != contacts.len()
                {
                    eprintln!("after {entity:?} x {other_entity:?}: {} contacts", contacts.len());
                }
            }
        })
    };

    if DebugConfig::is_enabled(DebugTool::PrintContactsCount)
    {
        eprintln!("after collision: {} contacts", contacts.len());
    }

    iterate_components_many_with!(
        entities,
        [joint, parent, transform],
        for_each,
        |entity, joint: &RefCell<Joint>, parent: &RefCell<Parent>, transform: &RefCell<Transform>|
        {
            let parent_position = some_or_return!(entities.transform(parent.borrow().entity())).position.xy();

            joint.borrow().add_contacts(&transform.borrow(), entity, parent_position, &mut contacts);
        });

    if DebugConfig::is_enabled(DebugTool::PrintContactsCount)
    {
        eprintln!("after joints: {} contacts", contacts.len());
    }

    if DebugConfig::is_enabled(DebugTool::DisplayCollisions)
    {
        for_each_component!(entities, collider, |entity, collider: &RefCell<Collider>|
        {
            collider.borrow().collided().iter().for_each(|collided_entity|
            {
                let pos = |x: Entity| entities.transform(x).map(|x| x.position);

                let this = some_or_return!(pos(entity));
                let other = some_or_return!(pos(*collided_entity));

                if let Some(line) = line_info(this, other, 0.005, [0.0, 0.0, 1.0])
                {
                    let entity = entities.push(true, line);
                    entities.add_watcher(entity, Watcher::simple_one_frame());
                }
            });
        });
    }

    contacts
}
