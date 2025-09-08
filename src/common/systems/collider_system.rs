use std::cell::RefCell;

use nalgebra::Vector3;

use crate::{
    debug_config::*,
    common::{
        direction_arrow_info,
        line_info,
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
        world::{TILE_SIZE, World},
        entity::{
            for_each_component,
            ClientEntities
        }
    }
};

pub use resolver::ContactResolver;

mod resolver;


pub fn update_physics(
    entities: &mut ClientEntities,
    world: &World,
    space: &mut SpatialGrid,
    follow_target: Entity,
    dt: f32
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
                let transform = if let Some(override_transform) = $collider.override_transform.clone()
                {
                    let mut overridden = override_transform.transform;

                    if !override_transform.override_position
                    {
                        overridden.position += some_or_return!(entities.transform($entity)).position;
                    }

                    overridden
                } else
                {
                    let mut transform = some_or_return!(entities.transform($entity)).clone();

                    let kind = $collider.kind;
                    if kind == ColliderType::Aabb
                    {
                        transform.rotation = 0.0;
                    }

                    transform
                };

                CollidingInfo{
                    entity: Some($entity),
                    transform,
                    collider: &mut $collider
                }
            }
        };
    }

    let mut contacts = Vec::new();

    let follow_position = some_or_return!(entities.transform(follow_target)).position;

    let follow_z = follow_position.z;
    let follow_position = follow_position.xy();

    let mut world_flat_time = None;
    let mut world_z_time = None;

    crate::frame_time_this!{
        3, collision_system_world,
        for_each_component!(entities, collider, |entity, collider: &RefCell<Collider>|
        {
            let mut collider = collider.borrow_mut();

            if DebugConfig::is_enabled(DebugTool::CollisionBounds)
            {
                if let Some(mut transform) = entities.transform(entity).as_deref().cloned()
                {
                    let (bounds, mix, sprite) = match &collider.kind
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
                    }) = &collider.override_transform
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

                    entities.push(true, EntityInfo{
                        transform: Some(transform),
                        render: Some(RenderInfo{
                            object: Some(RenderObjectKind::Texture{
                                name: sprite.to_owned()
                            }.into()),
                            mix,
                            above_world: true,
                            ..Default::default()
                        }),
                        watchers: Some(Watchers::simple_one_frame()),
                        ..Default::default()
                    });
                }
            }

            collider.reset_frame();

            let mut this = maybe_colliding_info!{with entity, collider};

            let is_sleeping = {
                let other_position = this.transform.position;

                ((other_position.z - follow_z).abs() > TILE_SIZE * 2.5)
                    || (other_position.xy().metric_distance(&follow_position) > TILE_SIZE * 30.0)
            };

            this.collider.sleeping = is_sleeping;

            if is_sleeping
            {
                return;
            }

            let mut physical = some_or_return!(entities.physical_mut_no_change(entity));

            if let Some(mut target) = entities.target(entity)
            {
                if !world.inside_chunk(target.position.into())
                {
                    return;
                }

                physical.update(
                    &mut target,
                    |physical, transform|
                    {
                        this.collider.inverse_inertia(physical, &transform.scale)
                    },
                    dt
                );

                if DebugConfig::is_enabled(DebugTool::Velocity)
                {
                    drop(target);

                    let velocity = *physical.velocity();
                    let magnitude = velocity.magnitude();

                    if let Some(info) = direction_arrow_info(
                        entities.transform(entity).unwrap().position,
                        velocity,
                        magnitude,
                        [0.0, 0.0, 1.0]
                    )
                    {
                        entities.push(true, info);
                    }
                }
            }

            if !this.collider.layer.collides(&ColliderLayer::World)
            {
                return;
            }

            crate::time_this_additive!{
                world_flat_time,
                this.collide_with_world(world, &mut contacts)
            };

            if physical.move_z
            {
                crate::time_this_additive!{
                    world_z_time,
                    {
                        let next_position = physical.next_position_mut();
                        if this.collide_with_world_z(world, *next_position) && !this.collider.ghost
                        {
                            next_position.z = this.transform.position.z;
                            physical.remove_velocity_axis(2);
                        }
                    }
                };
            }
        })
    };

    if DebugConfig::is_enabled(DebugTool::FrameTimings)
    {
        {
            let time = world_flat_time.map(|x| x.as_micros() as f64 / 1000.0).unwrap_or(0.0);
            crate::frame_timed!(4, world_flat_time, time);
        }

        {
            let time = world_z_time.map(|x| x.as_micros() as f64 / 1000.0).unwrap_or(0.0);
            crate::frame_timed!(4, world_z_time, time);
        }
    }

    if DebugConfig::is_enabled(DebugTool::PrintContactsCount)
    {
        eprintln!("after world: {} contacts", contacts.len());
    }

    crate::frame_time_this!{
        3, collision_system_collision,
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

    for_each_component!(entities, joint, |entity, joint: &RefCell<Joint>|
    {
        let parent = some_or_return!(entities.parent(entity));
        let transform = some_or_return!(entities.transform(entity));

        let parent_position = some_or_return!(entities.transform(parent.entity())).position.xy();

        joint.borrow().add_contacts(&transform, entity, parent_position, &mut contacts);
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
                    entities.push(true, line);
                }
            });
        });
    }

    contacts
}
